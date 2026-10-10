//! 轨道相机：目标值 / 当前值双状态 + 帧率无关平滑。

use std::f64::consts::PI;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::camera::{PerspectiveProjection, Projection};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use crate::config::Manifest;
use crate::globe::{AMBIENT_REALISTIC, EARTHR};

/// 相机最远距离（≈ 44,600 km 海拔）。
pub const CAM_MAX: f32 = 8.0;

/// 纯几何下限（防穿模）。
pub const ABS_MIN: f32 = EARTHR + 0.004;

/// 相机最近视高（世界单位，0.01 ≈ 64 km）。与各层抬升量、
/// 最深数据分辨率（GSHHG full 点距 50–250 m）互相咬合。
pub const MIN_ALT: f32 = 0.01;

pub const FOV_DEG: f32 = 45.0;

#[derive(Component)]
pub struct CameraOrbit {
    // 当前值（渲染使用）
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    pub focus: Vec3,
    // 目标值（输入直接修改）
    pub target_yaw: f32,
    pub target_pitch: f32,
    pub target_dist: f32,
    pub target_focus: Vec3,
}

impl Default for CameraOrbit {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.35,
            dist: 3.0,
            focus: Vec3::ZERO,
            target_yaw: 0.0,
            target_pitch: 0.35,
            target_dist: 3.0,
            target_focus: Vec3::ZERO,
        }
    }
}

impl CameraOrbit {
    /// 由 yaw/pitch 得到相机相对焦点的方向（`yaw=0, pitch=0` 时相机位于 **+X** 轴，正对 `lon=0`）。
    pub fn dir(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(cp * cy, sp, -cp * sy)
    }
}

/// 平滑后的相机状态，供细分 / 剔除 / 读数使用。
#[derive(Resource)]
pub struct CameraState {
    pub pos: Vec3,
    pub dist: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub screenh: f32,
    pub altkm: f32,
}

impl CameraState {
    /// 当前视口几何（像素 ↔ 世界尺寸的换算基准）。
    pub fn view(&self) -> CameraViwpt {
        CameraViwpt::from(self)
    }
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            pos: Vec3::new(3.0, 1.0, 0.0),
            dist: 3.0,
            yaw: 0.0,
            pitch: 0.35,
            screenh: 1080.0,
            altkm: (3.0 - EARTHR) * 6371.0,
        }
    }
}

/// 视口几何：垂直视场角 + 视口像素高。**像素 ↔ 世界尺寸的唯一换算基准**
/// （可见经纬度盒、条带的屏幕像素线宽都由它推出）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraViwpt {
    /// 垂直视场角（弧度）。
    pub fov: f32,
    /// 视口高度（像素）。
    pub screenh: f32,
}

impl CameraViwpt {
    /// 距相机 `dist` 处，屏幕上 1 像素对应多少世界单位
    /// （`dist` = 到目标点的距离；要算某个像素线宽，乘上像素数即可）。
    pub fn worldpx(&self, dist: f32) -> f32 {
        let d = dist.max(1e-6);
        2.0 * d * (self.fov * 0.5).tan() / self.screenh
    }
}

impl From<&CameraState> for CameraViwpt {
    fn from(v: &CameraState) -> Self {
        Self { fov: FOV_DEG * (PI / 180.0) as f32, screenh: v.screenh.max(1.0) }
    }
}

/// 相机：显式钉死投影参数，不依赖 Bevy 默认值（默认 fov 45°/near 0.1/far 1000）。
pub fn setup_camera(mut commands: Commands, manifest: Res<Manifest>) {
    commands.insert_resource(CameraState::default());
    
    let mut cam = CameraOrbit::default();
    // 启动视角可由 manifest 指定（演示/截图/"打开就停在某片海域"）
    let ic = manifest.camera;
    if let Some(d) = ic.dist {
        cam.dist = d;
        cam.target_dist = d;
    }
    if let Some(y) = ic.yaw_deg {
        cam.yaw = y.to_radians();
        cam.target_yaw = cam.yaw;
    }
    if let Some(p) = ic.pitch_deg {
        cam.pitch = p.to_radians();
        cam.target_pitch = cam.pitch;
    }
    let pos = cam.dir() * cam.dist;
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: FOV_DEG.to_radians(),
            // 与 Bevy `PerspectiveProjection` 的默认值一致，避免 `near_clip_plane` 错位。
            near: 0.004,
            far: 20.0,
            ..default()
        }),
        AmbientLight { color: Color::WHITE, brightness: AMBIENT_REALISTIC, ..default() },
        Transform::from_translation(pos).looking_at(Vec3::ZERO, Dir3::Y),
        cam,
    ));
}

