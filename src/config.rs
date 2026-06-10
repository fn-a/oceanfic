/// 全局配置

use bevy::prelude::*;

#[derive(Resource, Clone, Debug)]
pub struct AppConfig {
    pub window_width: f32,
    pub window_height: f32,
    pub globe_radius: f32,
    pub ocean_color: Color,
    pub land_color: Color,
    pub grid_visible: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            window_width: 1920.0,
            window_height: 1080.0,
            globe_radius: 5.0,
            ocean_color: Color::srgb(0.05, 0.15, 0.4),
            land_color: Color::srgb(0.2, 0.5, 0.15),
            grid_visible: true,
        }
    }
}
