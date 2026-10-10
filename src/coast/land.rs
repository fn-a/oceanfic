//! 陆地填色（近景）：按视野现栅格化的局部补丁。

use bevy::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_resource::{Extent3d, TextureFormat, TextureDimension};
use crate::camera::CameraState;
use crate::config::{Manifest, Colors};
use crate::globe::EARTHR;
use crate::geo::store::Levels;
use crate::geo::work::Worker;
use crate::geo::{GeoPoint, GeoCubid};
use super::view::Coastv;

/// 陆地填充层：夹在球体与矢量线之间。
pub const SLAND_OFT: f32 = 1.0 + 2.0 / 6_371_000.0;

/// 补丁纹理边长。补丁盒子取屏幕跨度的 2 倍（留平移余量），所以 **texel ≈ 1 屏幕像素**：
/// 1024 会让 1 texel 盖住约 2 像素，陆海边界被双线性采样拉成约 2 px 的糊边，贴不到描边上。
pub const PATCH_TEX: u32 = 2048;
/// 补丁网格分段（球面贴合度；64 段足够，只覆盖一小块视野）
pub const PATCH_SEG: u32 = 64;

/// 近景填色补丁的状态。
#[derive(Resource)]
pub struct Ldfill {
    pub entity: Entity,
    pub image: Handle<Image>,
    /// 已构建的盒子
    pub built: Option<GeoCubid>,
    pub rebuilds: u32,
    /// 本次用到的环数（排障用）
    pub rings: usize,
    /// 已提交的选环序号：环换了（例如 lo→hi 换档）必须重做补丁。
    pub seq: u64,
    land: [f32; 3],
    ocean: [f32; 3],
}

impl GeoCubid {
    /// 球面补丁网格：覆盖 `[lon0,lon1] × [lat0,lat1]`，UV 与栅格**一一对应**
    /// （`u: 0→1 = lon0→lon1`，`v: 0→1 = lat1→lat0`，即 v=0 在北）。
    fn mesh(
        &self,
        r: f32,
        seg_x: u32,
        seg_y: u32,
    ) -> Mesh {
        let seg_x = seg_x.max(1);
        let seg_y = seg_y.max(1);
        let row = seg_x + 1;
        let count = ((seg_x + 1) * (seg_y + 1)) as usize;
        let mut positions: Vec<[f32; 3]> = Vec::with_capacity(count);
        let mut normals: Vec<[f32; 3]> = Vec::with_capacity(count);
        let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(count);
        let mut indices: Vec<u32> = Vec::with_capacity((seg_x * seg_y * 6) as usize);
        for j in 0..=seg_y {
            let v = j as f64 / seg_y as f64;
            let lat = self.lat1 - v * (self.lat1 - self.lat0);
            for i in 0..=seg_x {
                let u = i as f64 / seg_x as f64;
                let lon = self.lon0 + u * (self.lon1 - self.lon0);
                let p = GeoPoint::new(lat, lon).world(r);
                positions.push(p.to_array());
                normals.push(p.normalize_or_zero().to_array());
                uvs.push([u as f32, v as f32]);
            }
        }
        for j in 0..seg_y {
            for i in 0..seg_x {
                let a = j * row + i;
                let (b, c, d) = (a + 1, a + row, a + row + 1);
                indices.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_indices(Indices::U32(indices));
        mesh
    }
}

/// 建立补丁实体与占位纹理（零顶点网格会让 Bevy 的 mesh slab 出问题，必须给有效面片）。
pub fn setup_ldfill(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    manifest: Res<Manifest>,
) {
    let land =  Colors::parsex(&manifest.colors.land_fill).unwrap_or([0.25, 0.42, 0.27]);
    let ocean = Colors::parsex(&manifest.colors.ocean).unwrap_or([0.05, 0.15, 0.4]);
    let size = PATCH_TEX.max(2);
    let image = images.add(Image::new(
        Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        TextureDimension::D2,
        vec![0u8; size as usize * size as usize * 4],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    ));
    // 起始网格给一小块有效面片（第一次构建时会被替换）
    let mesh = GeoCubid::new(0.0, 1.0, 0.0, 1.0).mesh(EARTHR * SLAND_OFT, 2, 2);
    let entity = commands
        .spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::WHITE,
                base_color_texture: Some(image.clone()),
                alpha_mode: AlphaMode::Blend,
                metallic: 0.0,
                perceptual_roughness: 0.9,
                cull_mode: None, // 小块球面，双面可见更稳
                ..default()
            })),
            Transform::default(),
            Visibility::Hidden,
        ))
        .id();
    commands.insert_resource(Ldfill {
        entity,
        image,
        built: None,
        rebuilds: 0,
        rings: 0,
        seq: 0,
        land,
        ocean,
    });
}

