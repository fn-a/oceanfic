/// 洋流系统

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct OceanCurrent {
    pub id: String,
    pub name: String,
    pub current_type: CurrentType,
    pub speed: f32,
    pub temperature: f32,
    pub direction: Vec3,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CurrentType {
    GulfStream,
    Kuroshio,
    AntarcticCircumpolar,
    Equatorial,
    Upwelling,
    Downwelling,
}

#[derive(Resource, Default)]
pub struct CurrentState {
    pub currents: Vec<OceanCurrent>,
}

pub fn update_currents(_time: Res<Time>, _state: ResMut<CurrentState>) {}
