//! 图元层：`GeoShape` → mesh（线 = 球面条带，点 = 八面体，面 = 闭合条带）。
//!
//! 表面层级约定（世界单位，米级）：球体 < 陆地填充 < 海岸线 < 高亮 < 网格 —— 具体数值各自定。

use std::time::Instant;
use bevy::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_resource::Face;
use crate::globe::EARTHR;
use crate::camera::{CameraState, CameraViwpt, MIN_ALT};
use crate::geo::{GeoPoint, GeoShape};

/// 图元材质：条带与八面体共用的渲染设置。
pub struct GlyphMatl {
    /// sRGB 分量（0..1）。
    pub rgb: [f32; 3],
    pub bias: f32,
    pub unlit: bool,
    pub cull: Option<Face>,
}

impl GlyphMatl {
    /// 单面材质。**必须保留背面剔除**：环绕方向已按"朝外"排好，否则正面会被整片剔掉。
    pub fn single(rgb: [f32; 3], unlit: bool) -> Self {
        Self { rgb, bias: 1.0, unlit, cull: Some(Face::Back) }
    }

    /// 双面材质：走向不定的线（经线 / 纬线）可能有半边被判成背面而消失。
    pub fn double(rgb: [f32; 3], unlit: bool) -> Self {
        Self { rgb, bias: 1.0, unlit, cull: None }
    }

    pub fn build(self, mats: &mut Assets<StandardMaterial>) -> Handle<StandardMaterial> {
        mats.add(StandardMaterial {
            base_color: Color::srgb(self.rgb[0], self.rgb[1], self.rgb[2]),
            cull_mode: self.cull,
            unlit: self.unlit,
            depth_bias: self.bias,
            ..default()
        })
    }
}

/// 一个图层：形状 + 样式 + 材质 + 构建状态。**作为组件挂在图层实体上**，由该图层的所有者持有。
///
/// 新增一层 = 所有者 `commands.spawn((GlyphLayer::new(style, matl), 自己的标记))`。
///
/// `require` 是**必须的**：图层的 mesh 子实体带 `GlobalTransform`/`InheritedVisibility`，
/// 若图层实体自己没有对应组件，Bevy 会对每个子实体报 B0004（"parent without ..."）。
#[derive(Component)]
#[require(Transform, Visibility)]
pub struct GlyphLayer {
    /// 待渲染的形状。改完必须调用 [`GlyphLayer::touched`]，否则不会重建。
    pub shapes: Vec<GeoShape>,
    pub style: LayerStyle,
    pub matl: Handle<StandardMaterial>,
    /// 输入版本：形状一变就 +1，用于判断"建进 mesh 的是不是最新输入"。
    pub version: u64,
    /// 本层读数。
    pub stats: LayerStats,
    /// 已建进 mesh 的输入版本与当时的相机距离；`None` = 还没建过。
    built: Option<GlyphBuilt>,
    /// 本层已挂的渲染实体（重建时整体替换）。
    items: Vec<Entity>,
    /// 跨帧增量构建状态。`None` = 没有待办。
    job: Option<LayerState>,
}

/// 上一次构建的快照。
#[derive(Clone, Copy)]
struct GlyphBuilt {
    version: u64,
    /// 构建时的视高（不是相机到球心的距离）。
    alt: f32,
}

impl GlyphLayer {
    pub fn new(style: LayerStyle, matl: Handle<StandardMaterial>) -> Self {
        Self {
            shapes: Vec::new(),
            style,
            matl,
            version: 0,
            stats: LayerStats::default(),
            built: None,
            items: Vec::new(),
            job: None,
        }
    }

    /// 形状变了：标记需要重建（所有者替换/修改 `shapes` 之后调用）。
    pub fn touched(&mut self) {
        self.version = self.version.wrapping_add(1);
    }

