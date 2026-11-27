# Zorai Reference

This document collects the practical reference material that used to live in the top-level README: providers, configuration, shortcuts, packaging, runtime integration, and development notes.

For onboarding, see [getting-started.md](getting-started.md). For runtime architecture, see [how-zorai-works.md](how-zorai-works.md). For deeper agent internals, see [self-orchestrating-agent.md](self-orchestrating-agent.md). For the canonical memory architecture, see [zorai-memory.md](zorai-memory.md). For the canonical security and governance model, see [zorai-security.md](zorai-security.md). For routine creation, preview, run-now, rerun, and recovery workflows, see [operating/routines.md](operating/routines.md).

## Paths And Locations

### Config File Locations

| Platform | Path |
|---|---|
| Linux | `~/.config/zorai/config.json` |
| macOS | `~/Library/Application Support/zorai/config.json` |
| Windows | `%APPDATA%\zorai\config.json` |

Data directory: `~/.zorai/` on Unix, `%LOCALAPPDATA%\zorai\` on Windows. Existing `zorai` directories are migrated forward when possible.

### Install Locations

- Desktop app binaries (packaged): inside application resources, typically `resources/bin`
- Runtime data directory: `~/.zorai` on Unix or `%LOCALAPPDATA%\zorai` on Windows
- Source builds: binaries are produced in `target/debug` or `target/release`

## Supported Providers

| Provider | ID | Default Model | Base URL |
|---|---|---|---|
| Featherless | `featherless` | meta-llama/Llama-3.3-70B-Instruct | api.featherless.ai |
| OpenAI | `openai` | gpt-4o | api.openai.com |
| Anthropic | `anthropic` | claude-sonnet-4-20250514 | api.anthropic.com |
| Qwen | `qwen` | qwen-max | api.qwen.com |
| Qwen (DeepInfra) | `qwen-deepinfra` | Qwen/Qwen2.5-72B-Instruct | api.deepinfra.com |
| Kimi (Moonshot) | `kimi` | moonshot-v1-32k | api.moonshot.ai |
| Kimi Coding Plan | `kimi-coding-plan` | kimi-for-coding | api.kimi.com/coding |
| Z.AI (GLM) | `z.ai` | glm-4-plus | api.z.ai |
| Z.AI Coding Plan | `z.ai-coding-plan` | glm-5 | api.z.ai/api/coding/paas/v4 |
| OpenRouter | `openrouter` | arcee-ai/trinity-large-thinking | openrouter.ai |
| Cerebras | `cerebras` | llama-3.3-70b | api.cerebras.ai |
| Together | `together` | meta-llama/Llama-3.3-70B-Instruct-Turbo | api.together.xyz |
| Groq | `groq` | llama-3.3-70b-versatile | api.groq.com |
| Ollama (local) | `ollama` | llama3.1 | localhost:11434 |
| Chutes | `chutes` | deepseek-ai/DeepSeek-V3 | llm.chutes.ai |
| Hugging Face | `huggingface` | meta-llama/Llama-3.3-70B-Instruct | router.huggingface.co |
| MiniMax | `minimax` | MiniMax-M1-80k | api.minimax.io |
| MiniMax Coding Plan | `minimax-coding-plan` | MiniMax-M2.7 | api.minimax.io/anthropic |
| Alibaba Coding Plan | `alibaba-coding-plan` | qwen3-coder | coding-intl.dashscope.aliyuncs.com |
| Alibaba Token Plan | `alibaba-token-plan` | qwen3.7-max | token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode |
| OpenCode Zen | `opencode-zen` | claude-sonnet-4-6 | opencode.ai/zen |
| OpenCode Go | `opencode-go` | glm-5.1 | opencode.ai/zen/go |
| Custom | `custom` | user-defined | user-defined |

### API Formats

- Most providers use OpenAI-compatible `/chat/completions` endpoints
- Anthropic, MiniMax, and MiniMax Coding Plan use the Anthropic Messages API at `/v1/messages`
- Alibaba Coding Plan supports both OpenAI-compatible `/v1` and Anthropic-compatible `/apps/anthropic`, auto-selected by model name
- Alibaba Token Plan is the pay-as-you-go (`sk-xxx`) tier at `token-plan.ap-southeast-1.maas.aliyuncs.com`, exposing OpenAI-compatible `/compatible-mode/v1` and Anthropic-compatible `/apps/anthropic` endpoints (auto-selected from the base URL), separate from the Coding Plan subscription (`sk-sp-xxx`)
- OpenCode Zen (`opencode.ai/zen/v1`) and OpenCode Go (`opencode.ai/zen/go/v1`) are separate providers; each supports both `chat_completions` and `anthropic_messages` transports (selectable), auto-selects Anthropic format for Claude models and OpenAI-compatible format for others, and fetches its live model list from `/models`

Switch providers at any time from the Settings panel. Each provider's base URL, model, and API key are independently configurable, and models can be selected from a searchable list or entered manually.

When OpenRouter is selected, Zorai automatically sends app attribution headers using `https://zorai.app` and the `zorai` title so usage can appear in OpenRouter analytics and rankings.

