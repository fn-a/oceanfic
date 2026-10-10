use bevy::prelude::*;
use crate::config::{Manifest, Colors};
use crate::glyphs::{GlyphLayer, GlyphMatl, LayerStyle};

/// 海岸线描边抬升：高于陆地填充，低于高亮与网格。
pub const SLINE_OFT: f32 = 1.0 + 6.0 / 6_371_000.0;

/// 海岸线描边图层实体（全场景只有这一层）。
#[derive(Component)]
pub struct Coastl;

/// 启动时：建海岸线描边层（样式 + 材质；形状等 `view` 第一次选出环）。
pub fn setup_coastl(
    mut commands: Commands,
    mut mats: ResMut<Assets<StandardMaterial>>,
    manifest: Res<Manifest>,
) {
    let rgb = Colors::parsex(&manifest.colors.land_line).unwrap_or([0.5, 0.75, 0.4]);
    let matl = GlyphMatl::single(rgb, false).build(&mut mats);
    // 线宽按**屏幕像素**给定，渲染时按相机距离换算成世界宽度。
    let wdpx: f32 = 1.7;
    let style = LayerStyle { offset: SLINE_OFT, width_px: wdpx, ..default() };
    commands.spawn((GlyphLayer::new(style, matl), Coastl));
}
