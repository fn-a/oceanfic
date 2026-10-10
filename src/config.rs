//! 配置：编译期内置默认值。

use std::fs;
use std::env;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::fmt::{Display, Formatter, Result};
use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};

/// `config/manifest.json`：只放**呈现选择**（配色 / 光照 / 初始相机位姿）。
#[derive(Debug, Clone, Serialize, Deserialize, Resource)]
pub struct Manifest {
    /// 光照模式：`realistic` 有昼夜，`uniform` 整球均匀照亮。
    #[serde(default)]
    pub lighting: Lighting,
    /// 启动时的相机位姿。
    #[serde(default)]
    pub camera: Initialm,
    #[serde(default)]
    pub colors: Colors,
}

/// 昼夜处理方式。运行时可切换（默认键 `L`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Resource)]
#[serde(rename_all = "lowercase")]
pub enum Lighting {
    /// 平行光 + 环境光：有昼夜明暗。
    Realistic,
    /// 只有环境光：整球一样亮，没有昼夜区分。
    Uniform,
}

/// 启动相机位姿（度 / 世界单位）。三项都可省，缺省用内置默认。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct Initialm {
    pub dist: Option<f32>,
    pub yaw_deg: Option<f32>,
    pub pitch_deg: Option<f32>,
}

impl Default for Lighting {
    fn default() -> Self {
        Self::Realistic
    }
}

impl Lighting {
    pub fn toggled(self) -> Self {
        match self {
            Self::Realistic => Self::Uniform,
            Self::Uniform => Self::Realistic,
        }
    }
}

impl Display for Lighting {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let strv = match self {
            Self::Realistic => "realistic",
            Self::Uniform => "uniform",
        };
        f.write_str(strv)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Colors {
    pub ocean: String,
    /// 矢量海岸线（描边）。
    pub land_line: String,
    /// 陆地填充。与海洋必须有明显区分。
    pub land_fill: String,
    pub grid: String,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            ocean: "#0d2666".to_string(),
            land_line: "#9fd98a".to_string(),
            land_fill: "#3f6b45".to_string(),
            grid: "#a9c6e2".to_string(),
        }
    }
}

impl Colors {
    /// `#rrggbb` → sRGB `[r,g,b]`（每分量 0..1），可直接给 `Color::srgb`。
    pub fn parsex(s: &str) -> Option<[f32; 3]> {
        let h = s.strip_prefix('#').unwrap_or(s);
        if h.len() != 6 {
            None
        } else {
            let r = u8::from_str_radix(&h[0..2], 16).ok()?;
            let g = u8::from_str_radix(&h[2..4], 16).ok()?;
            let b = u8::from_str_radix(&h[4..6], 16).ok()?;
            Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0])
        }
    }
}

impl Default for Manifest {
    fn default() -> Self {
        Self::builtin()
    }
}

impl Manifest {
    const RELPATH: &str = "config/manifest.json";

    /// 编译期内置默认值。
    pub fn builtin() -> Self {
        Self {
            lighting: Lighting::default(),
            camera: Initialm::default(),
            colors: Colors::default(),
        }
    }

    /// 读 `config/manifest.json`；文件不存在或解析失败都回退内置默认，
    /// 并把原因作为告警返回（不 panic —— 零配置也必须能出图）。
    pub fn loador(path: &Path) -> (Self, Option<String>) {
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == ErrorKind::NotFound => return (Self::builtin(), None),
            Err(e) => {
                return (
                    Self::builtin(),
                    Some(format!("cannot read {}: {e}; falling back to builtin defaults", path.display())),
                );
            }
        };
        match serde_json::from_slice::<Self>(&bytes) {
            Ok(m) => (m, None),
            Err(e) => (
                Self::builtin(),
                Some(format!("cannot parse {}: {e}; falling back to builtin defaults", path.display())),
            ),
        }
    }

    /// 从程序目录读；不存在则内置默认。
    pub fn loadir() -> (Self, Option<String>) {
        // 依次尝试：**exe 同级**的 `config/`（发布形态）、再当前工作目录（`cargo run`）。
        let path = execfg();
        if path.is_file() {
            Self::loador(&path)
        } else {
            let path = cwdcfg();
            if path.is_file() {
                let (m, warn) = Self::loador(&path);
                (m, warn)
            } else {
                (Self::builtin(), None)
            }
        }
    }
}

/// 程序路径查找配置
pub fn execfg() -> PathBuf {
    exedir().join(Manifest::RELPATH)
}

/// 执行路径查找配置
pub fn cwdcfg() -> PathBuf {
    PathBuf::from(Manifest::RELPATH)
}

/// 可执行文件所在目录；取不到则当前目录（测试环境）。
pub fn exedir() -> PathBuf {
    env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 资产查找：先看 **exe 同级** 的 `assets/`（发布形态就是 exe + config/ + assets/），
/// 再看当前工作目录。
pub fn assetp(name: &str) -> PathBuf {
    let cand = exedir().join("assets").join(name);
    if cand.exists() {
        return cand;
    } else {
        Path::new("assets").join(name)
    }
}