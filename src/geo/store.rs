//! 底图资产：二进制格式（编解码 + 校验）与解码后的数据（两级环 + 桶索引 + 按盒查询）。

use std::fs;
use std::path::Path;
use bevy::prelude::Resource;
use anyhow::{Result, anyhow};
use crate::config::assetp;
use crate::geo::{GeoPoint, GeoCubid};

pub const HEADER_LEN: usize = 24;
pub const MAGICB_STR: [u8; 4] = *b"OFBM";
/// 桶边长（度）。1° 在赤道约 111 km，取视野盒时再加余量，够用且桶数适中（360×180）。
pub const BUCKET_DEG: f64 = 1.0;

const FLAG_RAW: u8 = 0;
/// 微度换算：i32 微度 ≈ 0.11 m 分辨率。
const UNIT_DPI: f64 = 1_000_000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Header {
    pub magic: [u8; 4],
    pub ver: u16,
    /// 底图的可信缩放上限（低于它启用该档）
    pub altmax: f32,
    pub points: u32,
    pub crc: u32,
    pub flags: u8,
}

impl Header {
    pub fn globe(ver: u16, altmax: f32, points: u32, crc: u32) -> Self {
        Self { magic: MAGICB_STR, ver, altmax, points, crc, flags: FLAG_RAW }
    }

    pub fn encode(&self) -> [u8; HEADER_LEN] {
        let mut b = [0u8; HEADER_LEN];
        b[0..4].copy_from_slice(&self.magic);
        b[4..6].copy_from_slice(&self.ver.to_le_bytes());
        b[8..12].copy_from_slice(&self.altmax.to_bits().to_le_bytes());
        b[12..16].copy_from_slice(&self.points.to_le_bytes());
        b[16..20].copy_from_slice(&self.crc.to_le_bytes());
        b[20] = self.flags;
        b
    }

    pub fn decode(buf: &[u8]) -> Result<Self> {
        if buf.len() < HEADER_LEN {
            Err(anyhow!("payload too short: {} bytes, need at least {}", buf.len(), HEADER_LEN))
        } else {
            let mut magic = [0u8; 4];
            magic.copy_from_slice(&buf[0..4]);
            // 两种资产共用头部：底图（OFBM）与陆地遮罩（OFLM）
            if magic != MAGICB_STR {
                Err(anyhow!("bad magic: expected {:?}, got {:?}", MAGICB_STR, magic))
            } else {
                let ver = u16::from_le_bytes([buf[4], buf[5]]);
                let meta1 = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
                let points = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);
                let crc = u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]);
                let flags = buf[20];
                Ok(Self { magic, ver, altmax: f32::from_bits(meta1), points, crc, flags })
            }
        }
    }
}

const P1: u32 = 0x9E37_79B1;
const P2: u32 = 0x85EB_CA77;
const P3: u32 = 0xC2B2_AE3D;
const P4: u32 = 0x27D4_EB2F;
const P5: u32 = 0x1656_67B1;

/// 标准 xxHash32。用作载荷校验（抓截断与位翻转）。
fn xxh32(data: &[u8], seed: u32) -> u32 {
    let len = data.len();
    let mut i = 0usize;
    let mut h: u32;
    if len >= 16 {
        let mut v1 = seed.wrapping_add(P1).wrapping_add(P2);
        let mut v2 = seed.wrapping_add(P2);
        let mut v3 = seed;
        let mut v4 = seed.wrapping_sub(P1);
        while i + 16 <= len {
            v1 = round(v1, lane(data, i));
            v2 = round(v2, lane(data, i + 4));
            v3 = round(v3, lane(data, i + 8));
            v4 = round(v4, lane(data, i + 12));
            i += 16;
        }
        h = v1
            .rotate_left(1)
            .wrapping_add(v2.rotate_left(7))
            .wrapping_add(v3.rotate_left(12))
            .wrapping_add(v4.rotate_left(18));
    } else {
        h = seed.wrapping_add(P5);
    }
    h = h.wrapping_add(len as u32);
    while i + 4 <= len {
        h = h.wrapping_add(lane(data, i).wrapping_mul(P3)).rotate_left(17).wrapping_mul(P4);
        i += 4;
    }
    while i < len {
        h = h.wrapping_add((data[i] as u32).wrapping_mul(P5)).rotate_left(11).wrapping_mul(P1);
        i += 1;
    }
    h ^= h >> 15;
    h = h.wrapping_mul(P2);
    h ^= h >> 13;
    h = h.wrapping_mul(P3);
    h ^= h >> 16;
    h
}

fn round(acc: u32, input: u32) -> u32 {
    acc.wrapping_add(input.wrapping_mul(P2)).rotate_left(13).wrapping_mul(P1)
}

fn lane(d: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]])
}

