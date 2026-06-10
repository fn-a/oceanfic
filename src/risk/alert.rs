/// 预警系统

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct Alert {
    pub id: String,
    pub asset: String, // asset id
    pub alert: AlertType, // alert type
    pub severity: AlertSeverity,
    pub message: String,
    pub active: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AlertType {
    WeatherWarning,
    StructuralIntegrity,
    NavigationHazard,
    EnvironmentalViolation,
    EquipmentFailure,
    SecurityBreach,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Danger,
    Emergency,
}

#[derive(Resource, Default)]
pub struct AlertState {
    pub alerts: Vec<Alert>,
}

pub fn process_alerts(mut state: ResMut<AlertState>) {
    state.alerts.retain(|a| a.active);
}