    /// 是否需要（重新）构建：没建过、输入变了，或**视高**变化够大。
    ///
    /// 用视高而不是相机到球心的距离：贴地时后者几乎不变（1.13 → 1.004 只有 11%），
    /// 永远够不到判据 ⇒ mesh 里那套宽度（按旧距离算的）就冻住了，越往下描边越粗。
    fn neebds(&self, alt: f32) -> bool {
        match self.built {
            None => true,
            Some(b) => {
                if b.version != self.version {
                    true
                } else {
                    let a0 = b.alt.max(MIN_ALT);
                    // **视高**变化超过该比例才重建（不是每帧重建）。
                    // 必须用视高：条带宽度是按"屏幕像素 × 该点距离"写进 mesh 的，而相机到球心的距离
                    // 贴地时几乎不变（1.13 → 1.004 只有 11%），用它当判据会永远不再重建。
                    let ratio = 0.15;
                    ((alt.max(MIN_ALT) - a0) / a0).abs() > ratio
                }
            }
        }
    }

    /// 开始一次本层构建。
    fn startj(&self, alt: f32) -> LayerState {
        LayerState { build: LayerBuild::new(), alt, version: self.version, started: Instant::now() }
    }
}

/// 一层的绘制样式：球面抬升 + **屏幕像素**尺寸（渲染时按相机距离换算成世界宽度）。
#[derive(Debug, Clone, Copy)]
pub struct LayerStyle {
    /// 球面抬升倍率（与球体半径同单位）。
    pub offset: f32,
    /// 线宽（屏幕像素）。
    pub width_px: f32,
    /// 点尺寸（屏幕像素）。
    pub point_px: f32,
}

impl Default for LayerStyle {
    fn default() -> Self {
        Self { offset: 1.0, width_px: 1.4, point_px: 5.0 }
    }
}

/// 一层的构建读数。
#[derive(Default, Clone, Copy)]
pub struct LayerStats {
    pub verts: usize,
    pub tris: usize,
    pub runs: usize,
    /// 细分后进入 mesh 的总点数（无 CPU 剔除，`verts == 2 * tess_pts`）。
    pub tess_pts: usize,
    /// 本层实际用到的最大细分数。
    pub segs_max: u32,
    /// 上一次构建耗时（ms）。
    pub last_tms: f32,
    /// 本层重建次数。
    pub rebuilds: u32,
}

/// 一层的增量构建状态。
struct LayerState {
    build: LayerBuild,
    /// 起始视高（只用于下一次"是否该重建"的判据）。
    alt: f32,
    version: u64,
    started: Instant,
}

impl CameraViwpt {
    /// 单段最大角长（弧度）：弦高误差恰好 1 像素。
    /// `8` 是弦高近似 `h ≈ L² / (8R)` 的系数。
    fn thetamax(&self, dist: f32, r: f32) -> f64 {
        let pw = self.worldpx(dist).max(0.0) as f64;
        let visual = (8.0 * pw / r.max(1e-6) as f64).sqrt();
        // 夹住弧长上限：长直段的**弦**会下沉进球体（5° 段约 6 km）。
        // **细分弧长上限**（弧度）：约 0.29°。
        visual.min(0.005)
    }
    
    /// 一段需要切几刀。数据点距已小于 `θ_max` 时返回 1（稠密数据零开销）。
    fn subdivisions(&self, a: GeoPoint, b: GeoPoint, dist: f32, r: f32) -> u32 {
        let theta = GeoPoint::angulard(a, b);
        let tm = self.thetamax(dist, r);
        if tm <= 0.0 || theta <= tm {
            1
        } else {
            // 单段细分数上限，防病态输入把顶点数打爆。
            ((theta / tm).ceil() as u32).clamp(1, 512)
        }
    }
    
