/// 相机控制

use bevy::prelude::*;

#[derive(Component)]
pub struct OrbitCamera {
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            distance: 15.0,
            yaw: 0.0,
            pitch: 0.4,
        }
    }
}

pub fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 5.0, 15.0).looking_at(Vec3::ZERO, Dir3::Y),
        OrbitCamera::default(),
    ));
}

pub fn update_camera(mut query: Query<(&OrbitCamera, &mut Transform)>) {
    for (orbit, mut transform) in &mut query {
        let x = orbit.distance * orbit.pitch.cos() * orbit.yaw.cos();
        let y = orbit.distance * orbit.pitch.sin();
        let z = orbit.distance * orbit.pitch.cos() * orbit.yaw.sin();
        transform.translation = Vec3::new(x, y, z);
        transform.look_at(Vec3::ZERO, Dir3::Y);
    }
}