/// 底图数据（内嵌）。
#[derive(Debug, Clone)]
pub struct Embmap {
    pub ver: u16,
    pub altmax: f32,
    pub lines: Vec<Vec<GeoPoint>>,
}

impl Embmap {
    pub fn pcount(&self) -> usize {
        self.lines.iter().map(|l| l.len()).sum()
    }

    pub fn encode(lines: &[Vec<GeoPoint>], altmax: f32, ver: u16) -> Vec<u8> {
        let payload = {
            let total: usize = lines.iter().map(|l| l.len()).sum();
            let mut out = Vec::with_capacity(4 + 4 * lines.len() + total * 8);
            out.extend_from_slice(&(lines.len() as u32).to_le_bytes());
            for l in lines {
                out.extend_from_slice(&(l.len() as u32).to_le_bytes());
            }
            for l in lines {
                let (mut plon, mut plat) = (0i32, 0i32);
                for p in l {
                    let p = p.normalized();
                    let lon = (p.lon * UNIT_DPI).round() as i32;
                    let lat = (p.lat * UNIT_DPI).round() as i32;
                    out.extend_from_slice(&lon.wrapping_sub(plon).to_le_bytes());
                    out.extend_from_slice(&lat.wrapping_sub(plat).to_le_bytes());
                    plon = lon;
                    plat = lat;
                }
            }
            out
        };
        let points: u32 = lines.iter().map(|l| l.len() as u32).sum();
        let crc = xxh32(&payload, 0);
        let h = Header::globe(ver, altmax, points, crc).encode();
        let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
        out.extend_from_slice(&h);
        out.extend_from_slice(&payload);
        out
    }
    
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let h = Header::decode(buf)?;
        if h.magic != MAGICB_STR {
            Err(anyhow!("bad magic: expected {:?}, got {:?}", MAGICB_STR,  h.magic))
        } else if h.flags != FLAG_RAW {
            Err(anyhow!("unsupported flags: {} (supports only 0 = raw)", h.flags))
        } else {
            let payload = &buf[HEADER_LEN..];
            let actual = xxh32(payload, 0);
            if actual != h.crc {
                Err(anyhow!("payload crc mismatch: header {:#010x}, computed {:#010x}", h.crc, actual))
            } else {
                if payload.len() < 4 {
                    Err(anyhow!("payload too short: {} bytes, need at least {}", payload.len(), 4))
                } else {
                    let n = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;
                    let lensend = 4 + n * 4;
                    if payload.len() < lensend {
                        Err(anyhow!("payload too short: {} bytes, need at least {}", payload.len(), lensend))
                    } else {
                        let mut lens = Vec::with_capacity(n);
                        for i in 0..n {
                            let o = 4 + i * 4;
                            let l = u32::from_le_bytes([payload[o], payload[o + 1], payload[o + 2], payload[o + 3]]) as usize;
                            lens.push(l);
                        }
                        let total: usize = lens.iter().sum();
                        let need = lensend + total * 8;
                        if payload.len() < need {
                            Err(anyhow!("payload length mismatch: declared {} bytes, actual {} bytes", need, payload.len()))
                        } else {
                            let mut lines = Vec::with_capacity(n);
                            let mut o = lensend;
                            for len in lens {
                                let mut pts = Vec::with_capacity(len);
                                let (mut lon, mut lat) = (0i32, 0i32);
                                for _ in 0..len {
                                    let dlon = i32::from_le_bytes([payload[o], payload[o + 1], payload[o + 2], payload[o + 3]]);
                                    let dlat =
                                        i32::from_le_bytes([payload[o + 4], payload[o + 5], payload[o + 6], payload[o + 7]]);
                                    o += 8;
                                    lon = lon.wrapping_add(dlon);
                                    lat = lat.wrapping_add(dlat);
                                    pts.push(GeoPoint::new(lat as f64 / UNIT_DPI, lon as f64 / UNIT_DPI).normalized());
                                }
                                lines.push(pts);
                            }
                            Ok(Self { ver: h.ver, altmax: h.altmax, lines })
                        }
                    }
                }
            }
        }
    }
}

/// 一档底图数据。
pub struct Level {
    pub ver: u16,
    pub altmax: f32,
    pub lines: Vec<Vec<GeoPoint>>,
    /// 桶 → 环索引（`buckets[by * 360 + bx]`）
    buckets: Vec<Vec<u32>>,
    /// 包围盒过大的环（南极、欧亚大陆那种），任何查询都要考虑
    big: Vec<u32>,
    /// 每环的外环标志（缓存鞋带面积的符号）
    outer: Vec<bool>,
}

impl Level {
    /// 从资产文件加载，并建好桶索引。
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = fs::read(path)?;
        let ebmap = Embmap::decode(&bytes)?;

