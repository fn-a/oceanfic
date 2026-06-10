/// 海洋生态与物种

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct EcologicalZone {
    pub id: String,
    pub name: String,
    pub zone: EcoZoneType, // zone type
    pub center: Vec3,
    pub radius: f32,
    pub biodiversity: f32, // biodiversity index
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EcoZoneType {
    CoralReef,
    KelpForest,
    Mangrove,
    DeepSeaVent,
    ContinentalShelf,
    OpenOcean,
}

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct SpeciesGroup {
    pub id: String,
    pub name: String,
    pub species: SpeciesType, // species type
    pub population: u64,
    pub migrateroute: MigrationRoute, // migration route
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SpeciesType {
    Whale,
    SeaTurtle,
    FishSchool,
    Plankton,
    Seabird,
    MarineMammal,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MigrationRoute {
    pub id: String,
    pub waypoints: Vec<Vec3>,
    pub seasonality: String, // seasonal pattern
}

#[derive(Resource, Default)]
pub struct EcosystemState {
    pub zones: Vec<EcologicalZone>,
    pub species: Vec<SpeciesGroup>,
}

pub fn update_ecosystem(_time: Res<Time>, _state: ResMut<EcosystemState>) {}
