/// 气象系统

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct WeatherSystem {
    pub id: String,
    pub name: String,
    pub weather: WeatherType, // weather type
    pub intensity: f32,
    pub position: Vec3,
    pub radius: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WeatherType {
    Typhoon,
    Hurricane,
    Storm,
    Fog,
    HighWind,
    Normal,
}

#[derive(Resource, Default)]
pub struct MeteorologyState {
    pub actives: Vec<WeatherSystem>, // active weather
    pub interval: f32, // update interval secs
    pub elapsed: f32,
}

pub fn update_weather(time: Res<Time>, mut state: ResMut<MeteorologyState>) {
    state.elapsed += time.delta_secs();
    if state.elapsed >= state.interval {
        state.elapsed = 0.0;
    }
}