## Build And Run

### Build

```bash
# Rust workspace
cargo build --release

# Individual Rust crates
cargo build --release -p zorai-daemon
cargo build --release -p zorai-cli
cargo build --release -p zorai-gateway
cargo build --release -p zorai-mcp
cargo build --release -p zorai-protocol

# Frontend web bundle
cd frontend
npm install
npm run build

# Electron desktop package
cd frontend
npm run build:electron
```

Recommended slice:

- backend-only changes: `cargo build --release` or the specific `-p` crate
- frontend UI changes: `cd frontend && npm run build`
- Electron bridge or preload changes: `cd frontend && npm run build` and then `cd frontend && npm run dev:electron`
- packaging changes: `cd frontend && npm run build:electron`

### Run

```bash
# Start the daemon
cargo run --release --bin zorai-daemon

# Launch the Electron app
cd frontend
npm run dev:electron

# Or use the CLI directly
cargo run --release --bin zorai -- list
cargo run --release --bin zorai -- new --shell bash
cargo run --release --bin zorai -- attach <session-id>
cargo run --release --bin zorai -- kill <session-id>
cargo run --release --bin zorai -- ping
```

## Release Packaging

Use the platform-specific scripts when you want upload-ready artifacts gathered under `dist-release/`.

```bash
# Linux native release bundle
./scripts/build-release.sh

# Rebuild the full dist-release layout
./scripts/build-production-releases.sh

# Native macOS release bundle (run on macOS)
./scripts/build-release-macos.sh

# Windows cross-build from WSL/Linux
./scripts/build-release-wsl.sh
```

Windows native builds:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build-release.ps1
scripts\build-release.bat
```

The production wrapper `./scripts/build-production-releases.sh` deletes and recreates `dist-release/`, builds the native Linux release into `dist-release/linux/`, and builds the Windows cross-release into `dist-release/windows/` when `mingw-w64` is installed.

Typical output layout:

```text
dist-release/
  linux/
    zorai
    zorai-daemon
    zorai-gateway
    zorai-mcp
    GETTING_STARTED.md
    zorai-<version>.AppImage
    zorai_<version>_amd64.deb
    zorai-linux-x86_64.zip
    zorai-linux-aarch64.zip
    SHA256SUMS.txt
    RELEASE_NOTES.md
  windows/
    zorai.exe
    zorai-daemon.exe
    zorai-gateway.exe
    zorai-mcp.exe
    GETTING_STARTED.md
    zorai-portable.exe
    zorai Setup <version>.exe
    zorai-windows-x64.zip
    SHA256SUMS.txt
    RELEASE_NOTES.md
```

Wrapper options:

- `--native-only` builds only the native platform release
- `--windows-only` builds only the Windows cross-release
- `--skip-rust`, `--skip-frontend`, and `--skip-electron` are passed through to the native release script
- `--target <triple>` is passed through to the native release script
- `--sign` enables signing in child scripts

Host expectations:

- Linux host: can build Linux Rust binaries and Linux Electron artifacts
- Linux/WSL host: can also cross-compile Windows Rust binaries and package Windows Electron artifacts when `mingw-w64` is installed
- macOS host: required for signed macOS app bundles, DMGs, and notarization
- Windows host: required for the most reliable signed Windows installers

Recommended prerequisites on Linux:

```bash
sudo apt update
sudo apt install -y mingw-w64
cargo build --release
cd frontend && npm ci && cd ..
```

Signing environment variables:

- `ZORAI_SIGN_CERT` / `ZORAI_SIGN_PASSWORD` for PFX-based signing
- `ZORAI_SIGN_THUMBPRINT` for Windows certificate store signing
- `ZORAI_SIGN_IDENTITY` for macOS `codesign`

Legacy `ZORAI_*` signing variables are still accepted for compatibility.

## Notifications

Zorai supports in-app attention notifications emitted from terminal output using OSC sequences.

Supported formats:

- `OSC 9`: `9;<message>`
- `OSC 777`: `777;notify;<title>;<body>`
- `OSC 99`: `99;<text>` or metadata + `;<text>`

You can open the in-app notification panel with `Ctrl+I`.

Quick smoke test commands:

```bash
printf '\033]9;Claude needs attention\007'
printf '\033]777;notify;Claude;Waiting for your input\007'
printf '\033]99;Codex finished task\007'
```

Optional shell helpers:

```bash
osc9() {
  printf '\033]9;%s\007' "$*"
}

