/// 地球海洋3D渲染

use bevy::prelude::*;

#[derive(Component)]
pub struct Globe;

#[derive(Component)]
pub struct OceanSurface;

#[derive(Component)]
pub struct LandMass {
    pub name: String,
}

#[derive(Component)]
pub struct GridLine;

#[derive(Resource)]
pub struct GlobeState {
    pub rotation: f32, // rotation speed
    pub zoomlv: f32, // zoom level 
}

impl Default for GlobeState {
    fn default() -> Self {
        Self {
            rotation: 0.05,
            zoomlv: 1.0,
        }
    }
}

pub fn spawn_globe(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    config: Res<crate::config::AppConfig>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(config.globe_radius))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: config.ocean_color,
            metallic: 0.1,
            perceptual_roughness: 0.7,
            ..default()
        })),
        Globe,
        OceanSurface,
    ));
}

pub fn rotate_globe(
    time: Res<Time>,
    state: Res<GlobeState>,
    mut query: Query<&mut Transform, With<Globe>>,
) {
    for mut transform in &mut query {
        transform.rotate_y(state.rotation * time.delta_secs());
    }
}
