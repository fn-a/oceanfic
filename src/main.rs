// 发布构建不要附带控制台窗口；debug 保留控制台以便看日志
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process;
use bevy::prelude::*;
use bevy::log::LogPlugin;
use bevy::window::WindowPlugin;
use oceanfic::config::{Manifest, Colors};
use oceanfic::globe;
use oceanfic::coast;
use oceanfic::grid;
use oceanfic::metrics;
use oceanfic::camera;
use oceanfic::geo::work::Worker;
use oceanfic::geo::store::Levels;
use oceanfic::geo::{GeoPoint, GeoShape};
use oceanfic::glyphs::{self, GlyphLayer, GlyphMatl, LayerStyle};

fn main() {
    let (manifest, warn) = Manifest::loadir();

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(LogPlugin {
                // debug 构建保留诊断日志（视野重选、补丁重建…），发布构建只留 info。
                filter: if cfg!(debug_assertions) {
                    "oceanfic=debug,bevy=info,wgpu=warn".to_string()
                } else {
                    "oceanfic=info,bevy=warn,wgpu=error".to_string()
                },
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "OceanFic - Maritime Asset Risk Control Platform".into(),
                    ..default()
                }),
                ..default()
            }),
    );

    // `LogPlugin` 装好之后 tracing 才可用，所以日志一律放在这之后。
    if let Some(w) = warn {
        warn!("{w}");
    }

    let levels = match Levels::load() {
        Ok(l) => {
            info!(
                "basemap: lo {} rings/{} pts, hi {} rings/{} pts",
                l.lo.rcount(),
                l.lo.pcount(),
                l.hi.rcount(),
                l.hi.pcount()
            );
            l
        }
        Err(e) => {
            error!("cannot load basemap: {e} (check assets/)");
            process::exit(1);
        }
    };

    app.insert_resource(levels)
        .insert_resource(Worker::spawn())
        .insert_resource(manifest.lighting)
        .insert_resource(manifest)
        .add_systems(
            Startup,
            (
                camera::setup_camera,
                globe::setup_globe,
                coast::setup_coastv,
                coast::setup_coastl,
                coast::setup_ldfill,
                globe::setup_light,
                grid::setup_grid,
                metrics::setup_metrics,
                setup_samples,
            ),
        )
        .add_systems(
            Update,
            (
                globe::toggle_light,
                coast::update_coastv,
                coast::update_ldfill,
                globe::apply_light,
                camera::update_camera,
                camera::smooth_camera,
                grid::update_grid,
                glyphs::build_glyphs,
                metrics::update_msperf,
                metrics::update_mstext,
            ).chain(),
        )
        .run();
}

/// 演示用的验证夹具（不是数据）：**一个图层就是一个实体**，高亮层由它建出来，
/// 将来被资产层（点/航线/风险区）取代。
fn setup_samples(
    mut commands: Commands,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    // 高亮层颜色。
    let rgb = Colors::parsex("#ffd166").unwrap_or([1.0, 0.82, 0.4]);
    let matl = GlyphMatl::single(rgb, true).build(&mut mats);
    let style = LayerStyle {
        // 高亮层抬升（世界单位，米级）：高于海岸线，低于网格。
        offset: 1.0 + 8.0 / 6_371_000.0,
        // 线宽 / 点尺寸按**屏幕像素**给定。
        width_px: 2.6,
        point_px: 5.0,
    };
    let mut layer = GlyphLayer::new(style, matl);
    layer.shapes = vec![
        // 横滨 → 檀香山：跨 180° 经线
        GeoShape::Line(vec![
            GeoPoint::new(35.44, 139.64),
            GeoPoint::new(37.50, 165.00),
            GeoPoint::new(33.00, -175.00),
            GeoPoint::new(27.00, -165.00),
            GeoPoint::new(21.31, -157.86),
        ]),
        // 北海油气平台
        GeoShape::Point(GeoPoint::new(58.10, 1.60)),
        // 东海海上风电场
        GeoShape::Point(GeoPoint::new(30.50, 122.00)),
        // 光缆登陆点（香港）
        GeoShape::Point(GeoPoint::new(22.30, 114.17)),
    ];
    commands.spawn(layer);
}
