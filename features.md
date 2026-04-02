# winmon Feature Tracking

Target alignment: `macmon v0.7.0`

## Upstream

- Source: `https://github.com/vladkens/macmon`
- Last synced upstream version: `v0.7.0 (432225a, 2026-04-01)`
- To clone that upstream version: `git clone --branch v0.7.0 --single-branch https://github.com/vladkens/macmon.git`
- Notes: `winmon` keeps the overall TUI layout and interaction model from that baseline, but Windows-side sampling, bootstrap, packaging, and release flow are rewritten around PowerShell, `nvidia-smi`, and `OpenHardwareMonitorLib.dll`

Notes:
- This file only tracks user-visible features and fixes from upstream `changelog.md` that matter to `winmon`
- The implementation can differ as long as the user-facing result is close enough
- macOS-only items are marked `❌` when they do not fit Windows

Status:
- `✅` implemented
- `🟡` pending
- `❌` not planned

## v0.7.0

| Type | Upstream item | Status | Notes |
| --- | --- | --- | --- |
| Feature | HTTP server mode with JSON and Prometheus endpoints | ✅ | `serve` supports `/json` and `/metrics` |
| Feature | launchd service install/uninstall for HTTP server | ❌ | `launchd` is macOS-specific; Windows would need a different path |
| Feature | `cpu_usage_pct` metric | ✅ | Exposed through `pipe`, `debug`, and `serve` |
| Feature | RAM usage percentage display in the label | ✅ | Added to the TUI |
| Feature | Exposed as a library crate | ❌ | Current scope stays CLI/TUI only |
| Fix | Discard bogus sensor temperature readings | ✅ | Basic filtering and smoothing are in place |
| Fix | M5 related voltage-states / processor count fixes | ❌ | Apple Silicon specific |

## v0.6.1

| Type | Upstream item | Status | Notes |
| --- | --- | --- | --- |
| Feature | SoC info output in pipe/JSON mode | ✅ | `winmon pipe --device-info` covers the equivalent use case |

## v0.6.0

| Type | Upstream item | Status | Notes |
| --- | --- | --- | --- |
| Feature | Timestamp field in pipe output | ✅ | `pipe` already includes `timestamp` |
| Fix | Temperature smoothing when sensors are unavailable | ✅ | Missing-value fallback and smoothing are in place |

## v0.5.1

| Type | Upstream item | Status | Notes |
| --- | --- | --- | --- |
| Improvement | Improved CPU average temperature calculation via SMC | ❌ | SMC is macOS-specific |

## v0.5.0

| Type | Upstream item | Status | Notes |
| --- | --- | --- | --- |
| Feature | Interactive refresh interval hotkeys | ✅ | `-` and `+` are supported |
| Feature | `--interval` allowed in any argument position | ✅ | `interval` is a global option |
| Fix | CPU power reporting for Ultra chips | ❌ | Apple Silicon specific |

## v0.4.2

| Type | Upstream item | Status | Notes |
| --- | --- | --- | --- |
| Feature | RAM power metric | ❌ | No reliable generic source right now |
| Feature | Sample count limit for pipe command | ✅ | `pipe -s` is already supported |

## v0.4.0

| Type | Upstream item | Status | Notes |
| --- | --- | --- | --- |
| Feature | Raw metrics output in JSON via pipe | ✅ | Implemented |
| Improvement | Smooth interpolation of temperature and power values | 🟡 | Refresh is near 1s, but display-side interpolation is still lighter than upstream |
| Fix | GPU frequency reporting | ✅ | Implemented through the Windows/NVIDIA path |

## v0.3.0

| Type | Upstream item | Status | Notes |
| --- | --- | --- | --- |
| Feature | Switch chart type | ✅ | `v` switches between sparkline and gauge |
| Feature | Settings persistence between sessions | ✅ | Color, view, and interval persist in `%APPDATA%\\winmon\\config.json` |

## v0.2.1

| Type | Upstream item | Status | Notes |
| --- | --- | --- | --- |
| Feature | Total system power display | ❌ | No longer pursued; `N/A` is acceptable long-term |
| Feature | `--no-color` mode | ❌ | Not implemented, low priority |

## v0.2.0

| Type | Upstream item | Status | Notes |
| --- | --- | --- | --- |
| Feature | CPU/GPU average temperature display | ✅ | Implemented; CPU temperature comes from `OpenHardwareMonitorLib.dll` |
| Feature | Ability to change colors | ✅ | `c` is supported |
| Feature | Version label in the UI | ✅ | Implemented |
| Improvement | Better E/P CPU frequency calculation | ✅ | Equivalent behavior exists through Windows counters and OHM |

## Current Notes

- `sys_power` is no longer pursued and stays `N/A`
- The Grafana example only keeps metrics that `winmon` outputs reliably today
