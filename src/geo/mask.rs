//! 陆地遮罩：多边形 → 1 bit 位图 → RGBA 纹理数据。

use crate::geo::{GeoPoint, GeoCubid};

/// 陆地填充遮罩分辨率（等距圆柱）。texel ≈ 360/W 度；4096 时约 9.8 km。
pub const MASK_WIDTH: u32 = 8192;
pub const MASK_HEIGHT: u32 = 4096;

/// 1 bit/px 的等距圆柱遮罩。`row 0` 对应**北纬**，`col 0` 对应 **lon = -180**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mask {
    pub width: u32,
    pub height: u32,
    pub bits: Vec<u8>,
}

impl Mask {
    pub fn new(width: u32, height: u32) -> Self {
        let n = (width as usize * height as usize).div_ceil(8);
        Self { width, height, bits: vec![0u8; n] }
    }

    #[inline]
    pub fn get(&self, x: u32, y: u32) -> bool {
        if x >= self.width || y >= self.height {
            false
        } else {
            let idx = y as usize * self.width as usize + x as usize;
            (self.bits[idx / 8] >> (idx % 8)) & 1 == 1
        }
    }

    #[inline]
    pub fn set(&mut self, x: u32, y: u32, v: bool) {
        if !(x >= self.width || y >= self.height) {
            let idx = y as usize * self.width as usize + x as usize;
            if v {
                self.bits[idx / 8] |= 1 << (idx % 8);
            } else {
                self.bits[idx / 8] &= !(1 << (idx % 8));
            }
        }
    }

    /// 已置位（陆地）的 texel 数。
    pub fn count(&self) -> usize {
        self.bits.iter().map(|b| b.count_ones() as usize).sum()
    }

    /// 同 [`Mask::srgba`]，但把外缘 `feather` 个 texel 的 alpha 从 0 渐升到 1。
    pub fn rgbaf(&self, set: [f32; 3], clear: [f32; 3], feather: u32) -> Vec<u8> {
        let to_u8 = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        let set_px = [to_u8(set[0]), to_u8(set[1]), to_u8(set[2])];
        let clear_px = [to_u8(clear[0]), to_u8(clear[1]), to_u8(clear[2])];
        let f = feather as f64;
        let mut out = Vec::with_capacity(self.width as usize * self.height as usize * 4);
        for y in 0..self.height {
            for x in 0..self.width {
                let px = if self.get(x, y) { set_px } else { clear_px };
                let a = if f <= 0.0 {
                    1.0
                } else {
                    let d = x
                        .min(self.width - 1 - x)
                        .min(y.min(self.height - 1 - y)) as f64;
                    ((d + 0.5) / f).clamp(0.0, 1.0)
                };
                out.extend_from_slice(&[px[0], px[1], px[2], to_u8(a as f32)]);
            }
        }
        out
    }

    /// 展开成 `Rgba8UnormSrgb` 纹理数据。陆地/海洋两色由调用方给出（sRGB，0..1）。
    pub fn srgba(&self, land: [f32; 3], ocean: [f32; 3]) -> Vec<u8> {
        let to_u8 = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        let land_px = [to_u8(land[0]), to_u8(land[1]), to_u8(land[2]), 255];
        let ocean_px = [to_u8(ocean[0]), to_u8(ocean[1]), to_u8(ocean[2]), 255];
        let mut out = Vec::with_capacity(self.width as usize * self.height as usize * 4);
        for y in 0..self.height {
            for x in 0..self.width {
                let px = if self.get(x, y) { land_px } else { ocean_px };
                out.extend_from_slice(&px);
            }
        }
        out
    }
}

/// 遮罩的一条边：`x(y) = x0 + (y - y_min) * slope`。
#[derive(Debug, Clone, Copy)]
struct Edge {
    y_min: f64,
    y_max: f64,
    x_aty: f64,
    slope: f64,
    /// 有向：`+1` 表示向上穿过（用于**非零环绕**判定；全局版用 even-odd，忽略它）。
    dir: f64,
}

