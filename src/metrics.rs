//! 运行指标读数：不是"调试用的临时代码"，而是**长期展示的运行指标**。
//!
//! 存在的意义：验收标准里"无可见折角""无 z-fighting""无跳变"靠肉眼判断无法防回归，
//! 有了数字就变成可检验的——细分数是否随距离变密、重建次数是否失控、顶点数是否在预算内。

use bevy::prelude::*;
use crate::camera::CameraState;
use crate::config::Lighting;
use crate::coast::Coastv;
use crate::globe::EARTHR;
use crate::grid::GridState;
use crate::glyphs::GlyphLayer;

#[derive(Component)]
pub struct MetricsText;

/// 帧率与帧时（指数平滑，不依赖 diagnostics 插件）。
#[derive(Resource, Default)]
pub struct MetricsPerf {
    pub fps: f32,
    pub framems: f32,
    /// 距上次刷新读数的累计时间。
    pub txtaccum: f32,
}

pub fn setup_metrics(mut commands: Commands) {
    commands.insert_resource(MetricsPerf::default());

    commands.spawn((
        Text::new(""),
        TextFont { font_size: FontSize::Px(13.0), ..default() },
        TextColor(Color::srgb(0.78, 0.88, 0.95)),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(10.0),
            bottom: Val::Px(10.0),
            ..default()
        },
        MetricsText,
    ));
}

pub fn update_msperf(time: Res<Time>, mut perf: ResMut<MetricsPerf>) {
    let dt = time.delta_secs();
    perf.framems = dt * 1000.0;
    // 启动首帧可能异常（着色器编译会 stall 数百毫秒，也可能出现近零 delta）。
    // 这类值一旦进入指数平滑，需要几百帧才能冲掉——直接丢弃。
    if (0.001..=1.0).contains(&dt) {
        let inst = 1.0 / dt;
        perf.fps = if perf.fps <= 0.0 { inst } else { perf.fps * 0.9 + inst * 0.1 };
    }
}

pub fn update_mstext(
    time: Res<Time>,
    mut perf: ResMut<MetricsPerf>,
    cam: Res<CameraState>,
    grid: Res<GridState>,
    lighting: Res<Lighting>,
    view: Res<Coastv>,
    layers: Query<&GlyphLayer>,
    mut q: Query<&mut Text, With<MetricsText>>,
) {
    perf.txtaccum += time.delta_secs();
    // 读数刷新间隔。
    if perf.txtaccum >= 0.25 {
        perf.txtaccum = 0.0;

        // 每个图层只重建自己，所以读数按层汇总。
        let mut verts = 0usize;
        let mut tris = 0usize;
        let mut runs = 0usize;
        let mut tess = 0usize;
        let mut segs = 0u32;
        let mut rebuilds = 0u32;
        let mut buildms = 0.0f32;
        let mut nlayers = 0usize;
        for l in &layers {
            let s = &l.stats;
            verts += s.verts;
            tris += s.tris;
            runs += s.runs;
            tess += s.tess_pts;
            segs = segs.max(s.segs_max);
            rebuilds += s.rebuilds;
            buildms += s.last_tms;
            nlayers += 1;
        }
    
        if let Ok(mut text) = q.single_mut() {
            text.0 = {
                let d = cam.dist;
                let h = (d - EARTHR).max(0.0);
                let zoom = if h > 1e-4 { format!("{:.0} km", cam.altkm) } else { "0 km".to_string() };
                // 读数里显示此刻**实际在用**的档位（由 view 选择）
                let ds = view.level;

                format!(
                    "cam d={d:.3} ({zoom})  pitch {:.1}deg  yaw {:.1}deg\n\
                    tess step={:.0}deg  segs<={}  tess={}  verts={}  tris={}  runs={}\n\
                    layers={}  rebuilds={} ({:.0}ms)  fps {:.1}  frame {:.1} ms  data={}  light={} [L toggles, PgUp/PgDn zoom]",
                    cam.pitch.to_degrees(),
                    cam.yaw.to_degrees(),
                    grid.stpdeg,
                    segs,
                    tess,
                    verts,
                    tris,
                    runs,
                    nlayers,
                    rebuilds,
                    buildms,
                    perf.fps,
                    perf.framems,
                    ds,
                    *lighting,
                )
            };
        }
    }
}