osc777() {
  local title="$1"; shift
  printf '\033]777;notify;%s;%s\007' "$title" "$*"
}

osc99() {
  printf '\033]99;%s\007' "$*"
}
```

## Webhook Event Ingest

The daemon can expose a narrow local HTTP webhook listener that feeds Pack 1 event payloads into the same trigger engine used by internal event ingestion. This is implemented today as a localhost listener inside `zorai-daemon`, not as a separate gateway service.

Implemented behavior:

- Disabled by default
- Configured through `AgentConfig.extra` keys in `config.json`
- Binds `127.0.0.1:8787` by default
- Accepts `POST /webhook/event` only
- Requires a JSON body matching the `ingest_webhook_event` payload shape
- When a secret is configured, requires both timestamp and HMAC headers

`config.json` example:

```json
{
  "extra": {
    "webhook_listener_enabled": true,
    "webhook_listener_bind": "127.0.0.1:8787",
    "webhook_listener_secret": "replace-me",
    "webhook_listener_max_age_secs": 300
  }
}
```

Supported `extra` keys:

| Key | Type | Default | Meaning |
|---|---|---|---|
| `webhook_listener_enabled` | boolean | `false` | Start the local HTTP listener when the daemon run loop starts. |
| `webhook_listener_bind` | string | `127.0.0.1:8787` | Bind address for the listener. |
| `webhook_listener_secret` | string | unset | Enables signed-request validation when non-empty. |
| `webhook_listener_max_age_secs` | integer | `300` | Maximum allowed timestamp skew for signed requests. |

Request shape:

```http
POST /webhook/event
Content-Type: application/json
```

Body example:

```json
{
  "event_family": "filesystem",
  "event_kind": "file_changed",
  "state": "detected",
  "thread_id": "thread-demo-1",
  "payload": {
    "path": "src/lib.rs"
  }
}
```

Accepted top-level fields:

- `event_family` (required)
- `event_kind` (required)
- `state` (optional)
- `thread_id` (optional)
- `payload` (optional object, forwarded into trigger rendering and event logs)

### Signed webhook requests

If `webhook_listener_secret` is set, requests must include:

- `x-zorai-timestamp-ms`: Unix timestamp in milliseconds
- `x-zorai-signature-256`: `sha256=<hex-hmac>`

The daemon computes the expected signature over:

```text
<timestamp_ms> + "." + <raw_request_body>
```

using HMAC-SHA256 and the configured secret. Requests are rejected when the timestamp is missing, invalid, outside the allowed age window, or the signature does not match.

Response behavior:

- `202 Accepted` for a valid payload accepted into the trigger engine
- `400 Bad Request` for malformed HTTP or invalid JSON/payload
- `401 Unauthorized` for missing/invalid timestamp or signature when signing is enabled
- `404 Not Found` for paths other than `/webhook/event`
- `405 Method Not Allowed` for non-`POST` requests

This listener routes through the same `ingest_webhook_event_json` foundation used by the internal `ingest_webhook_event` tool, so seeded default triggers such as `filesystem/file_changed` and `system/disk_pressure` can fire without separate manual registration.

## Configuration

The built-in Settings panel exposes configuration in these sections:

- Appearance: font family, font size, theme, opacity, line height, padding, and custom terminal colors
- Cursor: style, blink toggle, and blink interval
- Terminal: default shell, shell arguments, scrollback, bell, visual bell, bracketed paste
- Behavior: session restore, close confirmation, auto-copy, URL opening, auto-save, transcript capture, and retention periods
- Infrastructure: sandbox, network policy, snapshot backend, WORM integrity, Cerbos PDP endpoint
- Agent: provider selection, per-provider API keys and models, system prompt, streaming, memory, compaction, bash tool, and web search settings
- Keybindings: custom key combinations and reset-to-defaults

To use Parallel for the agent's `web_search` tool, enable Web Search and select
Parallel in the search provider picker, or choose Parallel in `zorai setup`.
From the CLI, run `zorai settings set search_provider parallel` and
`zorai settings set tools.web_search true`.
No API key is needed. Zorai sends the search query to
[`https://mcp.parallel.ai/mcp`](https://mcp.parallel.ai/mcp) with a
`zorai-daemon/<version>` User-Agent so Parallel can count aggregate Zorai usage.