        // 由环数据构建（含外环判定与 1°x1° 桶索引）。
        let outer: Vec<bool> = ebmap.lines.iter().map(|l| GeoPoint::signedarea(l) >= 0.0).collect();
        let nx = (360.0 / BUCKET_DEG).round() as usize;
        let ny = (180.0 / BUCKET_DEG).round() as usize;
        let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); nx * ny];
        let mut big: Vec<u32> = Vec::new();
        for (i, l) in ebmap.lines.iter().enumerate() {
            if l.is_empty() {
                continue;
            }
            let (mut lo_x, mut hi_x) = (f64::MAX, f64::MIN);
            let (mut lo_y, mut hi_y) = (f64::MAX, f64::MIN);
            for p in l {
                lo_x = lo_x.min(p.lon);
                hi_x = hi_x.max(p.lon);
                lo_y = lo_y.min(p.lat);
                hi_y = hi_y.max(p.lat);
            }
            let (bx0, bx1) = (bucket_x(lo_x, nx), bucket_x(hi_x, nx));
            let (by0, by1) = (bucket_y(lo_y, ny), bucket_y(hi_y, ny));
            // 一档数据最多占多少桶（超过就进"大环"表，避免南极那种整圈包围盒把桶表撑爆）。
            if (bx1 - bx0 + 1) * (by1 - by0 + 1) > 2048 {
                big.push(i as u32); // 跨桶太多的环（如南极）单独走线性扫描
                continue;
            }
            for by in by0..=by1 {
                for bx in bx0..=bx1 {
                    buckets[by * nx + bx].push(i as u32);
                }
            }
        }
        
        Ok(Self { ver: ebmap.ver, altmax: ebmap.altmax, lines: ebmap.lines, buckets, big, outer })
    }

    pub fn rcount(&self) -> usize {
        self.lines.len()
    }

    pub fn pcount(&self) -> usize {
        self.lines.iter().map(|l| l.len()).sum()
    }

    pub fn outside(&self, i: usize) -> bool {
        self.outer.get(i).copied().unwrap_or(true)
    }

    /// 取出与经纬度盒相交的环索引（盒子可跨 ±180）。
    pub fn ringsin(&self, cubd: GeoCubid) -> Vec<u32> {
        let nx = 360usize;
        let ny = 180usize;
        let lat0 = cubd.lat0.clamp(-90.0, 90.0);
        let lat1 = cubd.lat1.clamp(-90.0, 90.0);
        let mut out: Vec<u32> = self.big.clone();
        let mut mark: std::collections::HashSet<u32> = out.iter().copied().collect();

        let mut push_box = |a: f64, b: f64| {
            let bx0 = bucket_x(a, nx);
            let bx1 = bucket_x(b, nx);
            let by0 = bucket_y(lat0, ny);
            let by1 = bucket_y(lat1, ny);
            for by in by0.min(by1)..=by0.max(by1) {
                for bx in bx0.min(bx1)..=bx0.max(bx1) {
                    for id in &self.buckets[by * nx + bx] {
                        if mark.insert(*id) {
                            out.push(*id);
                        }
                    }
                }
            }
        };
        if cubd.lon0 <= cubd.lon1 {
            push_box(cubd.lon0, cubd.lon1);
        } else {
            // 跨 ±180：拆成东、西两段
            push_box(cubd.lon0, 180.0);
            push_box(-180.0, cubd.lon1);
        }
        out
    }
}

pub fn bucket_x(lon: f64, nx: usize) -> usize {
    let v = ((lon + 180.0) / BUCKET_DEG).floor();
    (v.max(0.0) as usize).min(nx - 1)
}
pub fn bucket_y(lat: f64, ny: usize) -> usize {
    let v = ((lat + 90.0) / BUCKET_DEG).floor();
    (v.max(0.0) as usize).min(ny - 1)
}

/// 细节档（full 分辨率）。视高低于其 `alt_max` 时启用。
pub const COAST_HI_BIN: &str = "coasthi.bin";
/// 全球档（intermediate）。远景与兜底。
pub const COAST_LO_BIN: &str = "coastlo.bin";

/// 两档底图。
#[derive(Resource)]
pub struct Levels {
    pub lo: Level,
    pub hi: Level,
}

impl Levels {
    /// 加载随仓库分发的两个资产（85 MB，启动时读一次；解码后常驻内存约 180 MB）。
    pub fn load() -> Result<Self> {
        Ok(Self {
            lo: Level::load(&assetp(COAST_LO_BIN))?,
            hi: Level::load(&assetp(COAST_HI_BIN))?,
        })
    }

    /// 按视高选档：低于细节档 `altmax` 用细节档，否则全球档。
    pub fn pick(&self, alt: f32) -> &Level {
        if alt < self.hi.altmax {
            &self.hi
        } else {
            &self.lo
        }
    }
}