    /// 折线 → 细分后的经纬度序列。接缝处顶点复用（不产生裂缝）。
    fn tessellate(&self, pts: &[GeoPoint], r: f32, cmpos: Vec3) -> Vec<GeoPoint> {
        if pts.len() < 2 {
            pts.to_vec()
        } else {
            let mut out = Vec::with_capacity(pts.len() * 2);
            out.push(pts[0].normalized());
            for w in pts.windows(2) {
                let (a, b) = (w[0].normalized(), w[1].normalized());
                let mid = GeoPoint::interpolate(a, b, 0.5);
                let dist = cmpos.distance(mid.world(r));
                let n = self.subdivisions(a, b, dist, r);
                for i in 1..=n {
                    out.push(GeoPoint::interpolate(a, b, i as f64 / n as f64));
                }
            }
            out
        }
    }
}

/// 可**增量推进**的图层构建器。
#[derive(Default)]
pub struct LayerBuild {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
    next: usize,
    /// 当前折线内已处理到的点数（大折线要跨帧切段，否则单条就能吃掉整帧预算）。
    skip: usize,
    runs: usize,
    segs_max: u32,
    tess_pts: usize,
}

/// 构建结果。
pub struct LayerBuilt {
    pub mesh: Mesh,
    pub verts: usize,
    pub tris: usize,
    pub runs: usize,
    pub segs_max: u32,
    /// 细分后的总点数（用于账目自洽：`verts == 2 * tess_pts`）。
    pub tess_pts: usize,
}

impl LayerBuild {
    pub fn new() -> Self {
        Self::default()
    }

    /// 已处理到的形状下标（用于估算进度）。
    pub fn cursor(&self) -> usize {
        self.next
    }

    /// 条带：宽度按**屏幕像素**给，逐点按"该点到相机的实际距离"换算成世界尺寸。
    ///
    /// 逐点换算（而不是全条带用一个天底视高）才能让整条条带视觉宽度一致：
    /// 一律用天底视高的话，视野边缘的点会偏细/偏粗约 40%。
    fn aribbon(
        &mut self,
        run: &[Vec3],
        px: f32,
        vp: &CameraViwpt,
        cmpos: Vec3
    ) {
        let base = self.positions.len() as u32;
        // 与距离无关的换算因子：单位距离处 1 像素对应的世界尺寸。
        let k = vp.worldpx(1.0);
        let n = run.len();
        for i in 0..n {
            let p = run[i];
            let nrm = p.normalize_or_zero();
            let tangent = if i == 0 {
                run[1] - run[0]
            } else if i + 1 == n {
                run[i] - run[i - 1]
            } else {
                run[i + 1] - run[i - 1]
            };
            let t = tangent.normalize_or_zero();
            let mut side = nrm.cross(t);
            if side.length_squared() < 1e-12 {
                side = nrm.cross(Vec3::Y);
                if side.length_squared() < 1e-12 {
                    side = nrm.cross(Vec3::X);
                }
            }
            let half = k * cmpos.distance(p) * px * 0.5;
            let side = side.normalize_or_zero() * half;
            self.positions.push((p - side).to_array());
            self.positions.push((p + side).to_array());
            self.normals.push(nrm.to_array());
            self.normals.push(nrm.to_array());
        }
        // 环绕方向必须让**几何法线朝外**（与顶点法线 `nrm` 一致）：
        for i in 0..n.saturating_sub(1) {
            let a = base + (i * 2) as u32;
            let b = a + 1;
            let c = a + 2;
            let d = a + 3;
            self.indices.extend_from_slice(&[a, d, b, a, c, d]);
        }
    }

    /// 细分一段折线并追加条带。`px` = 屏幕像素线宽。
    fn pusharc(
        &mut self,
        pts: &[GeoPoint],
        r: f32,
        offset: f32,
        px: f32,
        vp: &CameraViwpt,
        cmpos: Vec3,
    ) {
        if pts.len() >= 2 {
            let tess = vp.tessellate(pts, r, cmpos);
            // `segs_max` 只服务读数：全量扫描每段是纯开销，抽样即可。
            let stride = (pts.len() / 32).max(1);
            for w in pts.windows(2).step_by(stride) {
                let mid = GeoPoint::interpolate(w[0], w[1], 0.5);
                let d = cmpos.distance(mid.world(r));
                self.segs_max = self.segs_max.max(vp.subdivisions(w[0], w[1], d, r));
            }
            self.tess_pts += tess.len();
            let world: Vec<Vec3> = tess.iter().map(|p| p.world(r) * offset).collect();
            if world.len() >= 2 {
                self.aribbon(&world, px, vp, cmpos);
                self.runs += 1;
            }
        }
    }

