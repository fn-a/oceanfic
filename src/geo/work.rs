use std::time::Instant;
use std::thread::Builder;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Sender, Receiver, channel};
use bevy::prelude::*;
use super::mask::{rasterbox, simplify};
use super::{GeoPoint, GeoCubid};

/// 交给 worker 的任务：**只带 Arc**，不深拷贝几何。
// 后台栅格化任务
struct Job {
    id: u64,
    cubd: GeoCubid,
    rings: Arc<Vec<Vec<GeoPoint>>>,
    size: u32,
    feather: u32,
    land: [f32; 3],
    ocean: [f32; 3],
}

/// worker 交回的结果。
pub struct Done {
    pub id: u64,
    pub cubd: GeoCubid,
    pub rgba: Vec<u8>,
    pub rings: usize,
    pub points: usize,
    pub ms: u128,
}

/// 后台栅格化线程：简化与扫描线填充都在这里，主线程只上传纹理。
#[derive(Resource)]
pub struct Worker {
    tx: Sender<Job>,
    rx: Mutex<Receiver<Done>>,
    inflight: bool,
    next_id: u64,
    last_ms: u128,
    count: u32,
}

impl Worker {
    pub fn spawn() -> Self {
        let (tx, rx) = channel::<Job>();
        let (done_tx, done_rx) = channel::<Done>();
        let spawned = Builder::new()
            .name("oceanfic-fill".into())
            .spawn(move || {
                // 简化容差（单位：texel）。取 1 texel 时填充边界可以偏离真实岸线 ±1 texel，
                // 那是 2 像素级的错位，藏不到 1.7 px 的描边下面；取 0.3 把它压到亚像素。
                let patch: f64 = 0.3;
                while let Ok(job) = rx.recv() {
                    let t0 = Instant::now();
                    let size = job.size.max(2);
                    // 按补丁分辨率简化。容差是 0.3 texel：留 1 texel 的话填充边界会
                    // 偏离真实岸线 ±1 texel（约 2 像素），和全分辨率的描边对不上。
                    let tol = ((job.cubd.lon1 - job.cubd.lon0).abs() / size as f64)
                        .max((job.cubd.lat1 - job.cubd.lat0).abs() / size as f64) * patch;
                    let mut points = 0usize;
                    let rigll: Vec<Vec<[f64; 2]>> = job
                        .rings
                        .iter()
                        .map(|r| {
                            let s = simplify(r, tol);
                            points += s.len();
                            s.iter().map(|p| [p.lon, p.lat]).collect()
                        })
                        .collect();
                    let m = rasterbox(&rigll, job.cubd, size, size);
                    let rgba = m.rgbaf(job.land, job.ocean, job.feather);
                    let done = Done {
                        id: job.id,
                        cubd: job.cubd,
                        rgba,
                        rings: rigll.len(),
                        points,
                        ms: t0.elapsed().as_millis(),
                    };
                    if done_tx.send(done).is_err() {
                        break;
                    }
                }
            });
        if spawned.is_err() {
            warn!("fill: cannot spawn raster worker; rasterizing on the main thread");
        }
        Self {
            tx,
            rx: Mutex::new(done_rx),
            inflight: false,
            next_id: 1,
            last_ms: 0,
            count: 0,
        }
    }

    /// 空闲时提交一个任务；返回是否提交成功。
    pub fn submit(
        &mut self,
        cubd: GeoCubid,
        rings: Arc<Vec<Vec<GeoPoint>>>,
        size: u32,
        feather: u32,
        land: [f32; 3],
        ocean: [f32; 3],
    ) -> bool {
        if self.inflight {
            false
        } else {
            let id = self.next_id;
            let job = Job { id, cubd, rings, size, feather, land, ocean };
            if self.tx.send(job).is_err() {
                false
            } else {
                self.next_id += 1;
                self.inflight = true;
                true
            }
        }
    }

    /// 取回已完成的结果（至多返回最后一个）。
    pub fn drain(&mut self) -> Option<Done> {
        let rx = self.rx.lock().ok()?;
        let mut latest = None;
        while let Ok(d) = rx.try_recv() {
            self.inflight = false;
            self.last_ms = d.ms;
            self.count += 1;
            latest = Some(d);
        }
        latest
    }

    pub fn flying(&self) -> bool {
        self.inflight
    }
}