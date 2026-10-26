//! 运行时烘焙：GSHHG shapefile → OFBM 资产。
//!
//! 同一份 shapefile 输入必须产生与预生成资产逐字节相同的结果
//! （绕向、日界线切分、微度 delta 编码都不变）。

use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;
use anyhow::{Result, anyhow, bail};
use super::store::{Header, COAST_HI_BIN, COAST_LO_BIN, HEADER_LEN, xxh32};

/// 需要烘焙的档位（缺失或损坏的资产）。
#[derive(Debug, Clone, Copy)]
pub struct Targets {
    pub hi: bool,
    pub lo: bool,
}

impl Targets {
    pub fn any(self) -> bool {
        self.hi || self.lo
    }
}

pub struct Bakedin {
    pub name: &'static str,
    pub bytes: Vec<u8>,
    pub rings: usize,
    pub points: usize,
}

impl Bakedin {
    /// 从 GSHHG shapefile zip 烘焙所需档位。
    ///
    /// `report` 用于把进度写进控制台；失败时返回错误，调用方负责降级。
    pub fn zip(
        zipath: &Path,
        targets: Targets,
        report: &mut dyn FnMut(String),
    ) -> Result<Vec<Bakedin>> {
        let file = File::open(zipath)?;
        let mut zip = ZipArchive::new(file)?;
    
        let mut out = Vec::new();
        if targets.hi {
            out.push(Self::level(&mut zip, "f", "detail (full)", 0.16, COAST_HI_BIN, report)?);
        }
        if targets.lo {
            out.push(Self::level(
                &mut zip,
                "i",
                "global (intermediate)",
                1.0e9,
                COAST_LO_BIN,
                report,
            )?);
        }
        Ok(out)
    }

    fn level(
        zip: &mut ZipArchive<File>,
        res: &str,
        label: &str,
        altmax: f32,
        name: &'static str,
        report: &mut dyn FnMut(String),
    ) -> Result<Bakedin> {
        report(format!("baking {label} ({res}) ..."));

        let mut rings: Vec<Vec<[f64; 2]>> = Vec::new();
        let mut total = 0usize;
        let mut l1see = false;

        // 陆地：L1（外环 + 内环作洞）、L3 湖中岛、L5 南极冰缘。
        for (lv, inner) in [("L1", true), ("L3", false), ("L5", false)] {
            let suffix = format!("GSHHS_shp/{res}/GSHHS_{res}_{lv}.shp");
            let bytes = archive(zip, &suffix)?;
            if lv == "L1" {
                l1see = true;
            }
            let mut records = 0usize;
            let mut points = 0usize;
            shpfile(&bytes, |record| {
                records += 1;
                for (i, part) in record.iter().enumerate() {
                    points += part.len();
                    let holed = inner && i > 0;
                    for piece in dateline(part) {
                        if piece.len() < 4 {
                            continue;
                        }
                        let r = if holed { normalcw(piece) } else { normalccw(piece) };
                        total += r.len();
                        rings.push(r);
                    }
                }
            })?;
            report(format!("  {lv}: {records} records / {points} points"));
        }

        // 水域：L2 湖泊、L4 岛中池 ⇒ 洞（顺时针）。
        for lv in ["L2", "L4"] {
            let suffix = format!("GSHHS_shp/{res}/GSHHS_{res}_{lv}.shp");
            let bytes = archive(zip, &suffix)?;
            let mut records = 0usize;
            let mut points = 0usize;
            shpfile(&bytes, |record| {
                records += 1;
                for part in record.iter() {
                    points += part.len();
                    for piece in dateline(part) {
                        if piece.len() < 4 {
                            continue;
                        }
                        let r = normalcw(piece);
                        total += r.len();
                        rings.push(r);
                    }
                }
            })?;
            report(format!("  {lv}: {records} records / {points} points"));
        }

        if !l1see {
            bail!("GSHHG archive has no L1 coastline for resolution {res}");
        }

        let bytes = encodring(&rings, altmax, 1);
        report(format!(
            "  {name}: {} rings / {} points / {:.1} MiB",
            rings.len(),
            total,
            bytes.len() as f64 / 1_048_576.0
        ));
        Ok(Bakedin { name, bytes, rings: rings.len(), points: total })
    }
}


fn archive(zip: &mut ZipArchive<File>, suffix: &str) -> Result<Vec<u8>> {
    let name = zip.file_names()
        .find(|n| n.ends_with(suffix))
        .map(ToOwned::to_owned);

    if let Some(name) = name {
        let mut file = zip.by_name(&name)?;
        let mut bytes = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    } else {
        Err(anyhow!("GSHHG archive is missing {suffix}"))
    }
}

