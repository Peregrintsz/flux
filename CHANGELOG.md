# Changelog

This project follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.3.0] - 2026-10-23

### Added
- Toggle and Hold activation modes with an in-app selector.
- Hold the global shortcut to click only while pressed.
- Refreshed Flux logo in the app and embedded Windows Explorer icon.

### Changed
- Stop clicking when switching activation mode or recording a shortcut.
- Publish the portable Windows executable directly as `flux.exe`.
- Link the Windows C runtime statically so the portable build does not require a separate Visual C++ runtime installation.

## [0.2.0] - 2026-10-23

### Added
- One-click switching between dark and light themes.

### Changed
- Improved text contrast and softened the light theme colors.
- Removed the unused app header and filled the full window background.
- Replaced the fixed hotkey list with configurable keyboard shortcuts.

## [0.1.0] - 2026-10-23

### Added
- Compact desktop UI built with Rust and egui.
- Left, right, and middle mouse button selection.
- Click interval settings in CPS or milliseconds.
- Configurable global start/stop hotkey, defaulting to F6.
- Click loop on a separate thread with safe stop handling.
