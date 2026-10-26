//! 底图兜底：资产缺失时后台下载 GSHHG、缓存、烘焙并热加载。
//!
//! 启动时若 `assets/` 中没有可用资产，主线程照常启动（只是暂时没有海岸线数据），后台线程负责：
//!   1. 探测 SOEST / GitHub 两条下载路线，选更快的；
//!   2. 下载 `gshhg-shp-2.3.7.zip` 到缓存目录（支持断点续传）；
//!   3. 从 zip 中取出所需的 L1–L5 shapefile，运行时烘焙成 `assets/*.bin`；
//!   4. 把加载好的 `Levels` 交回主线程热加载。

use std::sync::Mutex;
use std::thread::Builder;
use std::io::{Read, Write, stdout};
use std::path::{Path, PathBuf};
use std::fs::{self, OpenOptions};
use std::time::{Duration, Instant};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use bevy::prelude::*;
use ureq::Agent;
use ureq::http::HeaderMap;
use ureq::tls::{TlsConfig, TlsProvider};
use anyhow::{Result, anyhow, bail};
use crate::config::{assetdir, cachedir};
use super::store::Levels;
use super::bake::{Targets, Bakedin};

const GSHHG_VER: &str = "2.3.7";
const GSHHG_ZIP: &str = "gshhg-shp-2.3.7.zip";
/// 2.3.7 shapefile 包的精确大小（SOEST 与 GitHub 是同一份文件）。
const GSHHG_BIT: u64 = 149_157_845;

#[derive(Debug, Clone, Copy)]
struct Route {
    name: &'static str,
    url: &'static str,
}

impl Route {
    const GITHUB: Route = Route {
        name: "GitHub release",
        url: "https://github.com/GenericMappingTools/gshhg-gmt/releases/download/2.3.7/gshhg-shp-2.3.7.zip",
    };

    const SOEST: Route = Route {
        name: "SOEST (University of Hawaii)",
        url: "https://www.soest.hawaii.edu/pwessel/gshhg/gshhg-shp-2.3.7.zip",
    };

