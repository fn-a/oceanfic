/// UI界面

use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct UviewState {
    pub show_asset_panel: bool,
    pub show_risk_panel: bool,
    pub show_ocean_panel: bool,
    pub selected_asset: Option<String>,
}

#[derive(Component)]
pub struct TopBar;

#[derive(Component)]
pub struct OceanPanel;

#[derive(Component)]
pub struct AssetPanel;

#[derive(Component)]
pub struct RiskPanel;

#[derive(Component)]
pub struct PanelLabel;

const PANEL_BG: Color = Color::srgba(0.05, 0.08, 0.15, 0.85);
const PANEL_BORDER: Color = Color::srgba(0.2, 0.5, 0.8, 0.6);
const TEXT_COLOR: Color = Color::srgb(0.85, 0.9, 0.95);
const ACCENT_COLOR: Color = Color::srgb(0.15, 0.55, 0.9);
const TOP_BAR_HEIGHT: Val = Val::Px(40.0);
const SIDE_PANEL_WIDTH: Val = Val::Px(220.0);

pub fn spawn_uview(mut commands: Commands) {
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: TOP_BAR_HEIGHT,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            padding: UiRect::px(10.0, 10.0, 0.0, 0.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.05, 0.1, 0.9)),
        TopBar,
        children![
            (
                Text::new("OceanFic"),
                TextFont {
                    font_size: 20.0,
                    ..default()
                },
                TextColor(ACCENT_COLOR),
                Node {
                    margin: UiRect::right(Val::Px(20.0)),
                    ..default()
                },
            ),
            (
                Button,
                Node {
                    padding: UiRect::px(12.0, 12.0, 4.0, 4.0),
                    margin: UiRect::right(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.1, 0.3, 0.6, 0.5)),
                children![(
                    Text::new("Ocean"),
                    TextFont {
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(TEXT_COLOR),
                )],
            ),
            (
                Button,
                Node {
                    padding: UiRect::px(12.0, 12.0, 4.0, 4.0),
                    margin: UiRect::right(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.1, 0.3, 0.6, 0.5)),
                children![(
                    Text::new("Assets"),
                    TextFont {
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(TEXT_COLOR),
                )],
            ),
            (
                Button,
                Node {
                    padding: UiRect::px(12.0, 12.0, 4.0, 4.0),
                    margin: UiRect::right(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.1, 0.3, 0.6, 0.5)),
                children![(
                    Text::new("Risk"),
                    TextFont {
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(TEXT_COLOR),
                )],
            ),
        ],
    ));

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: TOP_BAR_HEIGHT,
            width: SIDE_PANEL_WIDTH,
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(10.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(PANEL_BG),
        BorderColor::all(PANEL_BORDER),
        OceanPanel,
        children![
            panel_title("Ocean Environment"),
            panel_item("Meteorology"),
            panel_item("Ocean Currents"),
            panel_item("Ecosystems"),
            panel_item("Migration Routes"),
        ],
    ));

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(0.0),
            top: TOP_BAR_HEIGHT,
            width: SIDE_PANEL_WIDTH,
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(10.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(PANEL_BG),
        BorderColor::all(PANEL_BORDER),
        AssetPanel,
        children![
            panel_title("Asset Management"),
            panel_item("Submarine Cables"),
            panel_item("Offshore Wind Farms"),
            panel_item("Oil & Gas Platforms"),
            panel_item("Vessels & Routes"),
            panel_item("Ecological Zones"),
        ],
    ));

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(230.0),
            top: Val::Px(50.0),
            width: Val::Px(200.0),
            height: Val::Auto,
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(10.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(PANEL_BG),
        BorderColor::all(Color::srgba(0.8, 0.3, 0.1, 0.6)),
        RiskPanel,
        children![
            panel_title("Risk Control"),
            panel_item("Active Alerts: 0"),
            panel_item("Risk Level: Normal"),
        ],
    ));
}

fn panel_title(text: &str) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        TextColor(ACCENT_COLOR),
        Node {
            margin: UiRect::bottom(Val::Px(8.0)),
            ..default()
        },
    )
}

fn panel_item(text: &str) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: 13.0,
            ..default()
        },
        TextColor(TEXT_COLOR),
        Node {
            margin: UiRect::px(4.0, 4.0, 2.0, 2.0),
            ..default()
        },
    )
}

pub fn toggle_panels(
    state: Res<UviewState>,
    mut ocean_query: Query<&mut Visibility, With<OceanPanel>>,
    mut asset_query: Query<&mut Visibility, (With<AssetPanel>, Without<OceanPanel>)>,
    mut risk_query: Query<
        &mut Visibility,
        (With<RiskPanel>, Without<OceanPanel>, Without<AssetPanel>),
    >,
) {
    for mut vis in &mut ocean_query {
        *vis = if state.show_ocean_panel {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    for mut vis in &mut asset_query {
        *vis = if state.show_asset_panel {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    for mut vis in &mut risk_query {
        *vis = if state.show_risk_panel {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}
