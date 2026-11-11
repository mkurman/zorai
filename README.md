<div align="center" style="display: flex; flex-direction: row; justify-content: center; align-items: center; gap: 32px; width: 100%">
  <div><img src="docs/assets/og_image.jpg" width=640/></div>
  <b style="font-size: 22px">Read our docs</b>
  <div class="display: flex; flex-direction: row; gap: 4px">
    <a href="https://docs.zorai.app" style="text-decoration: none">🌐</a>
    <a style="font-size: 16px; text-decoration: none" href="https://docs.zorai.app">docs.zorai.app</a>
  </div>
</div>

# zorai

**Fully agentic daemon runtime for durable AI work.**

Zorai is a persistent, multi-agent, auditable, learning execution platform where the daemon owns work, memory, approvals, tools, and long-running goals.

Official website: [https://zorai.app](https://zorai.app)

Zorai keeps the agent runtime, operator surfaces, tools, memory, and governed execution in one place. Threads, workspace tasks, approvals, and goal runs live in the daemon, so work can keep moving even when the UI closes.

The name comes from **Zora**: mythic but clean, suggesting dawn, awakening, watchfulness, and beginnings. It nods to Slavic dawn and light motifs without leaning on a brittle one-to-one mythological claim. The final **i** completes the AI identity.

In practice that means:

- Electron GUI, the TUI, the CLI, MCP clients, and chat gateways all reconnect to the same daemon state
- the built-in runtime can plan work, run tools, spawn bounded sub-agents, pause for approval, and learn over time
- memory, workspace boards, execution queue state, and operational history stay durable instead of vanishing with a single terminal tab

## Best Practices

Before you jump into installation and setup, read the operational guidance in [`docs/best-practices.md`](docs/best-practices.md). It covers model role selection, compaction strategy, governance usage, cost control, and the day-to-day habits that make zorai work well.

## Quick Start

### Quick Install

Linux/macOS:

```bash
curl -fsSL https://raw.githubusercontent.com/mkurman/zorai/main/scripts/install.sh | sh
zorai --help
```

Windows PowerShell, run as Administrator:

```powershell
irm https://raw.githubusercontent.com/mkurman/zorai/main/scripts/install.ps1 | iex
zorai --help
zorai gui
```

The quick installer downloads the same native release bundle family that the npm package uses, installs the binaries into `~/.local/bin` on Linux/macOS or `C:\Program Files\zorai` on Windows by default, and works without Node.js or npm. Windows x64 and Windows ARM64 use architecture-matched bundles, and every bundle includes the full app surface: CLI, daemon, TUI, MCP/gateway binaries, and the desktop GUI launcher (`zorai-desktop.exe` on Windows).

After a quick-install or direct-binary install, `zorai upgrade` reuses the direct installer path for the current install directory. If you installed through npm, `zorai upgrade` continues to use npm.

### NPM

```bash
npm install -g zor-ai
zorai --help

# or install locally in a project:
npm install zor-ai
npx zor-ai --help
```

If `npm install -g zor-ai` succeeds but `zorai` is still not found on macOS, your npm global bin directory is not on `PATH` yet:

```bash
export PATH="$(npm config get prefix)/bin:$PATH"
exec $SHELL -l
zorai --help
```

If the bin directory is already on `PATH`, opening a new shell is still useful because `zsh` and `bash` can cache command lookups.

When zorai is installed through npm, `zorai upgrade` upgrades through npm as well.

The npm package also installs the full platform bundle on every supported platform. On Windows, `npm install -g zor-ai` installs `zorai-desktop.exe` alongside the CLI binaries so `zorai gui` works after install.

### From sources

```bash
git clone https://github.com/mkurman/zorai.git

cd zorai

# 1. Start the daemon
cargo run --release --bin zorai-daemon

# 2. Launch the TUI in another terminal
cargo run --release --bin zorai setup

# or
cargo run --release --bin zorai-tui

# 3. Or launch the desktop app
cd frontend && npm install && npm run dev:electron
```

If you want to test the core loop fast, start a goal run and give Swarog a concrete objective: investigate a failing build, prepare a release checklist, or trace a bug across a workspace.

## What It Feels Like

zorai is for operators who want agents that stay with durable work and control surfaces that make long-running execution visible instead of mysterious.

Terminals remain first-class tools, especially for engineering work, but they are no longer the product's core premise. The center is the agentic runtime: planning, governed tool use, memory, collaboration, review, and durable goal execution.

## The TUI

The TUI is a keyboard-first control room for the daemon with mouse support.

- inspect sessions, threads, workspace tasks, approvals, and goal runs without leaving the terminal
- move between operator control and agent execution from the same live state used by Electron and the CLI
- keep working over SSH, inside tmux, or anywhere a browser UI is the wrong tool

Thread command reference: [`docs/tui/agent-directives.md`](docs/tui/agent-directives.md)

## The Fires

zorai gives its daemon-side agents a slightly mythic face, but the work stays concrete.

- **Swarog** is the main working fire: planning, tool use, sub-agent orchestration, memory, and durable goal runs
- **Rarog** is the guiding flame: onboarding, check-ins, operator context, and the gentler edge of the system
- **Weles** is the underwatch: governance, risk review, and guarded inspection when the runtime needs a second set of eyes
- **Swarozyc** is the quick worker: narrower, faster execution and focused implementation help
- **Radogost** is the negotiator: tradeoff analysis, comparison, and routing toward the strongest next move
- **Domowoj** is the keeper: local stability, cleanup, careful repair, and environment-aware fixes
- **Swietowit** is the wide watcher: broader architectural awareness and thread-level situational context
- **Perun** is the thunder hand: decisive execution, infrastructure discipline, and security-minded action
- **Mokosh** is the earth keeper: maintenance, reliability, and durable operational care
- **Dazhbog** is the bright giver: synthesis, explanation, and turning ambiguity into a useful next move

### Working With Them

Use `@agent ...` to add or update a visible thread participant on the current thread.

Examples:

- `@weles verify claims before answering`
- `@swarozyc review svarog's implementation and step in when needed`
- `@perun drive the risky migration plan to a concrete decision`
- `@mokosh stabilize this workspace and clean up the rough edges`
- `@dazhbog turn this discussion into a clear plan and operator update`

Behavior:

- the current thread owner does not change
- the participant watches visible thread activity
- when it decides to contribute, it adds a thread message or a queued visible suggestion instead of using hidden internal messages
- repeating `@agent ...` updates that participant instead of creating duplicates
- to stop or remove agent from participants list use @agent stop or @agent leave

Use `!agent ...` for hidden internal DM.

Examples:

- `!weles check whether this is risky`
- `!radogost compare these two approaches before we answer`
- `!perun assess the operational blast radius before we proceed`
- `!dazhbog summarize the strongest next move in plain language`

Behavior:

- the current visible thread stays where it is
- the target agent is contacted on the hidden internal path
- this is for behind-the-scenes coordination, not visible participation

Use handoff when another agent should own future replies in the thread.

Behavior:

- a handoff changes the active responder for the thread
- future operator messages route to that agent until a return handoff
- zorai records the switch as a visible system event while keeping linked handoff context hidden

### Registration And Setup

Builtin personas such as `@swarozyc`, `@radogost`, `@domowoj`, `@swietowit`, `@perun`, `@mokosh`, and `@dazhbog` need their own provider and model configuration before they can participate or receive internal DM.

If one of those personas is available but not configured yet:

- the desktop app opens a provider then model setup flow
- the TUI opens the same provider then model picker flow
- after setup, zorai retries the original `@agent ...` or `!agent ...` request automatically

`@veles` is accepted as an alias for `@weles`, matching the existing `swarog` and `svarog` alias handling.

If the target is not a known built-in persona or registered sub-agent alias, the leading `@agent` form is not treated as an agent directive and remains a normal message/file-mention path.


Together they give the system a little presence without hiding what it is doing.

## 🔊 Speech to Text / Text to Speech (Pinned)

Zorai supports voice workflows in both TUI and desktop app.

- **TUI:** `Ctrl+L` (record/transcribe), `Ctrl+P` (speak selected/latest assistant message), `Ctrl+S` (stop playback)
- **Desktop app:** mic/speak controls with daemon-backed STT/TTS settings
- **Settings persistence:** daemon config `extra.audio_*`

## Community
- 🗨️ Discord: [Join us!](https://discord.gg/QVXkqaNSUS)

## Read More

### Documentation

- [Best Practices](docs/best-practices.md)
- [Reference](docs/reference.md)
- [Getting Started](docs/getting-started.md)
- [Providers](docs/pages/providers.html)
- [Custom Providers](docs/pages/custom-providers.html)
- [How zorai Works](docs/how-zorai-works.md)
- [Thread Participants](docs/operating/thread-participants.md)
- [Workspaces](docs/workspaces.md)
- [Speech to Text / Text to Speech](docs/speech-to-text-and-text-to-speech.md)
- [Goal Runners](docs/goal-runners.md)
- [Self-Orchestrating Agent Architecture](docs/self-orchestrating-agent.md)
- [Agentic Mission Control](docs/agentic-mission-control.md)
- [TUI Capabilities Map](docs/tui/capabilities-map.md)
- [TUI README](crates/zorai-tui/README.md)
- [Speech-to-text / Text-to-speech](docs/speech-to-text-and-text-to-speech.md)
- [Plugin Development](docs/plugin-development.md)

## License

[MIT](LICENSE)

## MLflow conversation tracing

Zorai can export daemon-wide agent turns, model phases, and tool calls to MLflow 3.6+ over OTLP/HTTP. Tracing is disabled by default, privacy-controlled, bounded, and fail-open. See [`docs/mlflow-tracing.md`](docs/mlflow-tracing.md) for local server setup, Desktop/TUI configuration, capture modes, scopes, encrypted headers, and troubleshooting.