/// 读一个 `.shp`，逐记录回调 `记录 → 部件(环) → 点 [lon, lat]`。
///
/// 只处理面类型（5/15/25）；三种类型的 parts/points 布局相同，后面的 Z/M 数据忽略。
fn shpfile(buf: &[u8], mut once: impl FnMut(Vec<Vec<[f64; 2]>>)) -> Result<()> {
    if buf.len() < 100 || u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) != 9994 {
        bail!("not a shapefile");
    }
    let mut o = 100usize;
    while o + 8 <= buf.len() {
        let words = u32::from_be_bytes([buf[o + 4], buf[o + 5], buf[o + 6], buf[o + 7]]) as usize;
        let clen = words * 2;
        let c = o + 8;
        if c + clen > buf.len() || clen < 4 {
            break;
        }
        let stype = u32::from_le_bytes([buf[c], buf[c + 1], buf[c + 2], buf[c + 3]]);
        if stype == 5 || stype == 15 || stype == 25 {
            let mut p = c + 4 + 32; // 跳过 bbox(4×f64)
            let nparts = u32::from_le_bytes([buf[p], buf[p + 1], buf[p + 2], buf[p + 3]]) as usize;
            p += 4;
            let npoints = u32::from_le_bytes([buf[p], buf[p + 1], buf[p + 2], buf[p + 3]]) as usize;
            p += 4;
            if p + nparts * 4 + npoints * 16 <= buf.len() {
                let mut parts = Vec::with_capacity(nparts);
                for k in 0..nparts {
                    let q = p + k * 4;
                    parts.push(u32::from_le_bytes([buf[q], buf[q + 1], buf[q + 2], buf[q + 3]]) as usize);
                }
                p += nparts * 4;
                let mut pts = Vec::with_capacity(npoints);
                for k in 0..npoints {
                    let q = p + k * 16;
                    let x = f64::from_le_bytes([
                        buf[q], buf[q + 1], buf[q + 2], buf[q + 3], buf[q + 4], buf[q + 5], buf[q + 6], buf[q + 7],
                    ]);
                    let y = f64::from_le_bytes([
                        buf[q + 8], buf[q + 9], buf[q + 10], buf[q + 11], buf[q + 12], buf[q + 13], buf[q + 14],
                        buf[q + 15],
                    ]);
                    pts.push([x, y]);
                }
                let mut rings = Vec::with_capacity(nparts);
                for k in 0..nparts {
                    let s = parts[k];
                    let e = if k + 1 < nparts { parts[k + 1] } else { npoints };
                    if e > s && e <= pts.len() {
                        rings.push(pts[s..e].to_vec());
                    }
                }
                if !rings.is_empty() {
                    once(rings);
                }
            }
        }
        o = c + clen;
    }
    Ok(())
}

/// 鞋带公式（lon/lat 平面，lat 朝上）：逆时针为正。
fn shoelace(r: &[[f64; 2]]) -> f64 {
    let mut s = 0.0;
    for i in 0..r.len() {
        let a = r[i];
        let b = r[(i + 1) % r.len()];
        s += a[0] * b[1] - b[0] * a[1];
    }
    s * 0.5
}

/// 把环的经度展开成连续路径，再按 ±180 切成若干片。
///
/// 不切的话，跨日界线的环会在图上拉出一条横跨全球的直线。
fn dateline(r: &[[f64; 2]]) -> Vec<Vec<[f64; 2]>> {
    // 1) 展开
    let mut path: Vec<[f64; 2]> = Vec::with_capacity(r.len() + 1);
    let mut prev = r[0][0];
    path.push(r[0]);
    for p in &r[1..] {
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
    // 闭合
    {
        let mut x = r[0][0];
        while x - prev > 180.0 {
            x -= 360.0;
        }
        while x - prev < -180.0 {
            x += 360.0;
        }
        path.push([x, r[0][1]]);
    }
    // 2) 若没跨出 ±180，原样返回（注意去掉重复的收口点）
    let lo = path.iter().fold(f64::MAX, |m, p| m.min(p[0]));
    let hi = path.iter().fold(f64::MIN, |m, p| m.max(p[0]));
    if lo >= -180.0 && hi <= 180.0 {
        let mut out = path;
        out.pop();
        return vec![out];
    }
    // 3) 按 ±180 切：用 Sutherland–Hodgman 分别与两条半平面求交
    let mut pieces = Vec::new();
    for shift in [-360.0f64, 0.0, 360.0] {
        let mut cur: Vec<[f64; 2]> = Vec::new();
        for i in 0..path.len() {
            let a = [path[i][0] + shift, path[i][1]];
            let b = [path[(i + 1) % path.len()][0] + shift, path[(i + 1) % path.len()][1]];
            let ain = a[0] >= -180.0 && a[0] <= 180.0;
            let bin = b[0] >= -180.0 && b[0] <= 180.0;
            if ain {
                cur.push(a);
            }
            if ain != bin {
                let x = if bin { 180.0 } else { -180.0 };
                let t = (x - a[0]) / (b[0] - a[0]);
                cur.push([x, a[1] + t * (b[1] - a[1])]);
            }
        }
        if cur.len() >= 3 {
            pieces.push(cur);
        }
    }
    pieces
}

fn normalccw(mut r: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    if shoelace(&r) < 0.0 {
        r.reverse();
    }
    r
}

fn normalcw(mut r: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    if shoelace(&r) > 0.0 {
        r.reverse();
    }
    r
}

fn encodring(rings: &[Vec<[f64; 2]>], altmax: f32, ver: u16) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&(rings.len() as u32).to_le_bytes());
    for r in rings {
        payload.extend_from_slice(&(r.len() as u32).to_le_bytes());
    }
    let ud: f64 = 1_000_000.0;
    for r in rings {
        let (mut plon, mut plat) = (0i32, 0i32);
        for p in r {
            let lon = (p[0] * ud).round() as i32;
            let lat = (p[1] * ud).round() as i32;
            payload.extend_from_slice(&lon.wrapping_sub(plon).to_le_bytes());
            payload.extend_from_slice(&lat.wrapping_sub(plat).to_le_bytes());
            plon = lon;
            plat = lat;
        }
    }
    let points: u32 = rings.iter().map(|r| r.len() as u32).sum();
    let crc = xxh32(&payload, 0);
    let header = Header::globe(ver, altmax, points, crc).encode();
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(&header);
    out.extend_from_slice(&payload);
    out
}