    fn probe(self) -> Result<f64> {
        let agent = reqagent(true);
        let t0 = Instant::now();
        let res = agent
            .get(self.url)
            .header("Range", "bytes=0-262143")
            .call()
            .map_err(|e| anyhow!(e.to_string()))?;
        let status = res.status().as_u16();
        if !(200..300).contains(&status) {
            bail!("HTTP {status}");
        }
        let mut reader = res.into_body().into_reader();
        let mut buf = [0u8; 32 * 1024];
        let mut got = 0usize;
        while got < 256 * 1024 {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => got += n,
                Err(e) if got >= 32 * 1024 => {
                    let _ = e;
                    break;
                }
                Err(e) => return Err(anyhow!(e.to_string())),
            }
            if t0.elapsed() > Duration::from_secs(15) {
                break;
            }
        }
        if got < 32 * 1024 {
            bail!("too slow or no data ({got} bytes)");
        }
        Ok(got as f64 / t0.elapsed().as_secs_f64().max(1e-6))
    }

    /// 探测已验证路线并按时下载速度排序（最快在末尾，便于 `pop`）。
    fn routes() -> Result<Vec<Self>> {
        let mut found: Vec<(f64, Self)> = Vec::new();
        for route in [Self::GITHUB, Self::SOEST] {
            match route.probe() {
                Ok(speed) => {
                    Progress::note(&format!(
                        "route {}: {:.0} KiB/s",
                        route.name,
                        speed / 1024.0
                    ));
                    found.push((speed, route));
                }
                Err(e) => Progress::note(&format!("route {} unavailable: {e}", route.name)),
            }
        }
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(found.into_iter().map(|(_, r)| r).collect())
    }

    fn ensure() -> Result<PathBuf> {
        let cache = cachedir();
        fs::create_dir_all(&cache)?;
        let zip = cache.join(GSHHG_ZIP);
        let part = cache.join(format!("{GSHHG_ZIP}.part"));

        if let Ok(meta) = fs::metadata(&zip) {
            if meta.len() == GSHHG_BIT {
                Progress::note(&format!("cache hit: {}", zip.display()));
                return Ok(zip);
            }
            Progress::note(&format!(
                "cache size mismatch ({} != {}); re-downloading",
                meta.len(),
                GSHHG_BIT
            ));
            let _ = fs::remove_file(&zip);
        }

        if let Ok(meta) = fs::metadata(&part) {
            if meta.len() == GSHHG_BIT {
                fs::rename(&part, &zip)?;
                Progress::note(&format!("resumed complete download: {}", zip.display()));
                return Ok(zip);
            }
            if meta.len() > GSHHG_BIT {
                let _ = fs::remove_file(&part);
            }
        }

        let mut routes = Self::routes()?;
        let mut istried = false;
        loop {
            let Some(route) = routes.pop() else {
                if istried {
                    bail!(
                        "all download routes failed; you can fetch {} manually from {} and place it in {}",
                        GSHHG_ZIP,
                        Route::GITHUB.url,
                        cache.display()
                    );
                }
                istried = true;
                Progress::note("verified routes failed; retrying SOEST with certificate verification disabled");
                routes.push(Route::SOEST);
                continue;
            };
            Progress::note(&format!("downloading from {} ...", route.name));
            match route.download(&part) {
                Ok(()) => {
                    let len = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
                    if len != GSHHG_BIT {
                        Progress::note(&format!("warning: downloaded size {len} != expected {GSHHG_BIT}; retrying"));
                        let _ = fs::remove_file(&part);
                        continue;
                    }
                    fs::rename(&part, &zip)?;
                    Progress::note(&format!("downloaded {:.1} MiB", len as f64 / 1_048_576.0));
                    return Ok(zip);
                }
                Err(e) => {
                    Progress::note(&format!("route {} failed: {e:#}", route.name));
                }
            }
        }
    }

    fn download(&self, part: &Path) -> Result<()> {
        let mut start = fs::metadata(part).map(|m| m.len()).unwrap_or(0);
        if start > GSHHG_BIT {
            let _ = fs::remove_file(part);
            start = 0;
        }
        if start != GSHHG_BIT {
            let agent = reqagent(false);
            let mut req = agent.get(self.url);
            if start > 0 { req = req.header("Range", format!("bytes={start}-")); }
            let res = req.call().map_err(|e| anyhow!(e.to_string()))?;
            let status = res.status().as_u16();
            let append = start > 0 && status == 206;
            if !(status == 416 && start >= GSHHG_BIT) {
                if status != 200 && status != 206 {
                    bail!("HTTP {status}");
                }
                let total = contotal(res.headers()).unwrap_or(GSHHG_BIT);
                if !append {
                    start = 0;
                }
                let mut file = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .append(append)
                    .truncate(!append)
                    .open(part)?;
                if !append {
                    file.set_len(0)?;
                }
        
                let mut reader = res.into_body().into_reader();
                let _progress = Progress;
                let mut buf = vec![0u8; 128 * 1024];
                let mut done = start;
                let mut last = Instant::now();
                let mut last_done = done;
                loop {
                    let n = reader.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    file.write_all(&buf[..n])?;
                    done += n as u64;
                    if last.elapsed() >= Duration::from_millis(500) || done >= total {
                        let speed = (done - last_done) as f64 / last.elapsed().as_secs_f64().max(1e-6);
                        let mibit = 1_048_576.0;
                        Progress::note(&format!(
                            "downloading {:5.1}% ({:.1}/{:.1} MiB, {:.1} MiB/s)",
                            done as f64 / total.max(1) as f64 * 100.0,
                            done as f64 / mibit,
                            total as f64 / mibit,
                            speed / mibit
                        ));
                        last = Instant::now();
                        last_done = done;
                    }
                }
                file.flush()?;
                if done != total {
                    bail!("incomplete download ({done}/{total} bytes)");
                }
            }
        }

        Ok(())
    }
}


/// 后台任务 → 主线程的消息。
enum Info {
    Ready(Result<Levels, String>),
}

enum Poll {
    Message(Info),
    Empty,
    Disconnected,
}

/// 后台下载/烘焙任务（存在时表示底图还缺数据）。
#[derive(Resource)]
pub struct Quest {
    rx: Mutex<Receiver<Info>>,
    done: bool,
}

impl Quest {
    fn spawn(targets: Targets) -> Result<Self> {
        let (tx, rx) = channel();
        Builder::new()
            .name("oceanfic-basemap".into())
            .spawn(move || {
                let result = targets.bootup().map_err(|e| format!("{e:#}"));
                let _ = tx.send(Info::Ready(result));
            })?;
        Ok(Self { rx: Mutex::new(rx), done: false })
    }

    /// 缺哪档就在后台补哪档。资产齐全时什么也不做。
    pub fn intend(levels: &Levels) -> Option<Self> {
        let targets = Targets { hi: levels.hi.is_none(), lo: levels.lo.is_none() };
        if targets.any() {
            info!(
                "basemap: {} (assets missing/invalid); fetching GSHHG {} in background",
                levels.counts(),
                GSHHG_VER
            );
            match Quest::spawn(targets) {
                Ok(task) => Some(task),
                Err(e) => {
                    error!("basemap: cannot start background fetch: {e}");
                    None
                },
            }
        } else {
            None
        }
    }