## Keyboard Shortcuts

All keybindings are customizable from the Settings panel or by editing `keybindings.json`.

### Pane Management

| Action | Default Binding |
|---|---|
| Split horizontal | `Ctrl+D` |
| Split vertical | `Ctrl+Shift+D` |
| Close active pane | `Ctrl+Shift+W` |
| Toggle zoom pane | `Ctrl+Shift+Z` |
| Focus left pane | `Ctrl+Alt+Left` |
| Focus right pane | `Ctrl+Alt+Right` |
| Focus upper pane | `Ctrl+Alt+Up` |
| Focus lower pane | `Ctrl+Alt+Down` |

### Surfaces

| Action | Default Binding |
|---|---|
| New surface | `Ctrl+T` |
| Close surface | `Ctrl+W` |
| Next surface | `Ctrl+Tab` |
| Previous surface | `Ctrl+Shift+Tab` |

### Workspaces

| Action | Default Binding |
|---|---|
| New workspace | `Ctrl+Shift+N` |
| Switch workspace 1-9 | `Ctrl+1` through `Ctrl+9` |
| Next workspace | `Ctrl+PageDown` |
| Previous workspace | `Ctrl+PageUp` |

### Panels And Overlays

| Action | Default Binding |
|---|---|
| Command palette | `Ctrl+Shift+P` |
| Toggle sidebar | `Ctrl+B` |
| Toggle notifications | `Ctrl+I` |
| Toggle settings | `Ctrl+,` |
| Toggle session vault | `Ctrl+Shift+V` |
| Toggle command log | `Ctrl+Shift+L` |
| Toggle search | `Ctrl+Shift+F` |
| Toggle command history | `Ctrl+Alt+H` |
| Toggle snippets | `Ctrl+S` |
| Toggle agent panel | `Ctrl+Shift+A` |
| Toggle system monitor | `Ctrl+Shift+M` |
| Toggle execution canvas | `Ctrl+Shift+G` |
| Toggle time-travel snapshots | `Ctrl+Shift+T` |

## Plugins And MCP

Runtime-installed plugins are supported through `zorai install plugin <npm-package-or-local-path>`.

External npm plugins should declare `zoraiPlugin.entry` in `package.json`. The entry should be a self-contained browser script that registers itself through `window.ZoraiApi.registerPlugin(...)`.

Register `zorai-mcp` with Claude Code, Cursor, or any MCP-compatible client like this:

```json
{
  "mcpServers": {
    "zorai": {
      "command": "zorai-mcp"
    }
  }
}
```

For deeper plugin details, see [plugin-development.md](plugin-development.md). For MCP connection setup, see [skills/connection/setup.md](skills/connection/setup.md).

## Development Reference

### Crate Layout

| Crate | Role |
|---|---|
| `zorai-protocol` | Shared message types, length-prefixed bincode codec, and configuration |
| `zorai-daemon` | Background daemon: PTY management, workspace tasks, execution queue, snapshots, policy engine, credential scrubbing, telemetry, and history |
| `zorai-cli` | Command-line client that builds the `zorai` binary |
| `zorai-gateway` | Chat platform bridge crate |
| `zorai-mcp` | MCP server crate |
| `zorai-tui` | Keyboard-first terminal UI client for the daemon |

### Key Dependencies

Rust side:

| Crate | Purpose |
|---|---|
| `tokio` | Async runtime |
| `portable-pty` | Cross-platform PTY spawning |
| `rusqlite` | SQLite with FTS5 |
| `tree-sitter` / `tree-sitter-bash` | AST parsing and indexing |
| `sysinfo` | System telemetry |
| `sha2` | WORM hash chains |
| `regex` | Risk matching and credential scrubbing |
| `serde` / `serde_json` / `bincode` | Serialization for IPC and persistence |

Frontend side:

| Package | Purpose |
|---|---|
| `@xterm/xterm` | Terminal emulation |
| `@xyflow/react` | Execution graph UI |
| `react-resizable-panels` | Split pane layout |
| `zustand` | State management |
| `vite` | Build tooling and dev server |
| `electron` | Desktop shell |
| `electron-builder` | Packaging |

### IPC Protocol

The daemon and all clients communicate via length-prefixed bincode frames over Unix domain sockets on Linux/macOS or localhost TCP on Windows. The protocol lives in `zorai-protocol` and is centered on two enums:

- `ClientMessage`: requests from clients to the daemon
- `DaemonMessage`: responses and events from the daemon back to clients
