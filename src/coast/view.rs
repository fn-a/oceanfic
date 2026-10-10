//! 视野取环：决定"当前视野用哪些底图环"。

use std::sync::Arc;
use bevy::prelude::*;
use crate::globe::EARTHR;
use crate::camera::CameraState;
use crate::geo::store::{Levels, Level};
use crate::geo::{GeoPoint, GeoCubid, GeoShape};
use crate::glyphs::GlyphLayer;
use super::Coastl;

/// 当前取出的底图内容 + 上一次重选的依据（读数也在里面）。
#[derive(Resource, Default)]
pub struct Coastv {
    /// 当前档位（读数用）
    pub level: &'static str,
    /// 填充用：全部环（外环逆时针、洞顺时针）。
    /// 用 `Arc` 是为了**零拷贝**交给 worker 线程（几百万点时深拷贝很贵）。
    pub fill: Arc<Vec<Vec<GeoPoint>>>,
    /// 选环序号：每选出一轮新环 +1。填充补丁靠它判断"用的环换了"。
    pub seq: u64,
    pub rings: usize,
    pub points: usize,
    /// 上一次重选的依据
    pub last_dist: f32,
    /// **上一次选环时用的盒子**（关键：必须拿它来判断"中心是否跑出去了"）
    pub last_cubd: GeoCubid,
    pub selected: bool,
    /// 上次选环用的是不是**细节档**（GSHHG full）。
    pub detail: bool,
    /// 上次重选的时刻（秒）：拖动时节流，最多每 150ms 一次
    pub lastselt: f32,
}

impl Coastv {
    /// 是否需要重选：首次、数据**档位**变了、视高变化超过比例，或中心跑出**上次盒子**的内圈。
    ///
    /// 档位判据不能省：hi/lo 的门槛（alt 0.16 ≈ 1019 km）换算成相机距离只差约 14%，
    /// 永远触发不了下面 35% 那条视高判据 ⇒ 慢速缩放会一直停在粗档。
    pub fn stale(&self, dist: f32, center: GeoPoint, detail: bool) -> bool {
        if !self.selected || self.detail != detail {
            true
        } else {
            let d0 = self.last_dist.max(1e-6);
            // 视高变化超过该比例、或中心跑出上次盒子的内圈，才重新取环。
            // 重做阈值放宽：只有明显变化才重做（配合更大的覆盖面，来回拖动基本不重建）
            if ((dist - d0) / d0).abs() > 0.35 {
                true
            } else {
                let b = self.last_cubd;
                let (ix, iy) = ((b.lon1 - b.lon0) * 0.25, (b.lat1 - b.lat0) * 0.25);
                let dlon = {
                    // 经度差要按最短弧算（跨日界线时直接相减会得到 350 度）
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
                let half_lon = 0.5 * (b.lon1 - b.lon0) - ix;
                let half_lat = 0.5 * (b.lat1 - b.lat0) - iy;
                let lat_mid = 0.5 * (b.lat0 + b.lat1);
                !(dlon.abs() < half_lon && (center.lat - lat_mid).abs() < half_lat)
            }
        }
    }
}

pub fn setup_coastv(mut commands: Commands) {
    commands.insert_resource(Coastv::default());
}

/// 取出描边与填充所用的环；阈值内不动。描边环直接落到描边图层上。
pub fn update_coastv(
    levels: Res<Levels>,
    mut view: ResMut<Coastv>,
    mut stroke: Query<&mut GlyphLayer, With<Coastl>>,
    cam: Res<CameraState>,
    time: Res<Time>,
) {
    // **节流**：拖动/缩放时每帧都重选会导致明显卡顿（取环+简化+三角化+两套网格重建）。
    // 最多每 150ms 一次；被跳过的变化会在下一帧被 stale() 捕获，不会丢。
    // 注意首帧必须进来：`stale()` 正是靠 `selected == false` 做第一次选择。
    let now = time.elapsed_secs();
    if !view.selected || now - view.lastselt >= 0.15 {
        let alt = (cam.dist - EARTHR).max(0.0);
        let center = cam.pos.into();
        // 可见盒 = 屏幕跨度 × 该倍数。余量让平移时边缘不至于立刻缺数据。
        // 覆盖面放大：往回拖动仍在**已建范围内** ⇒ 不触发重建。
        let bx = GeoCubid::view(cam.view().fov, cam.dist, center, 2.0);
        let level: &Level = levels.pick(alt);
        let detail = std::ptr::eq(level, &levels.hi);
        if view.stale(cam.dist, center, detail) {
            let ids = level.ringsin(bx);
        
            let mut outline: Vec<GeoShape> = Vec::new();
            let mut fill: Vec<Vec<GeoPoint>> = Vec::new();
            let mut points = 0usize;
            for id in &ids {
                let r = &level.lines[*id as usize];
                // 填充：**不裁剪**，直接用整环。
                if r.len() >= 3 {
                    // 先展开经度（跨日界线的环在平面里必须连续，否则 ±180 的竖直边相隔 360，
                    // 三角化会把环当成横跨全球的多边形）。**不裁剪**，只做连续化。
                    let mut cont = bx.shift2frame(r);
                    // shift_into_frame 会追加收口点；环本来就隐含闭合，重复点会让耳切收尾时
                    // 产出一个巨大的退化三角形（实测面积比冲到 284）。
                    if cont.len() >= 2 {
                        let (f, l) = (cont[0], cont[cont.len() - 1]);
                        if (f.lon - l.lon).abs() < 1e-12 && (f.lat - l.lat).abs() < 1e-12 {
                            cont.pop();
                        }
                    }
                    points += cont.len();
                    fill.push(cont);
                }
                // 描边：**所有环都画**（外环 + 洞）。
                for run in bx.clipolyline(r) {
                    if run.len() >= 2 {
                        points += run.len();
                        outline.push(GeoShape::Line(run));
                    }
                }
            }
        
            // 描边环就地落到图层：以前这里是一次性交接（`fresh`）+ 另一个 system 取走。
            if let Ok(mut l) = stroke.single_mut() {
                l.shapes = outline;
                l.touched();
            }
            view.fill = std::sync::Arc::new(fill);
            view.level = if std::ptr::eq(level, &levels.hi) { "hi (GSHHG full)" } else { "lo (GSHHG intermediate)" };
            view.rings = ids.len();
            view.points = points;
            view.last_dist = cam.dist;
            view.last_cubd = bx;
            view.selected = true;
            view.detail = detail;
            view.lastselt = now;
            view.seq = view.seq.wrapping_add(1);
        
            debug!("view: {} rings={} pts={} alt={:.3}", view.level, view.rings, view.points, alt);
        }
    }
}
