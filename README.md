# <img src="web/logo.svg" width="28" alt=""> PureWarp

A clean terminal core stripped from [Warp](https://www.warp.dev) — no AI, no cloud, just GPU-accelerated rendering.

Lightweight, fast macOS terminal emulator rebuilt from the Warp codebase. AI, team collaboration, and telemetry removed. Only the high-performance Metal rendering engine remains.

## Features

- **GPU-accelerated rendering** — Metal shaders draw all text directly on the GPU
- **Native PTY** — runs zsh, bash, or any shell via a real pseudo-terminal
- **VTE-compatible** — parses ANSI / VT100 / xterm escape sequences
- **Scrollback** — up to 1,000 lines of history, scroll with the mouse wheel
- **Custom theming** — TOML config file for color schemes
- **No bloat** — no AI, no telemetry, no sign-in, no subscription

## Build

```bash
git clone https://github.com/EeroEternal/PureWarp.git
cd PureWarp
cargo build --release -p pure_warp
```

The binary lands at `./target/release/purewarp`.

## Run

```bash
cargo run --release -p pure_warp
```

Default window is 700×450; tweak via `AddWindowOptions`.

## Project Structure

```
app/                    # Application entry point + terminal view
crates/
├── terminal_backend/   # PTY session, terminal state, VTE parser
├── warpui/             # macOS native UI framework (Metal rendering)
├── warpui_core/        # UI core (Flex / Stack / Event system)
├── editor/             # Text editor component
├── languages/          # Syntax highlighting (Tree-sitter grammars)
├── markdown_parser/    # Markdown parser
├── command/            # Subprocess command wrapper
├── settings/           # Configuration system
├── fuzzy_match/        # Fuzzy matching engine
├── input_classifier/   # Input classifier
└── ...
```

## Tech Stack

- **Rust** 2021 edition
- **Metal** (macOS GPU API) — runtime shader compilation
- **PTY** — `nix` + `portable-pty`
- **VTE** — custom UTF-8 state-machine parser
- **warpui** — custom declarative, reactive UI framework

## License

MIT
