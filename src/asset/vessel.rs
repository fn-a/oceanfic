/// 船舶管理

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct Vessel {
    pub id: String,
    pub name: String,
    pub vessel: VesselType, // vessel type
    pub position: Vec3,
    pub speed: f32, // speed knots
    pub heading: f32,
    pub status: VesselStatus,
    pub cargo: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum VesselType {
    OilTanker,
    LngCarrier,
    CargoShip,
    FishingVessel,
    AquacultureVessel,
    ResearchVessel,
    Tugboat,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum VesselStatus {
    Underway,
    Anchored,
    Docked,
    Loading,
    Maintenance,
    Distress,
}

#[derive(Resource, Default)]
pub struct VesselRegistry {
    pub vessels: Vec<Vessel>,
}

pub fn update_vessel(time: Res<Time>, mut registry: ResMut<VesselRegistry>) {
    for vessel in &mut registry.vessels {
        if matches!(vessel.status, VesselStatus::Underway) {
            let heading_rad = vessel.heading.to_radians();
            let dx = vessel.speed * heading_rad.cos() * time.delta_secs() * 0.001;
            let dz = vessel.speed * heading_rad.sin() * time.delta_secs() * 0.001;
            vessel.position.x += dx;
            vessel.position.z += dz;
        }
    }
}
