/// 风险评估

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub asset: String, // asset id
    pub level: RiskLevel, // risk level
    pub factors: Vec<RiskFactor>,
    pub score: f32,
    pub updated: String, // last updated
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RiskLevel {
    Minimal,
    Low,
    Moderate,
    High,
    Critical,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RiskFactor {
    pub name: String,
    pub category: RiskCategory,
    pub weight: f32,
    pub value: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RiskCategory {
    Weather,
    Current,
    Structural,
    Operational,
    Environmental,
    Geopolitical,
}

#[derive(Resource, Default)]
pub struct RiskState {
    pub assessments: Vec<RiskAssessment>,
}

pub fn evaluate_risks(mut state: ResMut<RiskState>) {
    for assessment in &mut state.assessments {
        assessment.score = assessment.factors.iter().map(|f| f.weight * f.value).sum();
        assessment.level = match assessment.score {
            s if s < 0.2 => RiskLevel::Minimal,
            s if s < 0.4 => RiskLevel::Low,
            s if s < 0.6 => RiskLevel::Moderate,
            s if s < 0.8 => RiskLevel::High,
            _ => RiskLevel::Critical,
        };
    }
}