/// 每帧：先装上 worker 的结果，再按阈值决定是否提交新任务。
pub fn update_ldfill(
    mut fill: ResMut<Ldfill>,
    mut worker: ResMut<Worker>,
    view: Res<Coastv>,
    cam: Res<CameraState>,
    levels: Res<Levels>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut vis: Query<&mut Visibility>,
    mut mesh: Query<&mut Mesh3d>,
) {
    // 1) 先收后台结果：上传纹理 + 替换补丁网格（几 ms）
    if let Some(done) = worker.drain() {
        let size = PATCH_TEX.max(2);
        if let Some(mut img) = images.get_mut(&fill.image) {
            *img = Image::new(
                Extent3d { width: size, height: size, depth_or_array_layers: 1 },
                TextureDimension::D2,
                done.rgba,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            );
        }
        let handle = meshes.add(done.cubd.mesh(EARTHR * SLAND_OFT, PATCH_SEG.max(1), PATCH_SEG.max(1)));
        if let Ok(mut m) = mesh.get_mut(fill.entity) {
            m.0 = handle;
        }
        fill.built = Some(done.cubd);
        fill.rings = done.rings;
        fill.rebuilds = fill.rebuilds.saturating_add(1);
        if let Ok(mut v) = vis.get_mut(fill.entity) {
            *v = Visibility::Visible;
        }
        debug!(
            "patch #{}: rings={} pts={} raster={} ms",
            fill.rebuilds, done.rings, done.points, done.ms
        );
    }

    // 2) 远景不需要补丁（球体上的全局遮罩负责）。
    // 门槛取**细节档自己的**可信上限。
    if (cam.dist - EARTHR) >= levels.hi.altmax {
        if let Ok(mut v) = vis.get_mut(fill.entity) {
            *v = Visibility::Hidden;
        }
    } else {
        if !view.fill.is_empty() {
            // 3) 阈值：换了一轮选环结果、视高变化够大，或中心跑出**上次提交时**的盒子内圈
            let center = cam.pos.into();
            // 补丁覆盖面 = 屏幕跨度 × 该倍数（留余量，快速平移时不至于立刻露边）
            let bx = GeoCubid::view(cam.view().fov, cam.dist, center, 2.0);
            // 选环序号变了 = 填充用的环换了（例如 lo→hi 换档）：必须重做补丁，
            // 否则描边已经是细节档、陆地填充还停在粗档。
            let nsel = fill.seq != view.seq;
            let need = match fill.built {
                None => true,
                Some(b) => {
                    // 有任务在飞时不动（等它落地，避免抖动）
                    if worker.flying() {
                        return;
                    }
                    // 盒子尺寸变化够大 ⇒ 1 texel 盖住的屏幕像素数变了，重做才不糊
                    let old = (b.lon1 - b.lon0).abs().max((b.lat1 - b.lat0).abs());
                    let cur = (bx.lon1 - bx.lon0).abs().max((bx.lat1 - bx.lat0).abs());
                    // 补丁盒子尺寸变化超过该比例就重做。
                    // 补丁质量 = 1 texel 覆盖多少屏幕像素 ∝ 盒子尺寸，所以判据必须是**盒子尺寸**，
                    // 不能用相机到球心的距离：贴地时后者几乎不变（1.13 → 1.004 只有 11%），
                    // 阈值永远触发不了，补丁就一直停在旧的大盒子上 ⇒ 越往下陆地边缘越糊。
                    let ratio = 0.2;
                    let sized = (cur - old).abs() / old.max(1e-9) > ratio;
                    let (ix, iy) = ((b.lon1 - b.lon0) * 0.25, (b.lat1 - b.lat0) * 0.25);
                    let dlon = {
                        let d = center.lon - 0.5 * (b.lon0 + b.lon1);
                        let mut d = d % 360.0;
                        if d > 180.0 {
                            d -= 360.0;
                        }
                        if d < -180.0 {
                            d += 360.0;
                        }
                        d
                    };
                    let xi =  dlon.abs() < 0.5 * (b.lon1 - b.lon0) - ix;
                    let yi = (center.lat - 0.5 * (b.lat0 + b.lat1)).abs() < 0.5 * (b.lat1 - b.lat0) - iy;
                    let inside = xi && yi;
                    sized || !inside || nsel
                }
            };
            if !need {
                if let Ok(mut v) = vis.get_mut(fill.entity) {
                    *v = Visibility::Visible;
                }
            } else {
                // 4) 提交给 worker（`Arc` 克隆 = 只加引用计数，不深拷贝几何）
                let ok = worker.submit(
                    bx,
                    view.fill.clone(),
                    PATCH_TEX,
                    // 外缘羽化的 texel 数（与全局遮罩平滑衔接）
                    8,
                    fill.land,
                    fill.ocean,
                );
                if ok {
                    // 记下这一轮用的是哪次选环结果（提交失败则 next frame 重试）
                    fill.seq = view.seq;
                }
            }
        }
    }
}
