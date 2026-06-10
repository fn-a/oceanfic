/// 油气平台

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct OilGasPlatform {
    pub id: String,
    pub name: String,
    pub position: Vec3,
    pub platform: PlatformType, // platform type
    pub rate: f64, // production rate
    pub status: PlatformStatus,
    pub connections: Vec<String>, // pipeline connections
}

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct Pipeline {
    pub id: String,
    pub name: String,
    pub start: Vec3,
    pub end: Vec3,
    pub diameter: f32, // diameter mm
    pub status: PipelineStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PlatformType {
    FixedJacket,
    SemiSubmersible,
    TensionLeg,
    Spar,
    Fpso,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PlatformStatus {
    Producing,
    ShutIn,
    Maintenance,
    Abandoned,
    Planned,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PipelineStatus {
    Flowing,
    ShutDown,
    Maintenance,
    Leaking,
}

#[derive(Resource, Default)]
pub struct OilGasRegistry {
    pub platforms: Vec<OilGasPlatform>,
    pub pipelines: Vec<Pipeline>,
}