    /// 从上次的位置继续处理，直到预算用尽或全部处理完。返回**是否已完成**。
    pub fn advance(
        &mut self,
        shapes: &[GeoShape],
        r: f32,
        offset: f32,
        px: f32,
        vp: &CameraViwpt,
        cmpos: Vec3,
        bdpts: usize,
    ) -> bool {
        let mut used = 0usize;
        while self.next < shapes.len() {
            // 点由构建系统单独出网格（数量少，不进分摊预算）。
            // 面按**闭合**边界描边：填充走海岸线那套栅格化路线，不在条带里做。
            let (pts, closed): (&[GeoPoint], bool) = match &shapes[self.next] {
                GeoShape::Line(p) => (p, false),
                GeoShape::Area(p) => (p, true),
                GeoShape::Point(_) => {
                    self.next += 1;
                    self.skip = 0;
                    continue;
                }
            };
            if pts.len() < 2 {
                self.next += 1;
                self.skip = 0;
                continue;
            }

            // 本帧在这条折线内最多取多少点（至少取 2 点才能成段）
            let take = bdpts.saturating_sub(used).max(2);
            // 防御：正常情况下 skip 一定落在本折线内（输入变更由版本号作废处理），
            // 但这里再夹一道——宁可少画一小段，也不能越界 panic。
            let start = self.skip.min(pts.len().saturating_sub(2));
            let end = (start + take).min(pts.len());
            let sub = &pts[start..end];
            if sub.len() >= 2 {
                self.pusharc(sub, r, offset, px, vp, cmpos);
            }
            used += end - start;

            if end < pts.len() {
                // 重叠一个点，保证下一帧的段与这一帧的段在几何上连起来
                self.skip = end - 1;
                return false;
            }
            // 面：补一段回到起点，让边界闭合（首尾本来就重合的输入不补）。
            if closed && pts.len() >= 3 {
                let (f, l) = (pts[0], pts[pts.len() - 1]);
                if (f.lon - l.lon).abs() > 1e-12 || (f.lat - l.lat).abs() > 1e-12 {
                    self.pusharc(&[l, f], r, offset, px, vp, cmpos);
                }
            }
            self.next += 1;
            self.skip = 0;
            if used >= bdpts {
                return false;
            }
        }
        true
    }

    pub fn finish(self) -> Option<LayerBuilt> {
        if self.indices.is_empty() {
            None
        } else {
            let verts = self.positions.len();
            let tris = self.indices.len() / 3;
            let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
            mesh.insert_indices(Indices::U32(self.indices));
            Some(LayerBuilt {
                mesh,
                verts,
                tris,
                runs: self.runs,
                segs_max: self.segs_max,
                tess_pts: self.tess_pts,
            })
        }
    }
}

