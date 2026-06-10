/// 航道与路线

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct NavigationRoute {
    pub id: String,
    pub name: String,
    pub route: RouteType, // route type
    pub waypoints: Vec<RouteWaypoint>,
    pub traffic: f32, // traffic density
}

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct MigrationCorridor {
    pub id: String,
    pub species: String, // species id
    pub waypoints: Vec<Vec3>,
    pub season: String,
    pub active: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RouteWaypoint {
    pub position: Vec3,
    pub width: f32, // width km
    pub depth: f32, // depth m
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RouteType {
    ShippingLane,
    TerritorialWater,
    ExclusiveEconomicZone,
    FisheryZone,
    RestrictedArea,
}

#[derive(Resource, Default)]
pub struct RouteRegistry {
    pub routes: Vec<NavigationRoute>,
    pub corridors: Vec<MigrationCorridor>,
}
