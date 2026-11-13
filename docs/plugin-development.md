# Creating Your Own Zorai Plugin

Zorai plugins are frontend-registered extensions. A plugin can contribute:

- React components
- command handlers
- assistant-callable tools and executors
- startup and teardown hooks

The core runtime is implemented in [frontend/src/plugins/PluginManager.ts](../frontend/src/plugins/PluginManager.ts) and [frontend/src/plugins/globalAPI.ts](../frontend/src/plugins/globalAPI.ts).

## Current Plugin Models

Zorai now supports two plugin paths.

### 1. In-Tree Plugins

This is the model used by the built-in coding-agents plugin.

1. Add plugin source code under [frontend/src/plugins](../frontend/src/plugins).
2. Import and call its `registerPlugin` function from the frontend bootstrap (see the existing in-tree plugins' `registerPlugin.ts` for the pattern).
3. Build and validate with the normal frontend or Electron workflows.

### 2. Runtime-Installed npm Plugins

You can now install a packaged plugin with:

```bash
zorai install plugin <npm-package>
```

You can also install from a local package directory:

```bash
zorai install plugin ../my-zorai-plugin
```

This packaged path is a distribution layer on top of the same runtime plugin contract described here.

Practical implication:

- Write plugins against the current runtime API.
- Package them as self-contained browser scripts for external installation.

### Bundled plugins

Every plugin under [plugins/](../plugins) ships with zorai: in release archives and the npm package (staged to `~/.zorai/bundled-plugins`) and inside the desktop app's resources. On start the daemon installs bundled plugins that are missing, **disabled**, and replaces the files of ones it installed earlier when the bundled version changes, keeping the user's enabled state, settings and sign-ins. Plugins installed from npm, GitHub or a folder, and plugins the user removed, are never touched. A disabled plugin contributes no skills, slash commands or sign-in checks until it is enabled. Bump `version` in `plugin.json` when you change a bundled plugin, or existing installs will not pick up the change. `ZORAI_BUNDLED_PLUGINS_DIR` points the daemon (and the installers) at a different bundle folder.

## Panels and Commands in plugin.json

Daemon plugins (the folders with a `plugin.json`, such as `plugins/zorai-plugin-github`) can add UI to the desktop app without shipping any app code. Add a `ui` section next to `api`; the desktop app reads it from `~/.zorai/plugins/<plugin>/plugin.json` and only shows it while the plugin is enabled in Settings → Plugins.

```json
"ui": {
  "commands": [
    {
      "id": "repo-info",
      "title": "Repository info",
      "endpoint": "get_repo",
      "params": {},
      "inputs": [{ "key": "owner", "label": "Owner", "placeholder": "octocat" }],
      "output": "panel",
      "placement": ["palette", "composer"]
    }
  ],
  "panels": [
    { "id": "issues", "title": "Open issues", "endpoint": "list_issues", "params": { "state": "open" }, "inputs": [] },
    { "id": "board", "title": "Board", "web": "panel/index.html", "permissions": ["composer", "toast"] }
  ]
}
```

- `id` uses letters, digits, `-` and `_`. Every `endpoint` must be one of the plugin's own `api.endpoints`; anything else is dropped and reported in the Plugins view.
- `inputs` are asked for before the call (commands) or shown above the result (panels). Typed values are merged over `params` and passed to the endpoint as `params.<key>`. Values are remembered per panel and command.
- Command `output`: `panel` shows the result in a dialog (with "Add to message"), `composer` appends it to the message box, `toast` shows a short notification. `placement`: `palette` adds a command palette entry, `composer` adds a button under the message box.
- Markdown panels (with `endpoint`) appear in the **Plugins** view and render the endpoint's text result as markdown.
- Plugins without HTTP endpoints (for example Python plugins) can still offer commands: give the command a `prompt` instead of an `endpoint`, such as `"prompt": "Use the pubmed-database plugin to: {{request}}"`. `{{key}}` placeholders are filled from `inputs`, the text is placed in the message box, and the Threads view opens so the user can review and send it; the agent then does the work with the plugin's skill.
- Endpoints using POST, PUT, PATCH or DELETE count as changes. Web panels can call them only with the `write` permission, which the user approves; confirm anything that cannot be undone (sending, deleting) inside the panel before calling it. Commands and markdown panels should stay read-only; the agent can make changes through the `plugin_api_call` tool, where Weles reviews every call.

### Web panels

A panel with `web` instead of `endpoint` loads an HTML file from the plugin folder in an isolated frame. Before it first runs, the user approves it; the approval is tied to the plugin version, so an update asks again.

The frame is sandboxed (`allow-scripts` only): it has its own opaque origin, no access to the desktop bridge, files, or the rest of the app, and its Content-Security-Policy blocks all network access. Load scripts, styles, and images from your plugin folder with relative paths. All data comes through zorai with `postMessage`, for example from a `<script type="module">`:

```js
let next = 0;
const pending = new Map();
function zorai(method, params) {
  const id = ++next;
  parent.postMessage({ zorai: "plugin-panel", id, method, params }, "*");
  return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
}
addEventListener("message", (event) => {
  const message = event.data;
  if (message?.zorai !== "plugin-panel") return;
  if (message.event === "theme") return applyTheme(message.theme);
  const request = pending.get(message.id);
  if (!request) return;
  pending.delete(message.id);
  message.ok ? request.resolve(message.result) : request.reject(new Error(message.error));
});

const issues = await zorai("call", { endpoint: "list_issues", params: { owner: "me", repo: "app" } });
```

| Method | Params | Needs permission |
| --- | --- | --- |
| `call` | `{ endpoint, params }` — calls one of this plugin's endpoints; at most 4 at once | none |
| `theme` | — returns the current colours and fonts (`--bg-primary`, `--text-primary`, `--accent`, `--font-ui`, ...) and `appearance` | none |
| `insertText` | `{ text }` — appends to the message box (max 20,000 characters) | `composer` |
| `ask` | `{ text }` — opens the plugin's Ask tab with a new conversation and this text in its message box, for the user to finish and send (falls back to the Threads message box when the plugin has no prompt command) | `composer` |
| `toast` | `{ text }` — short notification (max 280 characters) | `toast` |
| `pickFolder` | `{ title }` — opens a folder picker; returns the chosen path, or `null` if cancelled | `files` |
| `terminal` | `{ title, cwd?, command }` — opens a titled terminal tab docked under the panel, starting in the absolute folder `cwd`, and runs `command` once; use it for long runs the user should watch (for example `tail -f` of a log) or interactive tools | `terminal` |
| `mediaOptions` | `{ kind }` with `tts`, `stt`, `image`, `video` or `vision` — returns `{ current: { provider, model, voice? }, providers: [{ id, label, ready, models: [{ id, name }] }] }`: the providers in zorai's settings that can do that, whether each has credentials, and known models, so a panel can let the user pick a cloud route | none |

zorai also sends `{ zorai: "plugin-panel", event: "theme", theme }` when the panel loads and whenever the theme changes, so panels can match the app.

### Local endpoints

An endpoint with `run` instead of `method` and `path` runs a program on the user's computer, for example to render audio or run a model. Panels, `ui.commands` and slash commands with an `action` call it exactly like an HTTP endpoint.

```json
{
  "python": { "env": true, "dependencies": ["soundfile>=0.12"] },
  "api": {
    "endpoints": {
      "render": { "run": { "command": "python scripts/voice.py render", "timeout_secs": 900 } }
    }
  }
}
```

- `run` takes the same fields as a `python` command (`command`, `run_path`, `source`, `env`, `dependencies`) and inherits the top-level `python` block, so `env: true` gives the plugin its own `uv` environment.
- The program reads `{ "params": {...}, "settings": {...} }` as JSON on stdin. Parameters never reach the command line, so they cannot inject shell commands.
- Whatever it prints to stdout is the result; keep logs on stderr. Output from installing dependencies is kept out of the result. A non-zero exit returns the last part of stderr as the error.
- It runs in its `run_path` (the plugin folder by default) with `ZORAI_PLUGIN_NAME`, `ZORAI_PLUGIN_DIR` and `ZORAI_PLUGIN_DATA_DIR` set. Write outputs to `ZORAI_PLUGIN_DATA_DIR` (`~/.zorai/plugin-data/<plugin>/`).
- `timeout_secs` defaults to 120 and is capped at 3600. On timeout zorai stops the program and everything it started.
- The first call asks the user to allow the plugin to run programs; the approval covers one plugin version and can be revoked in Settings, Plugins. Until then calls fail with `local_run_not_approved`.
- Programs are not sandboxed: they run with the user's permissions.
- Calls to the same endpoint run one at a time; different endpoints run side by side. For work that takes minutes, use quick `start`, `status` and `cancel` endpoints and let `start` launch the work in the background, so panels can show progress.

## Plugin Interface

The runtime plugin shape is:

```ts
export interface Plugin {
  id: string;
  name: string;
  version: string;
  components?: Record<string, React.ComponentType<any>>;
  commands?: Record<string, CommandAction>;
  assistantTools?: PluginAssistantToolDefinition[];
  assistantToolExecutors?: Record<string, PluginAssistantToolExecutor>;
  onLoad?: () => void;
  onUnload?: () => void;
}
```

Important behavior:

- component names are registered as `${plugin.id}:${name}`,
- command ids are registered as `${plugin.id}:${name}`,
- assistant tools are merged into the agent tool list at runtime.

## package.json Contract For npm Plugins

Externally installed packages should expose a `zoraiPlugin` field in `package.json`.

Minimal example:

```json
{
  "name": "zorai-plugin-example",
  "version": "1.0.44",
  "zoraiPlugin": {
    "entry": "dist/zorai-plugin.js",
    "format": "script"
  }
}
```

Current rules:

- `entry` is required.
- only `format: "script"` is currently supported.
- the entry file must be self-contained and executable in the renderer without further bundling.
- plugin installers are run with `npm install --ignore-scripts`, so published packages must already contain their built entry assets.
- the script should register itself through `window.ZoraiApi.registerPlugin(...)`.

Installed package metadata is recorded under `~/.zorai/plugins/registry.json`, and Electron preload loads those entries on app startup.

## Minimal Plugin Example

```ts
import type { Plugin } from "../PluginManager";
import ExamplePanel from "./ExamplePanel";

export const examplePlugin: Plugin = {
  id: "example",
  name: "Example Plugin",
  version: "1.0.44",
  components: {
    ExamplePanel,
  },
  commands: {
    sayHello: () => {
      console.info("hello from example plugin");
    },
  },
  views: {
    example: {
      schemaVersion: 1,
      title: "Example Plugin View",
      layout: {
        id: "example-root",
        type: "example:ExamplePanel",
      },
    },
  },
  onLoad: () => {
    console.info("example plugin loaded");
  },
};

export function registerExamplePlugin(): void {
  if (typeof window === "undefined" || !window.ZoraiApi) {
    return;
  }

  window.ZoraiApi.registerPlugin(examplePlugin);
}
```

For an externally installed plugin, bundle that registration path into the script referenced by `zoraiPlugin.entry` so the script self-registers when loaded.

## Registering The Plugin

The plugin must be registered during app startup.

Pattern:

1. Create a `registerMyPlugin()` function in your plugin module.
2. Guard against duplicate registration.
3. Call `window.ZoraiApi.registerPlugin(...)`.
4. Import and invoke the registration function from app initialization.

The coding-agents implementation in [frontend/src/plugins/coding-agents/registerPlugin.ts](../frontend/src/plugins/coding-agents/registerPlugin.ts) is the best current reference.

## Plugin Components

Use `components` when YAML or other runtime code needs to render a custom React component.

Notes:

- The plugin manager namespaces each component as `${plugin.id}:${name}`.
- In plugin YAML, reference the namespaced type string.
- Keep the component self-contained and avoid hidden cross-module side effects.

For npm-installed plugins, component code must be bundled into the external script in a way that can execute directly in the renderer.

Example YAML node:

```yaml
layout:
  id: "example-root"
  type: "example:ExamplePanel"
```

## Plugin Commands

Use `commands` for explicit UI actions that can be triggered by buttons, command palette entries, or YAML node commands.

Command ids are also namespaced as `${plugin.id}:${name}`.

Example:

```yaml
- id: "refresh-button"
  type: "Button"
  command: "example:refresh"
  props:
    label: "Refresh"
```

For runtime-installed `plugin.json` plugins, commands can now be backed by either:

- `action`: route the slash command to a declared plugin API endpoint.
- `python`: describe a Python command that the agent should execute through the shell tools.

The `python` block is available both at the top level for shared defaults and inside individual commands for the actual runnable command:

```json
{
  "python": {
    "run_path": "workspace",
    "source": "https://example.com/tool.py",
    "env": true,
    "dependencies": ["requests>=2.32"]
  },
  "commands": {
    "sync": {
      "description": "Run sync",
      "python": {
        "command": "python sync.py --full"
      }
    }
  }
}
```

Python manifest rules:

- `commands.<name>.python.command` is required.
- `run_path`, `source`, `env`, and `dependencies` can be inherited from the top-level `python` block.
- `source` must be an `http(s)` URL or an absolute path.
- `env` can be either a string path to an activation script or a boolean.
- `env: true` tells Zorai to prefer `uv` for `.venv` creation and fall back to `python -m venv`.

## Assistant Tools In Plugins

Plugins can extend the assistant tool list with OpenAI-style function tools.

Use:

- `assistantTools` to declare schemas.
- `assistantToolExecutors` to execute them.

Each executor returns a tool result payload that the assistant runtime feeds back into the next model round.

This is the right extension point when your plugin should be directly invocable by the built-in assistant.

The coding-agents plugin uses this path to expose discovery and launch operations.

The built-in coding-agents plugin now also demonstrates a richer runtime-profile pattern for external tools such as Hermes, pi.dev, and OpenClaw: discovery is not limited to PATH presence, and can include config-path checks, setup guidance, launch modes, and local runtime health checks surfaced through Electron IPC.

The new built-in `ai-training` plugin follows the same renderer/preload/main split, but uses a separate domain model because training integrations are not all plain CLIs. Prime Intellect Verifiers behaves like a training runtime, while AutoResearch and AutoRL are repository-bound workflows that require workspace path and file-shape checks in addition to global system prerequisites.

## Electron Bridges For System-Level Plugins

If a plugin needs filesystem access, PATH discovery, process spawning, or other host capabilities, do not reach straight from React into Node APIs.

Use the Electron bridge layers:

- [frontend/electron/preload.cjs](../frontend/electron/preload.cjs)
- [frontend/electron/main.cjs](../frontend/electron/main.cjs)

Recommended split:

- renderer plugin code owns UI and state,
- preload exposes a small safe bridge,
- main process performs privileged operations.

Use `npm run dev:electron` for this category of plugin work.

## Suggested Plugin Layout

One practical layout is:

```text
frontend/src/plugins/my-plugin/
  registerPlugin.ts
  types.ts
  store.ts
  bridge.ts
  MyPluginView.tsx
```

This keeps the registration surface small while leaving room for feature-specific state and Electron bridge logic.

## Build And Validation

Use the smallest validation loop that matches the plugin surface area:

- UI-only plugin: `cd frontend && npm run build`
- plugin with Electron bridge work: `cd frontend && npm run build` and then `cd frontend && npm run dev:electron`
- lint pass if you touched broader frontend code: `cd frontend && npm run lint`

For an npm-distributed plugin package, also validate:

- `npm pack` produces the expected bundle,
- the published package contains the file referenced by `zoraiPlugin.entry`,
- `zorai install plugin <path-or-package>` records the plugin in `~/.zorai/plugins/registry.json`,
- Electron startup loads the script without console errors.

## Common Failure Modes

- Plugin registers twice: guard registration with a module-level `registered` flag.
- Electron feature silently fails in browser preview: the bridge is only available in Electron.
- Assistant tool loops or repeats: ensure the executor returns a proper tool result and verify the tool name matches the declared schema.

## Design Guidance

- Keep plugin ids short and stable because they become part of public component and command names.
- Prefer explicit bridges over hidden globals for privileged behavior.
- Avoid coupling plugin code directly to daemon internals unless the feature truly needs a new backend capability.
