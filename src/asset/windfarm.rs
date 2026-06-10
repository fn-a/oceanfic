/// 海上风电

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct WindFarm {
    pub id: String,
    pub name: String,
    pub position: Vec3,
    pub turbine: u32, // turbine count
    pub capacity: f64, // total capacity mw
    pub status: FarmStatus,
    pub cables: Vec<String>, // transmission cables
}

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct OffshoreSubstation {
    pub id: String,
    pub name: String,
    pub position: Vec3,
    pub capacity: f64, // capacity mw 
    pub connects: Vec<String>, // connected farms
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FarmStatus {
    Operational,
    Partial,
    Maintenance,
    Planned,
    Decommissioned,
}

#[derive(Resource, Default)]
pub struct WindFarmRegistry {
    pub farms: Vec<WindFarm>,
    pub substations: Vec<OffshoreSubstation>,
}
