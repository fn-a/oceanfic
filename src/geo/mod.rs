//! 地理数学：经纬度、形状、球面换算、视口投影。

use std::f64::consts::PI;
use bevy::math::Vec3;
use crate::globe::EARTHR;

pub mod mask;
pub mod work;
pub mod store;

pub const DEG_TO_RAD: f64 = PI / 180.0;
pub const RAD_TO_DEG: f64 = 180.0 / PI;

/// 经纬度点（度）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoPoint {
    pub lat: f64,
    pub lon: f64,
}

impl GeoPoint {
    pub const fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }

    /// 纬度夹到 `[-90,90]`，经度归一化到 `(-180,180]`。
    pub fn normalized(self) -> Self {
        let lat = self.lat.clamp(-90.0, 90.0);
        let mut lon = self.lon % 360.0;
        if lon > 180.0 {
            lon -= 360.0;
        } else if lon <= -180.0 {
            lon += 360.0;
        }
        Self { lat, lon }
    }

    pub fn unit(self) -> [f64; 3] {
        self.into()
    }

    pub fn world(self, r: f32) -> Vec3 {
        let u = self.unit();
        Vec3::new(u[0] as f32, u[1] as f32, u[2] as f32) * r
    }

    fn lerp(a: Self, b: Self, t: f64) -> GeoPoint {
        GeoPoint::new(a.lat + t * (b.lat - a.lat), a.lon + t * (b.lon - a.lon))
    }

    /// 两点大圆角长（弧度）。
    pub fn angulard(a: Self, b: Self) -> f64 {
        let (ua, ub) = (a.unit(), b.unit());
        let d = dot(ua, ub).clamp(-1.0, 1.0);
        let cross = [
            ua[1] * ub[2] - ua[2] * ub[1],
            ua[2] * ub[0] - ua[0] * ub[2],
            ua[0] * ub[1] - ua[1] * ub[0],
        ];
        norm(cross).atan2(d)
    }

    /// 折线/多边形的代表点：单位向量的归一化平均。
    pub fn centroid(pts: &[Self]) -> Self {
        if pts.is_empty() {
            GeoPoint::new(0.0, 0.0)
        } else {
            let mut acc = [0.0f64; 3];
            for p in pts {
                let u = p.unit();
                acc[0] += u[0];
                acc[1] += u[1];
                acc[2] += u[2];
            }
            acc.into()
        }
    }

    /// 环的鞋带面积（lon/lat 平面）：逆时针为正。
    pub fn signedarea(pts: &[Self]) -> f64 {
        if pts.len() < 3 {
            0.0
        } else {
            let mut s = 0.0;
            for i in 0..pts.len() {
                let a = &pts[i];
                let b = &pts[(i + 1) % pts.len()];
                s += a.lon * b.lat - b.lon * a.lat;
            }
            s * 0.5
        }
    }

    /// 大圆插值（slerp），`t ∈ [0,1]`。
    pub fn interpolate(a: Self, b: Self, t: f64) -> Self {
        let t = t.clamp(0.0, 1.0);
        let (ua, ub) = (a.unit(), b.unit());
        let d = dot(ua, ub).clamp(-1.0, 1.0);
        if d > 1.0 - 1e-12 {
            a.normalized()
        } else {
            if d < -1.0 + 1e-9 {
                // 对跖点：大圆不唯一，退化处理（本场景不会出现）
                let v = [
                    ua[0] + (ub[0] - ua[0]) * t,
                    ua[1] + (ub[1] - ua[1]) * t,
                    ua[2] + (ub[2] - ua[2]) * t,
                ];
                v.into()
            } else {
                let theta = d.acos();
                let s = theta.sin();
                let w1 = ((1.0 - t) * theta).sin() / s;
                let w2 = (t * theta).sin() / s;
                [ua[0] * w1 + ub[0] * w2, ua[1] * w1 + ub[1] * w2, ua[2] * w1 + ub[2] * w2].into()
            }
        }
    }

    pub fn inside(self, b: GeoCubid) -> bool {
        self.lon >= b.lon0 && self.lon <= b.lon1 && self.lat >= b.lat0 && self.lat <= b.lat1
    }
}

