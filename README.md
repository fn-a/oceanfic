# OceanFic

**Maritime Asset Risk Control & Intelligent Management Platform**

[中文文档](README.zh.md)

## Overview

OceanFic is a high-performance 3D maritime asset risk control and intelligent management platform built with **Rust** and **Bevy Engine**. It provides real-time visualization and management of ocean-based assets, environmental monitoring, and risk assessment capabilities on an interactive 3D globe.

## Features

### 🌍 Globe & Ocean Visualization
- 3D interactive globe with ocean/land rendering
- Orbital camera with zoom, pan, and rotation
- Real-time grid overlay and coordinate system
- Layered rendering: filter by asset type, risk level, and timeline
- Thematic views: risk heat map and asset status colouring

### 🌦️ Ocean Environment & Meteorology
- Typhoon / hurricane / storm tracking and simulation
- Ocean current systems (Gulf Stream, Kuroshio, Antarctic Circumpolar, etc.)
- Sea-state elements: significant wave height, swell, storm surge
- Upwelling and downwelling zone monitoring
- Geohazard monitoring: earthquakes, tsunamis, submarine landslides
- Climate change trends: sea level, marine heatwaves
- Weather and sea-state impact analysis on maritime assets and operating-window determination

### 📡 Asset Management
- **Submarine Cables** — Cross-ocean cable routes, capacity, status monitoring
- **Submarine Power Cables** — Export cables, inter-array cables, interconnection cables
- **Offshore Wind & Solar** — Turbine clusters, offshore solar, offshore substations, transmission lines
- **Oil & Gas Platforms** — Fixed/semi-sub/FPSO platforms, pipeline networks
- **Vessels** — Oil tankers, LNG carriers, cargo ships, fishing/aquaculture vessels, real-time positioning
- **Navigation Routes** — Shipping lanes, EEZ boundaries, fishery zones, restricted areas
- **Shore-Based Facilities** — O&M home ports, receiving terminals, substations, supporting offshore maintenance and grid-connection decisions

### 🐋 Marine Ecology
- Ecological zone mapping (coral reefs, kelp forests, mangroves, deep-sea vents)
- Species group tracking (whales, sea turtles, fish schools, marine mammals)
- Migration route visualization and seasonal pattern analysis
- Protected-area and ecological red-line overlay

### ⚠️ Risk Control
- Multi-factor risk assessment engine (weather, current, structural, operational, environmental, geopolitical, geohazards, climate change)
- Real-time alert system with severity levels (Info → Emergency)
- Risk scoring and categorization (Minimal → Critical)
- Risk aggregation (Single factor → Asset → Area)
- Alert response (Acknowledge/Escalate → Response task → Outcome write-back)

## Getting Started

### Build & Run
```bash
# Clone the repository
git clone https://github.com/fn-a/oceanfic.git
cd oceanfic

# Run in development mode
cargo run

# Build for release
cargo build --release
```

### Controls

| Action | Input |
| --- | --- |
| Rotate / pan / zoom | Left drag / right drag / wheel |
| Raise / lower the camera | `Page Up` / `Page Down` |
| Day-night lighting ↔ uniform lighting | `L` |

### Base map data

Two pre-generated [GSHHG](https://www.soest.hawaii.edu/pwessel/gshhg/) levels ship in
`assets/` and are read once at startup (from `assets/` next to the executable, falling
back to `assets/` in the working directory):

| File | Source | Rings / points | Size | `altmax` |
| --- | --- | --- | --- | --- |
| `coasthi.bin` | GSHHG 2.3.7 shapefiles, **full** resolution | 188,275 / 10,587,877 | 81.5 MB | **0.16** (≈1,020 km) |
| `coastlo.bin` | same, **intermediate** resolution | 40,970 / 450,926 | 3.6 MB | ∞ (always usable) |

Format: a 24-byte header — magic `OFBM`,
version, `altmax`, point count, xxh32 checksum, flags — followed by i32 micro-degree
deltas, with each ring's delta sequence restarting at zero.

## Roadmap

- [x] 3D globe with realistic ocean shaders
- [ ] Real-time weather data integration (API)
- [ ] Submarine cable fault detection & visualization
- [ ] Wind farm power output simulation
- [ ] AIS vessel tracking integration
- [ ] Species migration path prediction (ML)
- [ ] Multi-user collaborative monitoring
- [ ] Historical event replay & analysis
- [ ] Unified asset register and lifecycle state (point/line/area)
- [ ] Geohazard and climate change factor ingestion (open data sources)
- [ ] Alert response closed loop (task flow and outcome write-back)

## License

MIT License
