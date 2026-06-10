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

### 🌦️ Meteorology & Ocean Currents
- Typhoon / hurricane / storm tracking and simulation
- Ocean current systems (Gulf Stream, Kuroshio, Antarctic Circumpolar, etc.)
- Weather impact analysis on maritime assets
- Upwelling and downwelling zone monitoring

### 📡 Asset Management
- **Submarine Cables** — Cross-ocean cable routes, capacity, status monitoring
- **Offshore Wind Farms** — Turbine clusters, substations, transmission lines
- **Oil & Gas Platforms** — Fixed/semi-sub/FPSO platforms, pipeline networks
- **Vessels** — Oil tankers, LNG carriers, cargo ships, fishing/aquaculture vessels, real-time positioning
- **Navigation Routes** — Shipping lanes, EEZ boundaries, fishery zones, restricted areas

### 🐋 Marine Ecology
- Ecological zone mapping (coral reefs, kelp forests, mangroves, deep-sea vents)
- Species group tracking (whales, sea turtles, fish schools, marine mammals)
- Migration route visualization and seasonal pattern analysis

### ⚠️ Risk Control
- Multi-factor risk assessment engine (weather, current, structural, operational, environmental, geopolitical)
- Real-time alert system with severity levels (Info → Emergency)
- Risk scoring and categorization (Minimal → Critical)

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

## Roadmap

- [ ] 3D globe with realistic ocean shaders
- [ ] Real-time weather data integration (API)
- [ ] Submarine cable fault detection & visualization
- [ ] Wind farm power output simulation
- [ ] AIS vessel tracking integration
- [ ] Species migration path prediction (ML)
- [ ] Multi-user collaborative monitoring
- [ ] Historical event replay & analysis

## License

MIT License