impl From<GeoPoint> for [f64; 3] {
    /// 单位球面向量。`lat=0, lon=0` 落在 **+X** 轴。
    fn from(value: GeoPoint) -> Self {
        let p = value.normalized();
        let (lat, lon) = (p.lat * DEG_TO_RAD, p.lon * DEG_TO_RAD);
        let cl = lat.cos();
        [cl * lon.cos(), lat.sin(), -cl * lon.sin()]
    }
}

impl From<[f64; 3]> for GeoPoint {
    /// 从单位向量还原经纬度。
    fn from(v: [f64; 3]) -> Self {
        let n = norm(v);
        if n <= f64::EPSILON {
            GeoPoint::new(0.0, 0.0)
        } else {
            let v = [v[0] / n, v[1] / n, v[2] / n];
            let lat = v[1].clamp(-1.0, 1.0).asin() * RAD_TO_DEG;
            let lon = (-v[2]).atan2(v[0]) * RAD_TO_DEG;
            GeoPoint::new(lat, lon).normalized()
        }
    }
}

impl From<Vec3> for GeoPoint {
    /// 经纬度 → 世界坐标（球面，半径 `r`）。
    fn from(p: Vec3) -> Self {
        [p.x as f64, p.y as f64, p.z as f64].into()
    }
}

/// 经纬盒子（连续经度，可越过 ±180）
#[derive(Debug, Default, Clone, Copy)]
pub struct GeoCubid {
    pub lon0: f64,
    pub lon1: f64,
    pub lat0: f64,
    pub lat1: f64,
}

impl GeoCubid {
    pub fn new(lon0: f64, lon1: f64, lat0: f64, lat1: f64,) -> Self {
        Self {
            lon0, lon1, lat0, lat1
        }
    }

    /// 相机 → 可见经纬度盒 `[lon0, lon1, lat0, lat1]`（经度连续，可越过 ±180）。
    pub fn view(fov: f32, dist: f32, center: GeoPoint, cover: f64) -> Self {
        let tanv = ((dist - EARTHR).max(1e-6) * (fov * 0.5).tan()) as f64;
        let half = (tanv * 180.0 / PI) * cover;
        let half_lat = half.clamp(0.02, 90.0);
        let coslat = center.lat.to_radians().cos().abs().max(0.05);
        let half_lon = (half_lat / coslat).clamp(0.02, 180.0);
        Self::new(
            center.lon - half_lon,
            center.lon + half_lon,
            (center.lat - half_lat).max(-90.0),
            (center.lat + half_lat).min(90.0),
        )
    }

    /// 线段与轴对齐盒子的参数区间（Liang–Barsky）。返回进入/离开参数，无交返回 None。
    fn segboxrange(self, a: GeoPoint, b: GeoPoint) -> Option<(f64, f64)> {
        let (dx, dy) = (b.lon - a.lon, b.lat - a.lat);
        let (mut t0, mut t1) = (0.0f64, 1.0f64);
        for (p, q) in [
            (-dx, a.lon - self.lon0),
            (dx, self.lon1 - a.lon),
            (-dy, a.lat - self.lat0),
            (dy, self.lat1 - a.lat),
        ] {
            if p.abs() < 1e-12 {
                if q < 0.0 {
                    return None; // 平行且在盒外
                }
            } else {
                let r = q / p;
                if p < 0.0 {
                    if r > t1 {
                        return None;
                    }
                    if r > t0 {
                        t0 = r;
                    }
                } else {
                    if r < t0 {
                        return None;
                    }
                    if r < t1 {
                        t1 = r;
                    }
                }
            }
        }
        Some((t0, t1))
    }

    /// 线段与矩形边界的交点（先到先得，够用即可）。
    fn bordercross(self, a: GeoPoint, b: GeoPoint) -> Option<GeoPoint> {
        let mut best: Option<(f64, GeoPoint)> = None;
        let mut consider = |t: f64| {
            if (0.0..=1.0).contains(&t) {
                let p = GeoPoint::new(a.lat + t * (b.lat - a.lat), a.lon + t * (b.lon - a.lon));
                if p.inside(self) && best.as_ref().map(|(bt, _)| t < *bt).unwrap_or(true) {
                    best = Some((t, p));
                }
            }
        };
        if (b.lon - a.lon).abs() > 1e-12 {
            consider((self.lon0 - a.lon) / (b.lon - a.lon));
            consider((self.lon1 - a.lon) / (b.lon - a.lon));
        }
        if (b.lat - a.lat).abs() > 1e-12 {
            consider((self.lat0 - a.lat) / (b.lat - a.lat));
            consider((self.lat1 - a.lat) / (b.lat - a.lat));
        }
        best.map(|(_, p)| p)
    }