/// 扫描线 even-odd 填充，`rings` 为 `[lon, lat]`（度）。
pub fn rasterize(rings: &[Vec<[f64; 2]>], width: u32, height: u32) -> Mask {
    let mut mask = Mask::new(width, height);
    let mut edges: Vec<Edge> = Vec::new();
    for ring in rings {
        if ring.len() < 3 {
            continue;
        }
        // 先把整个环按 ±180 切开，再逐段收集边。
        for piece in split_ring(ring) {
            if piece.len() < 3 {
                continue;
            }
            for i in 0..piece.len() {
                press_edge(&mut edges, piece[i], piece[(i + 1) % piece.len()]);
            }
        }
    }
    if edges.is_empty() {
        mask
    } else {
        edges.sort_by(|a, b| a.y_min.total_cmp(&b.y_min));
    
        let w = width as usize;
        let h = height as usize;
        let mut active: Vec<usize> = Vec::new();
        let mut next = 0usize;
        let mut xs: Vec<f64> = Vec::new();
    
        // **必须自南向北扫**：边按 y_min 升序排列，只有当纬度上升时才"到点加入"，
        // 越过 y_max 才移除。若从北极开始扫，第一行就会把所有边判为"已结束"而清空。
        for row in (0..h).rev() {
            // texel 中心纬度（row 0 = 北）
            let y = 90.0 - (row as f64 + 0.5) * (180.0 / h as f64);
            while next < edges.len() && edges[next].y_min <= y {
                active.push(next);
                next += 1;
            }
            active.retain(|&ei| edges[ei].y_max >= y);
    
            xs.clear();
            for &ei in &active {
                let e = &edges[ei];
                xs.push(e.x_aty + (y - e.y_min) * e.slope);
            }
            xs.sort_by(|a, b| a.total_cmp(b));
    
            let mut i = 0;
            while i + 1 < xs.len() {
                let (xa, xb) = (xs[i], xs[i + 1]);
                if xb > xa {
                    // 按 texel 中心判定，与行的判定方式一致。
                    let ca = (xa + 180.0) / 360.0 * w as f64 - 0.5;
                    let cb = (xb + 180.0) / 360.0 * w as f64 - 0.5;
                    let c0 = ca.ceil() as i64;
                    let c1 = cb.floor() as i64;
                    for c in c0.max(0)..=c1.min(w as i64 - 1) {
                        mask.set(c as u32, row as u32, true);
                    }
                }
                i += 2;
            }
        }
        mask
    }
}