/// 输入 → 目标值。
pub fn update_camera(
    time: Res<Time>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut q: Query<&mut CameraOrbit>,
) -> Result {
    let mut cam = q.single_mut()?;
    let min = (EARTHR + MIN_ALT).clamp(ABS_MIN, CAM_MAX);
    let limit = 89.0_f32.to_radians();

    // 灵敏度随高度归一化：贴地时同样的拖动只转很小角度
    let height = (cam.dist - EARTHR).max(0.02);
    // 拖拽灵敏度（会随高度归一化）。
    let dragsens: f32 = 0.004;

    if buttons.pressed(MouseButton::Left) && motion.delta != Vec2::ZERO {
        let k = dragsens * height;
        cam.target_yaw -= motion.delta.x * k;
        cam.target_pitch = (cam.target_pitch + motion.delta.y * k).clamp(-limit, limit);
    }

    if buttons.pressed(MouseButton::Right) && motion.delta != Vec2::ZERO {
        let k = dragsens * height * 0.5;
        let dir = cam.dir();
        let right = dir.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(dir).normalize_or_zero();
        let moved = cam.target_focus + (-right * motion.delta.x + up * motion.delta.y) * k;
        // 焦点保持在球面上，避免越推越远
        cam.target_focus = if moved.length() > EARTHR { moved.normalize_or_zero() * EARTHR } else { moved };
    }

    // 缩放：滚轮 + PageUp/PageDown。
    // 键盘要同时处理"按一下"和"按住"：只判 `pressed()` 会漏掉极快的点按
    // （按下与抬起落在同一帧内时，轮询到的状态已经是抬起）。
    let mut zoom = scroll.delta.y;
    let dt = time.delta_secs().clamp(0.0, 0.1);
    for key in [KeyCode::PageUp, KeyCode::PageDown] {
        // Page Up（抬高 ⇒ 拉远）取 −1，Page Down（降低 ⇒ 拉近）取 +1。
        let sign =  match key {
            KeyCode::PageUp => -1.0,
            KeyCode::PageDown => 1.0,
            _ => 0.0,
        };

        if keys.just_pressed(key) {
            // Page Up/Down：单次点按的步进（与滚轮一格等效）与按住时的速率（"格/秒"）。
            zoom += sign * 1.0; // 一档（与滚轮一格等效）
        }
        if keys.pressed(key) {
            // 按住方向键的连续缩放速率（单位/秒，指数式）。14.0 时全程只要 0.2 秒 ⇒ 观感上
            // 没有中间过程；2.5 约 1.1 秒。
            zoom += sign * 2.5 * dt; // 按住则连续
        }
    }
    if zoom.abs() > f32::EPSILON {
        // **缩放曲线**：越远越快、越近越慢。
        // t = 到"最远处"的相对位置（1 = 最远，0 = 贴着下限）；用开方让曲线在近处更平缓。
        let t = ((cam.target_dist - min) / (CAM_MAX - min).max(1e-6)).clamp(0.0, 1.0);
        // **缩放曲线**：贴着下限时的速度系数（1.0 = 不做曲线）。
        // 越远越快、越近越慢 —— 远处快速掠过，近处一点点挪，便于定位细节。
        let factor = 0.12;
        let curve = factor + (1.0 - factor) * t.sqrt();
        // `zoom > 0` 拉近，`zoom < 0` 拉远， 对数缩放系数：`dist *= exp(-scroll * k)`。
        cam.target_dist = (cam.target_dist * (-(zoom * curve) * 0.15).exp()).clamp(min, CAM_MAX);
    }
    cam.target_dist = cam.target_dist.clamp(min, CAM_MAX);
    
    Ok(())
}

/// 目标值 → 当前值（帧率无关平滑）→ 写 Transform。
pub fn smooth_camera(
    time: Res<Time>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut q: Query<(&mut CameraOrbit, &mut Transform)>,
    mut state: ResMut<CameraState>,
    mut proj: Query<&mut Projection>,
) -> Result {
    let (mut cam, mut tf) = q.single_mut()?;
    let dt = time.delta_secs().clamp(1e-4, 0.1);

    // 帧率无关平滑系数：`alpha = 1 - exp(-k·dt)`。
    let a_orbit = 1.0 - (-12.0 * dt).exp();
    let a_zoom = 1.0 - (-8.0 * dt).exp();
    let a_pan = 1.0 - (-8.0 * dt).exp();

    cam.yaw += (cam.target_yaw - cam.yaw) * a_orbit;
    cam.pitch += (cam.target_pitch - cam.pitch) * a_orbit;
    cam.dist += (cam.target_dist - cam.dist) * a_zoom;
    // Vec3 字段用局部变量过渡：`cam.focus += (…cam.focus…)` 会同时要求可变与不可变借用
    let focus = cam.focus + (cam.target_focus - cam.focus) * a_pan;
    cam.focus = focus;

    let pos = cam.focus + cam.dir() * cam.dist;
    tf.translation = pos;
    tf.look_at(cam.focus, Dir3::Y);

    state.pos = pos;
    state.dist = cam.dist;
    state.yaw = cam.yaw;
    state.pitch = cam.pitch;
    state.altkm = (cam.dist - EARTHR) * 6371.0;

    // **动态近平面**：`near` 始终贴着当前视高 ⇒ 各高度都有毫米级深度精度，
    // 贴面图层（几十米抬升）才不会被深度测试吃掉。
    let alt = (cam.dist - EARTHR).max(1e-4);
    let near = (alt * 0.5).clamp(1e-4, 2.0);
    let far = cam.dist + 2.5 * EARTHR;
    if let Ok(mut proj) = proj.single_mut() {
        if let Projection::Perspective(p) = &mut *proj {
            if (p.near - near).abs() > near * 0.01 || (p.far - far).abs() > far * 0.01 {
                p.near = near;
                p.far = far;
            }
        }
    }

    if let Ok(w) = windows.single() {
        state.screenh = w.resolution.height().max(1.0);
    }

    Ok(())
}