/// 点状图元：朝外的八面体（6 顶点 / 8 面），比球体省。
pub fn build_ptmesh(center: Vec3, size: f32) -> Mesh {
    let n = center.normalize_or_zero();
    let mut t = n.cross(Vec3::Y);
    if t.length_squared() < 1e-12 {
        t = n.cross(Vec3::X);
    }
    let t = t.normalize_or_zero();
    let b = n.cross(t).normalize_or_zero();
    let tip = center + n * size;
    let tail = center - n * size;
    let ring = [
        center + t * size,
        center + b * size,
        center - t * size,
        center - b * size,
    ];
    let mut positions: Vec<[f32; 3]> = vec![tip.to_array(), tail.to_array()];
    for r in ring {
        positions.push(r.to_array());
    }
    let mut normals = Vec::with_capacity(positions.len());
    for p in &positions {
        normals.push(Vec3::from_array(*p).normalize_or_zero().to_array());
    }
    // ring 索引：0=tip, 1=tail, 2..5=ring
    let indices: Vec<u32> = vec![
        0, 2, 3, 0, 3, 4, 0, 4, 5, 0, 5, 2, // 上四面
        1, 3, 2, 1, 4, 3, 1, 5, 4, 1, 2, 5, // 下四面
    ];
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// 唯一的构建系统：遍历所有图层实体，**只重建需要重建的那层**。
pub fn build_glyphs(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    cam: Res<CameraState>,
    mut layers: Query<(Entity, &mut GlyphLayer)>,
) {
    let vp = cam.view();
    let r = EARTHR;
    // 视高：像素 ↔ 世界换算的唯一依据（也是重建判据的依据）
    let alt = (cam.dist - EARTHR).max(MIN_ALT);

    for (ent, mut l) in &mut layers {
        // 1) 该建才开始一次新构建（没建过 / 输入变了 / 视高变化够大）
        if l.job.is_none() && l.neebds(alt) {
            l.job = Some(l.startj(alt));
        }
        let Some(mut job) = l.job.take() else {
            continue;
        };
        // 2) 推进跨帧构建；没建完就继续显示旧 mesh
        // 每帧每层最多处理多少个点（分摊式重建）。
        let budget: usize = 6_000;
        if !job.build.advance(&l.shapes, r, l.style.offset, l.style.width_px, &vp, cam.pos, budget) {
            l.job = Some(job);
            continue;
        }
        // 3) 完成：整体替换本层的渲染实体
        let stale = job.version != l.version;
        for e in l.items.drain(..) {
            commands.entity(e).despawn();
        }
        let mut st = LayerStats {
            rebuilds: l.stats.rebuilds.saturating_add(1),
            last_tms: job.started.elapsed().as_secs_f32() * 1000.0,
            ..default()
        };
        if let Some(b) = job.build.finish() {
            st.verts = b.verts;
            st.tris = b.tris;
            st.runs = b.runs;
            st.tess_pts = b.tess_pts;
            st.segs_max = b.segs_max;
            let handle = meshes.add(b.mesh);
            let matl = l.matl.clone();
            let item = commands.spawn((
                Mesh3d(handle),
                MeshMaterial3d(matl),
                Transform::default(),
                ChildOf(ent),
            )).id();
            l.items.push(item);
        }

        // 点状图元：数量少，不参与分摊。每层用**自己的**点尺寸、材质与抬升。
        // 先攒到临时表：`&l.shapes` 的借用要一直活到循环结束，中途不能改 `l.items`。
        let mut pits: Vec<Entity> = Vec::new();
        for s in &l.shapes {
            let GeoShape::Point(p) = s else { continue };
            let center = p.world(r) * l.style.offset;
            // 点尺寸同样按"该点到相机的实际距离"换算
            let size = vp.worldpx(cam.pos.distance(center)) * l.style.point_px;
            let handle = meshes.add(build_ptmesh(center, size));
            st.verts += 6;
            st.tris += 8;
            let matl = l.matl.clone();
            let item = commands.spawn((
                Mesh3d(handle),
                MeshMaterial3d(matl),
                Transform::default(),
                ChildOf(ent),
            )).id();
            pits.push(item);
        }
        l.items.extend(pits);

        l.stats = st;
        l.built = Some(GlyphBuilt { version: job.version, alt: job.alt });
        if stale {
            // 构建期间输入又变了：立刻用最新输入再开一轮（旧 mesh 已撤，这轮建完即最终结果）
            l.job = Some(l.startj(alt));
        }
        debug!(
            "layer rebuild #{}: {} verts / {} tris, {:.0} ms",
            l.stats.rebuilds, l.stats.verts, l.stats.tris, l.stats.last_tms
        );
    }
}