    /// 把环的经度展开成连续路径，并整体平移到盒子附近（跨日界线必需）。
    // 裁剪（把环裁到视野盒）
    pub fn shift2frame(self, pts: &[GeoPoint]) -> Vec<GeoPoint> {
        if pts.is_empty() {
            Vec::new()
        } else {
            let center = 0.5 * (self.lon0 + self.lon1);
            let mut out: Vec<GeoPoint> = Vec::with_capacity(pts.len());
            let mut prev = pts[0].lon;
            out.push(pts[0]);
            for p in &pts[1..] {
                let mut x = p.lon;
                while x - prev > 180.0 {
                    x -= 360.0;
                }
                while x - prev < -180.0 {
                    x += 360.0;
                }
                out.push(GeoPoint::new(p.lat, x));
                prev = x;
            }
            // 收口点（用于闭合）也展开
            {
                let mut x = pts[0].lon;
                while x - prev > 180.0 {
                    x -= 360.0;
                }
                while x - prev < -180.0 {
                    x += 360.0;
                }
                out.push(GeoPoint::new(pts[0].lat, x));
            }
            let mean = out.iter().map(|p| p.lon).sum::<f64>() / out.len() as f64;
            let shift = ((center - mean) / 360.0).round() * 360.0;
            if shift != 0.0 {
                for p in &mut out {
                    p.lon += shift;
                }
            }
            out
        }
    }

    /// 折线裁到盒子：返回盒内的若干段（长度 ≥ 2）。
    pub fn clipolyline(self, pts: &[GeoPoint]) -> Vec<Vec<GeoPoint>> {
        let path = self.shift2frame(pts);
        let mut runs: Vec<Vec<GeoPoint>> = Vec::new();
        let mut cur: Vec<GeoPoint> = Vec::new();
        for i in 0..path.len().saturating_sub(1) {
            let (a, b) = (path[i], path[i + 1]);
            let (ain, bin) = (a.inside(self), b.inside(self));
            if ain {
                if cur.is_empty() {
                    cur.push(a);
                }
                if bin {
                    cur.push(b);
                } else {
                    // 出盒：在边界上补一个交点后收尾
                    if let Some(x) = self.bordercross(a, b) {
                        cur.push(x);
                    }
                    if cur.len() >= 2 {
                        runs.push(std::mem::take(&mut cur));
                    } else {
                        cur.clear();
                    }
                }
            } else if bin {
                // 入盒
                if let Some(x) = self.bordercross(a, b) {
                    cur.push(x);
                }
                cur.push(b);
            } else if let Some((t0, t1)) = self.segboxrange(a, b) {
                // 两端都在盒外，但线段**穿过**盒子（Liang–Barsky）
                if t1 > t0 {
                    runs.push(vec![GeoPoint::lerp(a, b, t0), GeoPoint::lerp(a, b, t1)]);
                }
            }
        }
        if cur.len() >= 2 {
            runs.push(cur);
        }
        runs
    }

}


fn norm(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// 矢量数据类型。`Area` 目前按**闭合边界**描边，填充未实现。
#[derive(Debug, Clone, PartialEq)]
pub enum GeoShape {
    Point(GeoPoint),
    Line(Vec<GeoPoint>),
    Area(Vec<GeoPoint>),
}

impl GeoShape {
    pub fn pcount(&self) -> usize {
        match self {
            GeoShape::Point(_) => 1,
            GeoShape::Line(p) | GeoShape::Area(p) => p.len(),
        }
    }

    /// 形状的代表点（用于剔除与简单定位）。
    pub fn centroid(&self) -> GeoPoint {
        match self {
            GeoShape::Point(p) => *p,
            GeoShape::Line(p) | GeoShape::Area(p) => GeoPoint::centroid(p),
        }
    }
}
