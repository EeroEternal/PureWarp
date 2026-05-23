# <img src="web/logo.svg" width="28" alt=""> PureWarp

A clean terminal core stripped from [Warp](https://www.warp.dev) — no AI, no cloud, just GPU-accelerated rendering.

Lightweight, fast macOS terminal emulator rebuilt from the Warp codebase. AI, team collaboration, and telemetry removed. Only the high-performance Metal rendering engine remains.

## Features

- **GPU-accelerated rendering** — Metal shaders draw all text directly on the GPU
- **Native PTY** — runs zsh, bash, fish, or any shell via a real pseudo-terminal
- **VTE-compatible** — parses ANSI / VT100 / xterm escape sequences
- **Alternate screen buffer** — full support for TUI programs like vim, htop, and less
- **Per-cell truecolor** — 24-bit color rendering with automatic contrast
- **CJK IME input** — native Chinese/Japanese/Korean input method support on macOS
- **Scrollback** — up to 10,000 lines of history, scroll with the mouse wheel
- **Custom theming** — TOML config file for fonts, colors, cursor style, and terminal size
- **No bloat** — no AI, no telemetry, no sign-in, no subscription

## Installation

### Download DMG

Download the latest `.dmg` from [GitHub Releases](https://github.com/EeroEternal/PureWarp/releases/latest). Open the DMG and drag PureWarp.app into your Applications folder.

Requires **macOS 13** (Ventura) or later.

### Install via Cargo

```bash
cargo install --git https://github.com/EeroEternal/PureWarp pure_warp
```

### Build from Source

```bash
git clone https://github.com/EeroEternal/PureWarp.git
cd PureWarp
cargo build --release -p pure_warp
```

The binary lands at `./target/release/purewarp`.

> **Font**: PureWarp defaults to **JetBrains Mono**. If not installed, it falls back to SF Mono, Menlo, Monaco, or Courier. Install [JetBrains Mono](https://www.jetbrains.com/lp/mono/) for the best experience.

## Configuration

PureWarp reads configuration from `~/.config/purewarp/config.toml`. If the file doesn't exist, built-in defaults are used.

```toml
[shell]
program = "/bin/zsh"           # Default: $SHELL environment variable, or /bin/bash
args = []                       # Additional arguments passed to the shell

[terminal]
font_size = 14.0                # Font size in points
font_family = "JetBrains Mono"  # Font family name (falls back to system fonts if not found)
cursor_style = "block"          # "block", "underline", or "beam"
cols = 80                       # Initial terminal columns
rows = 24                       # Initial terminal rows
max_scrollback = 10000          # Maximum scrollback buffer lines

[theme]
background = "#f6edda"          # Background color (hex)
foreground = "#005661"          # Text color (hex)
cursor = "#00c6e0"              # Cursor color (hex)
palette = [                     # ANSI 16-color palette (normal 0-7, bright 8-15)
  "#003b42", "#e34e1c", "#00b368", "#f49725",   # black, red, green, yellow
  "#0094f0", "#ff5792", "#00bdd6", "#8ca6a6",   # blue, magenta, cyan, white
  "#004d57", "#ff4000", "#00d17a", "#ff8c00",   # bright black, red, green, yellow
  "#0fa3ff", "#ff6b9f", "#00cbe6", "#bbc3c4",   # bright blue, magenta, cyan, white
]
```

The default theme is **Noctis Lux** (a light theme). Restart PureWarp after editing the config file for changes to take effect.

## Keyboard Shortcuts

| Category | Keys |
|----------|------|
| **Input** | All printable characters, Enter, Tab, Backspace, Delete, Escape |
| **Navigation** | Arrow keys, Home, End, PageUp, PageDown, Insert |
| **Function keys** | F1 through F12 |
| **Control codes** | Ctrl+A through Ctrl+Z, Ctrl+[, Ctrl+\\, Ctrl+], Ctrl+^, Ctrl+/ |
| **Scrollback** | Mouse wheel to scroll through history |
| **IME** | macOS native Chinese/Japanese/Korean input methods |

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

## Contributing

Contributions are welcome. Please open an issue or submit a pull request on [GitHub](https://github.com/EeroEternal/PureWarp).

Before submitting a PR, make sure:

- `cargo build -p pure_warp` compiles without errors
- `cargo clippy --workspace -- -D warnings` passes with no warnings
- `cargo fmt --all` has been run
- `cargo test --workspace` passes

Commit messages should follow [Conventional Commits](https://www.conventionalcommits.org/) (e.g. `feat:`, `fix:`, `chore:`, `docs:`).

## License

MIT
