use bevy::prelude::*;

mod asset;
mod config;
mod core;
mod ocean;
mod risk;

use core::{camera, globe, uview};
use ocean::{currents, ecosystem, meteorology};
use asset::{cable, oilgas, route, windfarm, vessel};
use risk::{alert, assessment};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "OceanFic - Maritime Asset Risk Control Platform".into(),
                ..default()
            }),
            ..default()
        }))
        .init_resource::<config::AppConfig>()
        .init_resource::<globe::GlobeState>()
        .init_resource::<uview::UviewState>()
        .init_resource::<meteorology::MeteorologyState>()
        .init_resource::<currents::CurrentState>()
        .init_resource::<ecosystem::EcosystemState>()
        .init_resource::<cable::CableRegistry>()
        .init_resource::<windfarm::WindFarmRegistry>()
        .init_resource::<oilgas::OilGasRegistry>()
        .init_resource::<vessel::VesselRegistry>()
        .init_resource::<route::RouteRegistry>()
        .init_resource::<assessment::RiskState>()
        .init_resource::<alert::AlertState>()
        .add_systems(
            Startup,
            (
                camera::spawn_camera,
                globe::spawn_globe,
                uview::spawn_uview,
                spawn_lighting,
            ),
        )
        .add_systems(
            Update,
            (
                globe::rotate_globe,
                camera::update_camera,
                uview::toggle_panels,
                meteorology::update_weather,
                currents::update_currents,
                ecosystem::update_ecosystem,
                vessel::update_vessel,
                assessment::evaluate_risks,
                alert::process_alerts,
            ),
        )
        .run();
}

fn spawn_lighting(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.5, 0.5, 0.0)),
    ));
    commands.spawn((AmbientLight {
        color: Color::WHITE,
        brightness: 200.0,
        affects_lightmapped_meshes: false,
    },));
}
