/// 跨洋光缆

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct SubmarineCable {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub capacity: f64, // capacity gbps
    pub length: f64, // length km
    pub status: CableStatus,
    pub points: Vec<Vec3>, // landing points
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CableStatus {
    Operational,
    Degraded,
    Fault,
    Maintenance,
    Planned,
}

#[derive(Resource, Default)]
pub struct CableRegistry {
    pub cables: Vec<SubmarineCable>,
}
