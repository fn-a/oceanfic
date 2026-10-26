//! 地球本体：球体 + 海洋/陆地着色 + 光照，以及内嵌底图与陆地遮罩的解码。

use std::time::Instant;
use bevy::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use crate::config::{Manifest, Colors, Lighting};
use crate::geo::GeoPoint;
use crate::geo::fetch::Quest;
use crate::geo::store::Levels;
use crate::geo::mask::{MASK_WIDTH, MASK_HEIGHT, rasterize};

/// 地球半径（世界单位）。f32 有效精度约 7 位 ⇒ 最小可分辨 ≈ 地表 0.6 m。
/// 不用米：坐标过大会在贴地时抖动，还要引入 camera-relative rendering 补丁。
pub const EARTHR: f32 = 1.0;

/// 环境光亮度（挂在相机上，覆盖全局 `GlobalAmbientLight`）。
/// 夜半球需要足够底光，否则另一半球面的矢量数据完全看不见。
pub const AMBIENT_REALISTIC: f32 = 900.0;
/// 均匀光照模式下的环境光：整球一样亮，没有昼夜区分。
pub const AMBIENT_UNIFORM: f32 = 2200.0;
/// 平行光强度：真实模式有昼夜，均匀模式关掉它。
pub const DISTANT_REALISTIC: f32 = 12_000.0;
pub const DISTANT_UNIFORM: f32 = 0.0;

#[derive(Component)]
pub struct Globe;

#[derive(Component)]
pub struct Light;

/// 全局陆地遮罩的已应用代次（底图热加载后重建纹理用）。
#[derive(Resource, Default)]
pub struct Shore {
    epoch: u64,
}

pub fn setup_globe(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    manifest: Res<Manifest>,
) {
    let ocean = Colors::parsex(&manifest.colors.ocean).unwrap_or([0.05, 0.15, 0.4]);
    let mesh = {
        // 由经纬度生成 UV 球；UV 与遮罩的等距圆柱栅格**严格对齐**（u = lon, v = lat）。
        // 球体网格密度（经纬分段）。贴近时球体轮廓的圆滑度由它决定。
        let sectors = 1024;
        let stacks = 512;
        let row = sectors + 1;
        let mut positions: Vec<[f32; 3]> = Vec::with_capacity(((stacks + 1) * row) as usize);
        let mut normals: Vec<[f32; 3]> = Vec::with_capacity(((stacks + 1) * row) as usize);
        let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(((stacks + 1) * row) as usize);
        let mut indices: Vec<u32> = Vec::with_capacity((stacks * sectors * 6) as usize);

        for j in 0..=stacks {
            let v = j as f64 / stacks as f64;
            let lat = 90.0 - v * 180.0;
            for i in 0..=sectors {
                let u = i as f64 / sectors as f64;
                let lon = -180.0 + u * 360.0;
                let p = GeoPoint::new(lat, lon).world(EARTHR);
                positions.push(p.to_array());
                normals.push(p.normalize_or_zero().to_array());
                uvs.push([u as f32, v as f32]);
            }
        }
        for j in 0..stacks {
            for i in 0..sectors {
                let a = j * row + i;
                let b = a + 1;
                let c = a + row;
                let d = c + 1;
                indices.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_indices(Indices::U32(indices));
        mesh
    };
    // 先给出纯海洋球体；底图可用后贴上全局陆地遮罩。
    commands.spawn((
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(ocean[0], ocean[1], ocean[2]),
            base_color_texture: None,
            metallic: 0.0,
            perceptual_roughness: 0.9,
            ..default()
        })),
        Transform::default(),
        Globe,
    ));
    commands.insert_resource(Shore::default());
}

/// 底图首次可用或热加载后，重建球体上的全局陆地遮罩纹理。
pub fn update_globe(
    mut levels: ResMut<Levels>,
    mut state: ResMut<Shore>,
    manifest: Res<Manifest>,
    fquest: Option<ResMut<Quest>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    globe: Query<&MeshMaterial3d<StandardMaterial>, With<Globe>>,
) {
    if let Some(mut quest) = fquest {
        if let Some(newlvs) = quest.pumpup(&levels) {
            *levels = newlvs;
        }
    }

    if state.epoch != levels.epoch {
        let ocean = Colors::parsex(&manifest.colors.ocean).unwrap_or([0.05, 0.15, 0.4]);
        let land = Colors::parsex(&manifest.colors.land_fill).unwrap_or([0.25, 0.42, 0.27]);
    
        let texture = levels.lo.as_ref().map(|lo| {
            // 从**全球档**（GSHHG intermediate）现栅格化一张全局遮罩。
            let rings: Vec<Vec<[f64; 2]>> = lo
                .lines
                .iter()
                .map(|r| r.iter().map(|p| [p.lon, p.lat]).collect())
                .collect();
    
            let t0 = Instant::now();
            let mask = rasterize(&rings, MASK_WIDTH, MASK_HEIGHT);
            let texels = mask.width as usize * mask.height as usize;
            let pctval = if texels > 0 {
                mask.count() as f32 / texels as f32 * 100.0
            } else {
                0.0
            };
            info!(
                "land mask: {:.1}% set {}x{} ({} ms, from GSHHG lo)",
                pctval,
                mask.width,
                mask.height,
                t0.elapsed().as_millis()
            );
            let rgba = mask.srgba(land, ocean);
            images.add(Image::new(
                Extent3d {
                    width: mask.width,
                    height: mask.height,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                rgba,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            ))
        });
    
        for material in &globe {
            if let Some(mut mat) = materials.get_mut(&material.0) {
                match &texture {
                    Some(tex) => {
                        mat.base_color = Color::WHITE;
                        mat.base_color_texture = Some(tex.clone());
                    }
                    None => {
                        mat.base_color = Color::srgb(ocean[0], ocean[1], ocean[2]);
                        mat.base_color_texture = None;
                    }
                }
            }
        }
        state.epoch = levels.epoch;
    }
}

/// 平行光。环境光必须挂在相机上。
pub fn setup_light(mut commands: Commands) {
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.5, 0.5, 0.0)),
        Light,
    ));
}

/// 按当前模式套用光照强度。两种模式的差别就是"有没有昼夜"。
pub fn apply_light(
    lighting: Res<Lighting>,
    mut distant: Query<&mut DirectionalLight, With<Light>>,
    mut ambient: Query<&mut AmbientLight>,
) {
    let (sun_illum, amb_bright) = match *lighting {
        Lighting::Realistic => (DISTANT_REALISTIC, AMBIENT_REALISTIC),
        Lighting::Uniform => (DISTANT_UNIFORM, AMBIENT_UNIFORM),
    };
    for mut l in &mut distant {
        if (l.illuminance - sun_illum).abs() > f32::EPSILON {
            l.illuminance = sun_illum;
        }
    }
    for mut a in &mut ambient {
        if (a.brightness - amb_bright).abs() > f32::EPSILON {
            a.brightness = amb_bright;
        }
    }
}

/// `L` 键在"有昼夜 / 无昼夜"之间切换。
pub fn toggle_light(
    keys: Res<ButtonInput<KeyCode>>,
    mut lighting: ResMut<Lighting>
) {
    if keys.just_pressed(KeyCode::KeyL) {
        *lighting = lighting.toggled();
        info!("lighting mode: {}", *lighting);
    }
}
