# Changelog

All notable changes to PureWarp are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.3] - 2026-05-21

### Added
- Chinese/Japanese/Korean (CJK) IME input support via macOS native input methods
- Per-cell truecolor (24-bit) rendering with auto-contrast
- Alternate screen buffer support (`CSI ?1047/1048/1049`) for TUI programs (vim, htop, less, etc.)
- Configurable font family with fallback chain (JetBrains Mono -> SF Mono -> Menlo -> Monaco -> Courier)

### Fixed
- Restore top+left padding for macOS traffic-light buttons
- Render each row as a single Text element to eliminate sub-pixel cell spacing drift
- Set `line_height_ratio` to 1.0 for terminal cells to match macOS Terminal spacing
- Revert default rows to 24 for proper window fill

## [0.1.2] - 2026-05-10

### Added
- Visible blinking cursor with configurable style (block/underline/beam)
- Noctis Lux light theme as default
- Font preloading before first render to prevent blank initial frame

### Fixed
- Terminal initial render, cursor width, and shell interactivity
- LF no longer resets column position (ANSI/VT standard compliance)
- Respond to DA (Device Attributes) queries from shell
- Respond to DSR cursor position queries from shell
- Suppress zsh `PROMPT_SP` spurious `%` marker via `NO_PROMPT_SP` flag
- Use `std::thread` for blink timer instead of `tokio::spawn` (avoid UI thread crash)

## [0.1.1] - 2026-05-09

### Added
- Landing page website (purewarp.dev) with dual theme support
- Application icon (black background with teal border)
- Local app bundle launch script (`scripts/run_app.sh`)
- macOS DMG packaging script (`scripts/package_dmg.sh`)
- GitHub Actions release workflow for automated DMG builds

### Changed
- README rewritten in English with full documentation

### Fixed
- All compilation warnings resolved across Rust and Objective-C code

## [0.1.0] - 2026-05-09

### Added
- GPU-accelerated Metal rendering engine with runtime shader compilation
- Native PTY integration (zsh, bash, or any shell via `portable-pty`)
- VTE-compatible parser (ANSI / VT100 / xterm escape sequences)
- Scrollback buffer with mouse wheel scrolling
- TOML-based configuration system (`~/.config/purewarp/config.toml`)
- Custom `warpui` declarative reactive UI framework
- Initial release based on stripped Warp codebase

[0.1.3]: https://github.com/EeroEternal/PureWarp/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/EeroEternal/PureWarp/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/EeroEternal/PureWarp/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/EeroEternal/PureWarp/releases/tag/v0.1.0