/// 局部等距圆柱遮罩：跟着视野走的填色图。
pub fn rasterbox(
    rings: &[Vec<[f64; 2]>],
    cubd: GeoCubid,
    width: u32,
    height: u32,
) -> Mask {
    let mut mask = Mask::new(width, height);
    if width == 0 || height == 0 || cubd.lon1 <= cubd.lon0 || cubd.lat1 <= cubd.lat0 {
        mask
    } else {
        let center = 0.5 * (cubd.lon0 + cubd.lon1);
        let mut edges: Vec<Edge> = Vec::new();
        for ring in rings {
            if ring.len() < 3 {
                continue;
            }
            // 1) 展开成连续路径（相邻点经度差不超过 180°）
            let mut path: Vec<[f64; 2]> = Vec::with_capacity(ring.len() + 1);
            let mut prev = ring[0][0];
            path.push(ring[0]);
            for p in &ring[1..] {
                let mut x = p[0];
                while x - prev > 180.0 {
                    x -= 360.0;
                }
                while x - prev < -180.0 {
                    x += 360.0;
                }
                path.push([x, p[1]]);
                prev = x;
            }
            // 收口边也要展开，否则最后一点到首点的边会横跨 360°
            {
                let mut x = ring[0][0];
                while x - prev > 180.0 {
                    x -= 360.0;
                }
                while x - prev < -180.0 {
                    x += 360.0;
                }
                path.push([x, ring[0][1]]);
            }
            // 2) 整体平移，让这条环靠近盒子（跨日界线的环因此落进同一个连续区间）
            let mean = path.iter().map(|p| p[0]).sum::<f64>() / path.len() as f64;
            let shift = ((center - mean) / 360.0).round() * 360.0;
            if shift != 0.0 {
                for p in &mut path {
                    p[0] += shift;
                }
            }
            // 3) 收集边（水平边不参与交点计数，与全局版一致）
            for i in 0..path.len() {
                press_edge(&mut edges, path[i], path[(i + 1) % path.len()]);
            }
        }
        if edges.is_empty() {
            mask
        } else {
            edges.sort_by(|a, b| a.y_min.total_cmp(&b.y_min));
            let w = width as usize;
            let h = height as usize;
            let lon_span = cubd.lon1 - cubd.lon0;
            let lat_span = cubd.lat1 - cubd.lat0;
            let mut active: Vec<usize> = Vec::new();
            let mut next = 0usize;
            let mut xs: Vec<f64> = Vec::new();
            let mut dirs: Vec<f64> = Vec::new();
            let mut order: Vec<usize> = Vec::new();
        
            // 与全局版同样的理由：边按 y_min 升序 ⇒ **必须自南向北**扫
            for row in (0..h).rev() {
                let y = cubd.lat1 - (row as f64 + 0.5) * (lat_span / h as f64);
                while next < edges.len() && edges[next].y_min <= y {
                    active.push(next);
                    next += 1;
                }
                active.retain(|&ei| edges[ei].y_max >= y);
        
                // **非零环绕**（nonzero winding）而不是 even-odd：湖中的岛才会重新填上。
                xs.clear();
                dirs.clear();
                for &ei in &active {
                    let e = &edges[ei];
                    xs.push(e.x_aty + (y - e.y_min) * e.slope);
                    dirs.push(e.dir);
                }
                order.clear();
                order.extend(0..xs.len());
                order.sort_by(|&a, &b| xs[a].total_cmp(&xs[b]));
        
                let mut wind = 0.0f64;
                let mut span = 0.0f64;
                for &k in &order {
                    if wind == 0.0 {
                        span = xs[k];
                    }
                    wind += dirs[k];
                    if wind == 0.0 && xs[k] > span {
                        // 按 **texel 中心**判定（与全局版同一约定，避免整体偏半个 texel）
                        let ca = (span - cubd.lon0) / lon_span * w as f64 - 0.5;
                        let cb = (xs[k] - cubd.lon0) / lon_span * w as f64 - 0.5;
                        let c0 = ca.ceil() as i64;
                        let c1 = cb.floor() as i64;
                        for c in c0.max(0)..=c1.min(w as i64 - 1) {
                            mask.set(c as u32, row as u32, true);
                        }
                    }
                }
            }
            mask
        }
    }
}

