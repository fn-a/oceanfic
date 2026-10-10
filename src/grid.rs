//! 经纬网格：决定"选哪些经纬线"，以及网格自己的样式与材质。

use bevy::prelude::*;
use crate::camera::CameraState;
use crate::config::{Manifest, Colors};
use crate::geo::{GeoPoint, GeoShape};
use crate::glyphs::{GlyphLayer, GlyphMatl, LayerStyle};

/// 网格图层实体（全场景只有这一层网格）。
#[derive(Component)]
pub struct GridLines;

/// 网格状态（给 [`crate::metrics`] 读数用）。
#[derive(Resource, Default)]
pub struct GridState {
    /// 当前网格步长（度）。
    pub stpdeg: f64,
}

/// 生成经纬线（程序化 [`GeoShape`]）。
pub fn graticule(sdeg: f64) -> Vec<GeoShape> {
    // 经纬网格的纬度上限。取到接近极点，**极区也要有网格**；
    // 经线在极点自然收敛（等距圆柱下 15° 间隔在 89.5° 处仍有约 45 px，不会糊成一团）。
    let clamp = 89.5;
    let step = sdeg.clamp(0.25, 90.0);

    // 经线等分 360°，纬线在 ±clamp 内等分，保证首尾对称且不重复
    let cols = (360.0 / step).round().max(1.0) as i32;
    let dlon = 360.0 / cols as f64;
    let rows = ((2.0 * clamp) / step).round().max(1.0) as i32;
    let dlat = (2.0 * clamp) / rows as f64;
    // 采样间隔：大网格也不必用整档步长铺点，封顶 5°
    let sample = step.min(5.0);

    let mut out = Vec::with_capacity((cols + rows + 1) as usize);

    let lat_seg = ((2.0 * clamp) / sample).ceil().max(1.0) as i32;
    for i in 0..cols {
        let lon = -180.0 + i as f64 * dlon;
        let mut pts = Vec::with_capacity(lat_seg as usize + 1);
        for k in 0..=lat_seg {
            let lat = -clamp + k as f64 * (2.0 * clamp / lat_seg as f64);
            pts.push(GeoPoint::new(lat, lon));
        }
        out.push(GeoShape::Line(pts));
    }

    let lon_seg = (360.0 / sample).ceil().max(4.0) as i32;
    for j in 0..=rows {
        let lat = -clamp + j as f64 * dlat;
        let mut pts = Vec::with_capacity(lon_seg as usize + 1);
        for k in 0..=lon_seg {
            let lon = -180.0 + k as f64 * (360.0 / lon_seg as f64);
            pts.push(GeoPoint::new(lat, lon));
        }
        out.push(GeoShape::Line(pts));
    }

    out
}


/// 取网格步长：第一个 `cam_dist < 上限` 的档位，都不满足取最后一档。
pub fn gridstdeg(cam_dist: f32) -> f64 {
    // 经纬网分档：`(相机距离上限, 步长°)`，升序。与相机距离语义、细分弧长上限、
    // 数据档位切换点耦合 ⇒ 同样是实现常量。
    let steps: &[(f32, f64)] = &[(1.2, 1.0), (2.0, 5.0), (4.0, 15.0), (8.0, 30.0)];
    steps
        .iter()
        .find(|(d, _)| cam_dist < *d)
        .or(steps.last())
        .map(|(_, s)| *s)
        .unwrap_or(30.0)
}

/// 启动时：建网格图层（样式 + 材质）。
pub fn setup_grid(
    mut commands: Commands,
    mut mats: ResMut<Assets<StandardMaterial>>,
    manifest: Res<Manifest>,
) {
    commands.insert_resource(GridState::default());

    // 网格是**参考层**：用 unlit 让它不受昼夜影响。
    // 否则夜半球的那一半网格在暗光下几乎看不见（实测就被误认为"网格只出了一半"）。
    let rgb = Colors::parsex(&manifest.colors.grid).unwrap_or([0.66, 0.78, 0.89]);
    // 双面可见：经线/纬线走向与岸线不同，可能有半边被判成背面。
    let matl = GlyphMatl::double(rgb, true).build(&mut mats);
    // 线宽按**屏幕像素**给定，渲染时按相机距离换算成世界宽度。
    // 0.9 px 在无 AA 的条带渲染下容易碎成点/断续（观感为"看不见"）。
    let wdpx = 1.4;
    // 网格抬升（世界单位，米级）：压在所有图层之上。
    let goft = 1.0 + 50.0 / 6_371_000.0;
    let style = LayerStyle { offset: goft, width_px: wdpx, ..default() };
    commands.spawn((GlyphLayer::new(style, matl), GridLines));
}

/// 相机距离跨档时重建网格。步长不变则什么都不做。
pub fn update_grid(
    mut state: ResMut<GridState>,
    cam: Res<CameraState>,
    mut q: Query<&mut GlyphLayer, With<GridLines>>,
) {
    let step = gridstdeg(cam.dist);
    if state.stpdeg <= 0.0 || (step - state.stpdeg).abs() >= f64::EPSILON {
        state.stpdeg = step;
        for mut l in &mut q {
            l.shapes = graticule(step);
            l.touched();
        }
    }
}