    /// 后台完成后热加载，并让渲染系统按新代次重建。
    pub fn pumpup(&mut self, levels: &Levels) -> Option<Levels> {
        if !self.done {
            let msg = match self.rx.lock() {
                Ok(rx) => match rx.try_recv() {
                    Ok(msg) => Poll::Message(msg),
                    Err(TryRecvError::Empty) => Poll::Empty,
                    Err(TryRecvError::Disconnected) => Poll::Disconnected,
                },
                Err(_) => Poll::Disconnected,
            };
            let msg = match msg {
                Poll::Message(msg) => msg,
                Poll::Empty => return None,
                Poll::Disconnected => {
                    self.done = true;
                    error!("basemap: background task stopped unexpectedly");
                    return None;
                }
            };
            let Info::Ready(result) = msg;
            self.done = true;
            match result {
                Ok(mut newlvs) => {
                    newlvs.epoch = levels.epoch.wrapping_add(1);
                    info!("basemap: hot-loaded {}", newlvs.counts());
                    Some(newlvs)
                }
                Err(e) => {
                    error!("basemap: background fetch failed: {e}");
                    None
                }
            }
        } else {
            None
        }
    }
}

impl Targets {
    fn bootup(self) -> Result<Levels> {
        Progress::note(&format!("starting; cache {}", cachedir().display()));
        let mut zip = Route::ensure()?;
    
        // 生成资产写到哪里：优先已存在的 exe 同级 `assets/`（发布形态），
        // 其次工作目录 `assets/`（`cargo run`），都不存在则在工作目录创建。
        let assets = assetdir();
        fs::create_dir_all(&assets)?;
    
        let baked = match Bakedin::zip(&zip, self, &mut |line| Progress::note(&line)) {
            Ok(baked) => baked,
            Err(e) => {
                // 缓存文件大小正确但内容损坏时（极少数），清掉缓存重下一次。
                Progress::note(&format!("bake failed: {e:#}; discarding cached archive and retrying once"));
                let _ = fs::remove_file(&zip);
                zip = Route::ensure()?;
                Bakedin::zip(&zip, self, &mut |line| Progress::note(&line))?
            }
        };
        for bin in baked {
            let dest = assets.join(bin.name);
            wtatomic(&dest, &bin.bytes)?;
            Progress::note(&format!(
                "wrote {} ({} rings / {} points, {:.1} MiB)",
                dest.display(),
                bin.rings,
                bin.points,
                bin.bytes.len() as f64 / 1_048_576.0
            ));
        }
    
        let (levels, cases) = Levels::load();
        for case in &cases {
            Progress::note(&format!("warning: {case}"));
        }
        if levels.lo.is_none() && levels.hi.is_none() {
            bail!("bake finished but no basemap could be loaded");
        }
        Progress::note(&format!("ready: {}", levels.counts()));
        Ok(levels)
    }
}

fn reqagent(probing: bool) -> Agent {
    let mut builder = Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(12)))
        .timeout_recv_response(Some(Duration::from_secs(25)));
    
    builder = if probing {
        builder.timeout_global(Some(Duration::from_secs(18)))
    } else {
        builder.timeout_per_read(Some(Duration::from_secs(45)))
    };

    builder = builder.tls_config(
        TlsConfig::builder()
            .provider(TlsProvider::NativeTls)
            .disable_verification(true)
            .build(),
    );

    builder.build().new_agent()
}

fn wtatomic(dest: &Path, bytes: &[u8]) -> Result<()> {
    let name = dest
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("invalid asset path {}", dest.display()))?;
    let tmp = dest.with_file_name(format!("{name}.tmp"));
    fs::write(&tmp, bytes)?;
    if dest.exists() {
        fs::remove_file(dest)?;
    }
    fs::rename(&tmp, dest)?;
    Ok(())
}

fn contotal(headers: &HeaderMap) -> Option<u64> {
    if let Some(range) = headers.get("content-range").and_then(|v| v.to_str().ok()) {
        if let Some(total) = range.rsplit('/').next().and_then(|s| s.parse::<u64>().ok()) {
            return Some(total);
        }
    }
    headers
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

/// 结束下载进度行（成功或失败都换行，避免后续日志粘在进度条后面）。
struct Progress;

impl Progress {
    fn note(msg: &str) {
        let mut out = stdout();
        let _ = write!(out, "\r[basemap] {msg}");
        let _ = out.flush();
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        let mut out = stdout();
        let _ = writeln!(out);
        let _ = out.flush();
    }
}