/// Douglas–Peucker 简化（lon/lat 平面，`tol` 单位为度）。
pub fn simplify(pts: &[GeoPoint], tol: f64) -> Vec<GeoPoint> {
    if pts.len() < 4 || tol <= 0.0 {
        pts.to_vec()
    } else {
        let mut keep = vec![false; pts.len()];
        keep[0] = true;
        keep[pts.len() - 1] = true;
        let mut stack = vec![(0usize, pts.len() - 1)];
        while let Some((s, e)) = stack.pop() {
            if e <= s + 1 {
                continue;
            }
            let (a, b) = (pts[s], pts[e]);
            let (dx, dy) = (b.lon - a.lon, b.lat - a.lat);
            let len2 = dx * dx + dy * dy;
            let mut far = 0.0f64;
            let mut far_i = s;
            for (k, p) in pts.iter().enumerate().take(e).skip(s + 1) {
                let t = if len2 > 0.0 {
                    (((p.lon - a.lon) * dx + (p.lat - a.lat) * dy) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let (px, py) = (a.lon + t * dx - p.lon, a.lat + t * dy - p.lat);
                let d = px * px + py * py;
                if d > far {
                    far = d;
                    far_i = k;
                }
            }
            if far.sqrt() > tol {
                keep[far_i] = true;
                stack.push((s, far_i));
                stack.push((far_i, e));
            }
        }
        pts.iter().enumerate().filter(|(i, _)| keep[*i]).map(|(_, p)| *p).collect()
    }
}

/// 一条边 → 扫描线记录。水平边不参与 even-odd 计数。
fn press_edge(edges: &mut Vec<Edge>, a: [f64; 2], b: [f64; 2]) {
    let (x0, y0) = (a[0], a[1]);
    let (x1, y1) = (b[0], b[1]);
    if (y1 - y0).abs() >= 1e-12 {
        let slope = (x1 - x0) / (y1 - y0);
        let (y_min, y_max) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        let x_at_ymin = x0 + (y_min - y0) * slope;
        // 方向：从较小的 y 到较大的 y 记为 +1（非零环绕用）
        let dir = if y1 > y0 { 1.0 } else { -1.0 };
        edges.push(Edge { y_min, y_max, x_aty: x_at_ymin, slope, dir });
    }
}

/// 把环的经度展开成连续路径，再切成落在 [-180,180] 内的若干片。
fn split_ring(ring: &[[f64; 2]]) -> Vec<Vec<[f64; 2]>> {
    let crosses = (0..ring.len()).any(|i| {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        let dlon = (b[0] - a[0]).abs();
        let dlat = (b[1] - a[1]).abs();
        if dlon <= 180.0 {
            false
        } else {
            // 恰好 360° 的水平边是**极区收口边**（极点上所有经度是同一个点），
            // 不是跨日界线；把它算进来会让极区环被误展开成退化路径。
            let poledge = dlat < 1e-12 && dlon > 359.0;
            !poledge
        }
    });
    if !crosses {
        vec![ring.to_vec()]
    } else {
        let mut unwrapped: Vec<[f64; 2]> = Vec::with_capacity(ring.len() + 1);
        let mut prev = ring[0][0];
        unwrapped.push(ring[0]);
        for p in &ring[1..] {
            let mut x = p[0];
            while x - prev > 180.0 {
                x -= 360.0;
            }
            while x - prev < -180.0 {
                x += 360.0;
            }
            unwrapped.push([x, p[1]]);
            prev = x;
        }
        // 收口边也要展开：否则最后一点到首点的边会变成横跨 360° 的假边
        {
            let mut x = ring[0][0];
            while x - prev > 180.0 {
                x -= 360.0;
            }
            while x - prev < -180.0 {
                x += 360.0;
            }
            unwrapped.push([x, ring[0][1]]);
        }
    
        let mut pieces = Vec::new();
        for k in [-360.0f64, 0.0, 360.0] {
            let (lo, hi) = (-180.0 + k, 180.0 + k);
            if unwrapped.iter().all(|p| p[0] < lo) || unwrapped.iter().all(|p| p[0] > hi) {
                continue;
            }
            let clipped = prune_half(&prune_half(&unwrapped, lo, true), hi, false);
            if clipped.len() < 3 {
                continue;
            }
            pieces.push(clipped.into_iter().map(|p| [p[0] - k, p[1]]).collect());
        }
        pieces
    }
}

/// Sutherland-Hodgman：把多边形按竖直线 `x` 裁一刀（`keep_ge` 决定保留哪一侧）。
fn prune_half(poly: &[[f64; 2]], x: f64, keep_ge: bool) -> Vec<[f64; 2]> {
    let inside = |p: &[f64; 2]| if keep_ge { p[0] >= x } else { p[0] <= x };
    let mut out = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let cur = poly[i];
        let prev = poly[(i + poly.len() - 1) % poly.len()];
        let (ci, pi) = (inside(&cur), inside(&prev));
        if ci {
            if !pi {
                out.push(cross_abxv(prev, cur, x));
            }
            out.push(cur);
        } else if pi {
            out.push(cross_abxv(prev, cur, x));
        }
    }
    out
}

fn cross_abxv(a: [f64; 2], b: [f64; 2], x: f64) -> [f64; 2] {
    let dx = b[0] - a[0];
    let t = if dx.abs() < 1e-12 { 0.0 } else { (x - a[0]) / dx };
    [x, a[1] + (b[1] - a[1]) * t]
}
