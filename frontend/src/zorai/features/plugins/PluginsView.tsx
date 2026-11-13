import { useEffect, useMemo, useRef, useState } from "react";
import { MarkdownContent } from "@/components/agent-chat-panel/chat-view/markdown";
import { pushToast } from "@/lib/toastStore";
import { useComposerDraftStore } from "@/zorai/features/threads/composerDraftStore";
import { getBridge } from "@/lib/bridge";
import { answerLocalRunRequest, appendToComposer, ASK_PANEL_ID, callPluginEndpoint, commandParams, executePluginCommand, fillPromptTemplate, refreshPluginUi, runPluginCommand, usePluginUiStore } from "./pluginUiStore";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import { useAgentStore } from "@/lib/agentStore";
import { ThreadsView } from "@/zorai/features/threads/ThreadsView";
import { ThreadComposer } from "@/zorai/features/threads/ThreadComposer";
import { matchesPluginSearch, sortPluginsByFavorite, usePluginPrefsStore } from "./pluginPrefsStore";
import { PluginIcon } from "./PluginIcon";
import { pluginUiSummary } from "./pluginUiSummary";
import { mediaOptions, type MediaKind } from "./pluginMediaOptions";
import { usePluginStore } from "@/lib/pluginStore";

export { PluginIcon };
import { handlePanelMessage, panelApprovalKey, panelThemeSnapshot, readPanelApprovals, writePanelApproval } from "./pluginPanelHost";
import { ComposerTerminalPanel } from "../terminal/ComposerTerminalPanel";
import { openThreadShell, setThreadTerminalOpen, useThreadTerminalStore } from "../terminal/threadTerminalStore";
import { Presence } from "@/components/Presence";

type MarkdownPanel = Extract<ZoraiPluginUiPanel, { kind: "markdown" }>;
type WebPanel = Extract<ZoraiPluginUiPanel, { kind: "web" }>;

const PANEL_INPUTS_KEY = "zorai-plugin-panel-inputs";

function readStoredInputs(key: string): Record<string, string> {
  try {
    return JSON.parse(localStorage.getItem(PANEL_INPUTS_KEY) ?? "{}")[key] ?? {};
  } catch {
    return {};
  }
}

function storeInputs(key: string, values: Record<string, string>) {
  try {
    const all = JSON.parse(localStorage.getItem(PANEL_INPUTS_KEY) ?? "{}");
    localStorage.setItem(PANEL_INPUTS_KEY, JSON.stringify({ ...all, [key]: values }));
  } catch {
    localStorage.setItem(PANEL_INPUTS_KEY, JSON.stringify({ [key]: values }));
  }
}

function InputFields({ inputs, values, onChange }: { inputs: ZoraiPluginUiInput[]; values: Record<string, string>; onChange: (values: Record<string, string>) => void }) {
  return (
    <>
      {inputs.map((input) => (
        <label key={input.key} className="zorai-plugin-field">
          <span>{input.label}</span>
          <input className="zorai-input" value={values[input.key] ?? input.default} placeholder={input.placeholder} onChange={(event) => onChange({ ...values, [input.key]: event.target.value })} />
        </label>
      ))}
    </>
  );
}

function selectPlugin(plugin: string | null, panel: string | null = null) {
  usePluginUiStore.setState({ selected: plugin ? { plugin, panel } : null });
}

export function openPinnedPlugin(name: string) {
  usePluginUiStore.setState({ focusPlugin: name, selected: { plugin: name, panel: null } });
}

function useSortedPlugins() {
  const plugins = usePluginUiStore((state) => state.plugins);
  const favorites = usePluginPrefsStore((state) => state.favorites);
  return useMemo(() => sortPluginsByFavorite(plugins, favorites), [plugins, favorites]);
}

export function PluginsRail() {
  const focusName = usePluginUiStore((state) => state.focusPlugin);
  const selected = usePluginUiStore((state) => state.selected);
  const query = usePluginUiStore((state) => state.query);
  const favorites = usePluginPrefsStore((state) => state.favorites);
  const sorted = useSortedPlugins();
  useEffect(() => { void refreshPluginUi(); }, []);
  const focus = sorted.find((plugin) => plugin.name === focusName);

  if (focus) {
    const hasAsk = focus.commands.some((command) => command.prompt);
    const activePanel = selected?.panel ?? focus.panels[0]?.id ?? (hasAsk ? ASK_PANEL_ID : null);
    return (
      <nav className="zorai-rail-stack" aria-label={`${focus.name} plugin`}>
        <div className="zorai-plugin-rail-focus">
          <PluginIcon plugin={focus} size={32} />
          <strong>{focus.name}</strong>
        </div>
        {focus.panels.length || hasAsk ? <div className="zorai-section-label">Panels</div> : null}
        {focus.panels.map((panel) => (
          <button key={panel.id} type="button" className={["zorai-plugin-rail-item", activePanel === panel.id ? "is-active" : ""].filter(Boolean).join(" ")} onClick={() => selectPlugin(focus.name, panel.id)}>
            {panel.title}
          </button>
        ))}
        {hasAsk ? (
          <button type="button" className={["zorai-plugin-rail-item", activePanel === ASK_PANEL_ID ? "is-active" : ""].filter(Boolean).join(" ")} onClick={() => selectPlugin(focus.name, ASK_PANEL_ID)}>Ask</button>
        ) : null}
        {focus.commands.length ? <div className="zorai-section-label">Commands</div> : null}
        {focus.commands.map((command) => (
          <button key={command.id} type="button" className="zorai-plugin-rail-item" onClick={() => runPluginCommand(focus, command)}>{command.title}</button>
        ))}
        <button type="button" className="zorai-plugin-rail-item zorai-plugin-rail-item--muted" onClick={() => usePluginUiStore.setState({ focusPlugin: null, selected: null })}>Show all plugins</button>
      </nav>
    );
  }

  const visible = sorted.filter((plugin) => matchesPluginSearch(plugin, query));
  return (
    <nav className="zorai-rail-stack" aria-label="Plugins">
      <input className="zorai-input zorai-plugin-search" value={query} placeholder="Search plugins" aria-label="Search plugins" onChange={(event) => usePluginUiStore.setState({ query: event.target.value })} />
      <button type="button" className={["zorai-plugin-rail-item", !selected ? "is-active" : ""].filter(Boolean).join(" ")} onClick={() => selectPlugin(null)}>All plugins</button>
      {visible.length === 0 ? <div className="zorai-empty">{sorted.length ? "No plugins match." : "No plugins with panels or commands yet."}</div> : null}
      {visible.map((plugin) => (
        <button key={plugin.name} type="button" className={["zorai-plugin-rail-item", selected?.plugin === plugin.name ? "is-active" : ""].filter(Boolean).join(" ")} onClick={() => selectPlugin(plugin.name)}>
          <PluginIcon plugin={plugin} size={20} />
          <span className="zorai-plugin-rail-item__name">{plugin.name}</span>
          {favorites.includes(plugin.name) ? <span className="zorai-plugin-star" aria-label="Favourite">★</span> : null}
        </button>
      ))}
    </nav>
  );
}

export function PluginsView() {
  const loaded = usePluginUiStore((state) => state.loaded);
  const selected = usePluginUiStore((state) => state.selected);
  const sorted = useSortedPlugins();
  const plugin = sorted.find((entry) => entry.name === selected?.plugin);
  if (!loaded) return <div className="zorai-empty-main"><p>Loading plugins…</p></div>;
  if (plugin) return <PluginHome key={plugin.name} plugin={plugin} panelId={selected?.panel ?? null} />;
  return <PluginsOverview />;
}

function PluginToggles({ plugin }: { plugin: ZoraiPluginUiManifest }) {
  const favorite = usePluginPrefsStore((state) => state.favorites.includes(plugin.name));
  const pinned = usePluginPrefsStore((state) => state.pinned.includes(plugin.name));
  const { toggleFavorite, togglePinned } = usePluginPrefsStore.getState();
  return (
    <div className="zorai-plugin-toggles">
      <button type="button" className={["zorai-plugin-toggle", favorite ? "is-on" : ""].filter(Boolean).join(" ")} aria-pressed={favorite} title={favorite ? "Remove from favourites" : "Add to favourites; favourites are listed first"} onClick={(event) => { event.stopPropagation(); toggleFavorite(plugin.name); }}>{favorite ? "★" : "☆"}<span>Favourite</span></button>
      <button type="button" className={["zorai-plugin-toggle", pinned ? "is-on" : ""].filter(Boolean).join(" ")} aria-pressed={pinned} title={pinned ? "Remove from the sidebar" : "Pin to the sidebar for one-click access"} onClick={(event) => { event.stopPropagation(); togglePinned(plugin.name); }}><svg className="zorai-plugin-pin" viewBox="0 0 16 16" aria-hidden="true"><path d="M6 2h4l-.6 4.2L12 9H4l2.6-2.8L6 2Z" fill={pinned ? "currentColor" : "none"} /><path d="M8 9v5" /></svg><span>{pinned ? "Pinned" : "Pin"}</span></button>
    </div>
  );
}

async function turnOnPlugin(name: string) {
  try {
    await usePluginStore.getState().toggleEnabled(name, true);
    await refreshPluginUi();
    pushToast(`${name} is on. Its views are in Plugins.`, "info");
  } catch (error) {
    pushToast(`Couldn't turn on ${name}: ${error instanceof Error ? error.message : String(error)}`, "error");
  }
}

function PluginsOverview() {
  const sorted = useSortedPlugins();
  const query = usePluginUiStore((state) => state.query);
  const disabled = usePluginUiStore((state) => state.disabled);
  const viewMode = usePluginPrefsStore((state) => state.viewMode);
  const setViewMode = usePluginPrefsStore((state) => state.setViewMode);
  const visible = sorted.filter((plugin) => matchesPluginSearch(plugin, query));
  const available = disabled.filter((plugin) => matchesPluginSearch(plugin, query));
  const problems = sorted.flatMap((entry) => entry.problems.map((problem) => `${entry.name}: ${problem}`));

  return (
    <section className="zorai-feature-surface zorai-plugins-surface">
      <div className="zorai-plugins-toolbar">
        <h1>Plugins</h1>
        <input className="zorai-input zorai-plugin-search" value={query} placeholder="Search plugins, panels and commands" aria-label="Search plugins" onChange={(event) => usePluginUiStore.setState({ query: event.target.value })} />
        <div className="zorai-plugin-view-toggle" role="group" aria-label="Layout">
          <button type="button" className={viewMode === "grid" ? "is-on" : undefined} aria-pressed={viewMode === "grid"} onClick={() => setViewMode("grid")}>Grid</button>
          <button type="button" className={viewMode === "list" ? "is-on" : undefined} aria-pressed={viewMode === "list"} onClick={() => setViewMode("list")}>List</button>
        </div>
      </div>
      {problems.length ? <div className="zorai-theme-errors" role="alert">{problems.map((problem) => <span key={problem}>{problem}</span>)}</div> : null}
      {sorted.length === 0 ? (
        <div className="zorai-empty-main">
          <h2>No plugin panels or commands yet</h2>
          <p>Plugins add them by declaring a <code>ui</code> section in their plugin.json. Install or turn one on and it appears here.</p>
        </div>
      ) : visible.length === 0 ? <p className="zorai-shortcuts-note">No plugins match “{query}”.</p> : (
        <div className={viewMode === "grid" ? "zorai-plugin-grid" : "zorai-plugin-list"}>
          {visible.map((plugin) => (
            <div key={plugin.name} role="button" tabIndex={0} className="zorai-plugin-card" onClick={() => selectPlugin(plugin.name)} onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); selectPlugin(plugin.name); } }}>
              <PluginIcon plugin={plugin} size={viewMode === "grid" ? 44 : 32} />
              <div className="zorai-plugin-card__text">
                <strong>{plugin.name}</strong>
                {plugin.description ? <span className="zorai-plugin-card__description">{plugin.description}</span> : null}
                <span className="zorai-plugin-card__meta">{[pluginUiSummary(plugin), `v${plugin.version}`].filter(Boolean).join(" · ")}</span>
              </div>
              <PluginToggles plugin={plugin} />
            </div>
          ))}
        </div>
      )}
      {available.length ? (
        <section className="zorai-plugin-available" aria-label="Plugins with views that are switched off">
          <div className="zorai-section-label">Switched off · {available.length} more with their own views</div>
          <div className={viewMode === "grid" ? "zorai-plugin-grid" : "zorai-plugin-list"}>
            {available.map((plugin) => (
              <div key={plugin.name} className="zorai-plugin-card is-off">
                <PluginIcon plugin={plugin} size={viewMode === "grid" ? 44 : 32} />
                <div className="zorai-plugin-card__text">
                  <strong>{plugin.name}</strong>
                  {plugin.description ? <span className="zorai-plugin-card__description">{plugin.description}</span> : null}
                  <span className="zorai-plugin-card__meta">{[pluginUiSummary(plugin), `v${plugin.version}`].filter(Boolean).join(" · ")}</span>
                </div>
                <button type="button" className="zorai-ghost-button" onClick={() => void turnOnPlugin(plugin.name)}>Turn on</button>
              </div>
            ))}
          </div>
        </section>
      ) : null}
    </section>
  );
}

function PluginConversation({ plugin, askCommand }: { plugin: ZoraiPluginUiManifest; askCommand: ZoraiPluginUiCommand }) {
  const runtime = useAgentChatPanelRuntime();
  const threads = useAgentStore((state) => state.threads);
  const currentId = usePluginPrefsStore((state) => state.currentConversation[plugin.name] ?? null);
  const history = usePluginPrefsStore((state) => state.conversations[plugin.name]);
  const askRequest = usePluginUiStore((state) => state.askRequest);
  const pendingSend = useRef<{ threadId: string; payload: { text: string; contentBlocksJson?: string | null } } | null>(null);
  const thread = currentId ? threads.find((entry) => entry.id === currentId || entry.daemonThreadId === currentId) ?? null : null;
  const known = useMemo(() => new Set(history ?? []), [history]);

  const start = (payload: { text: string; contentBlocksJson?: string | null }, title: string) => {
    const message = payload.text.trim();
    if (!message && !payload.contentBlocksJson) return;
    const { recordConversation, setCurrentConversation } = usePluginPrefsStore.getState();
    const id = runtime.createThread({
      workspaceId: runtime.activeWorkspace?.id ?? null,
      agentId: null,
      agentName: null,
      title: `${plugin.name}: ${(title.trim() || "attachment").slice(0, 60)}`,
    });
    pendingSend.current = { threadId: id, payload: { ...payload, text: message } };
    recordConversation(plugin.name, id);
    setCurrentConversation(plugin.name, id);
    runtime.openThread(id);
  };

  useEffect(() => {
    const pending = pendingSend.current;
    if (!pending || runtime.activeThread?.id !== pending.threadId) return;
    pendingSend.current = null;
    runtime.sendMessage(pending.payload);
  }, [runtime.activeThread?.id, runtime.sendMessage]);

  useEffect(() => {
    if (askRequest?.plugin !== plugin.name) return;
    usePluginUiStore.setState({ askRequest: null });
    start({ text: askRequest.text }, askRequest.title);
  }, [askRequest]);

  useEffect(() => {
    if (thread && runtime.activeThread?.id !== thread.id) runtime.openThread(thread.id);
  }, [thread?.id]);

  useEffect(() => {
    if (thread?.daemonThreadId) usePluginPrefsStore.getState().recordConversation(plugin.name, thread.daemonThreadId);
  }, [thread?.daemonThreadId]);

  if (!thread) {
    const input = askCommand.inputs[0];
    return (
      <div className="zorai-plugin-ask">
        <ThreadComposer
          placeholder={input?.placeholder ? `e.g. ${input.placeholder}` : `Ask ${plugin.name}…`}
          onSendNewThread={(payload) => start({ ...payload, text: fillPromptTemplate(askCommand.prompt ?? "", input ? { [input.key]: payload.text } : {}) || payload.text }, payload.text)}
        />
        <span className="zorai-shortcuts-note">The agent answers here using {plugin.name}. Conversations also appear in Threads.</span>
      </div>
    );
  }

  return (
    <div className="zorai-plugin-chat">
      <ThreadsView
        variant="compact"
        compactHeaderActions={<button type="button" className="zorai-ghost-button zorai-plugin-new-chat" onClick={() => usePluginPrefsStore.getState().setCurrentConversation(plugin.name, null)}>New conversation</button>}
        sessionTabFilter={(id, entry) => known.has(id) || Boolean(entry && (known.has(entry.id) || (entry.daemonThreadId && known.has(entry.daemonThreadId))))} />
    </div>
  );
}

function PluginHome({ plugin, panelId }: { plugin: ZoraiPluginUiManifest; panelId: string | null }) {
  const focused = usePluginUiStore((state) => state.focusPlugin === plugin.name);
  const askCommand = plugin.commands.find((command) => command.prompt);
  const showAsk = Boolean(askCommand) && (panelId === ASK_PANEL_ID || plugin.panels.length === 0);
  const panel = showAsk ? undefined : plugin.panels.find((entry) => entry.id === panelId) ?? plugin.panels[0];
  return (
    <section className="zorai-feature-surface zorai-plugins-surface">
      <div className="zorai-plugin-home__header">
        <PluginIcon plugin={plugin} size={44} />
        <div className="zorai-plugin-home__title">
          {focused ? null : <button type="button" className="zorai-ghost-button zorai-plugin-back" onClick={() => selectPlugin(null)}>← All plugins</button>}
          <h1>{plugin.name}</h1>
          <span className="zorai-plugin-card__meta">{[plugin.description, `v${plugin.version}`].filter(Boolean).join(" · ")}</span>
        </div>
        <PluginToggles plugin={plugin} />
      </div>
      {plugin.problems.length ? <div className="zorai-theme-errors" role="alert">{plugin.problems.map((problem) => <span key={problem}>{problem}</span>)}</div> : null}
      {plugin.commands.length ? (
        <div className="zorai-plugin-home__commands" aria-label="Commands">
          {plugin.commands.map((command) => <button key={command.id} type="button" className="zorai-ghost-button" onClick={() => runPluginCommand(plugin, command)}>{command.title}</button>)}
        </div>
      ) : null}
      {plugin.panels.length + (askCommand ? 1 : 0) > 1 ? (
        <div className="zorai-plugin-tabs" role="tablist">
          {plugin.panels.map((entry) => (
            <button key={entry.id} type="button" role="tab" aria-selected={entry.id === panel?.id} className={entry.id === panel?.id ? "is-active" : undefined} onClick={() => selectPlugin(plugin.name, entry.id)}>{entry.title}</button>
          ))}
          {askCommand ? <button type="button" role="tab" aria-selected={showAsk} className={showAsk ? "is-active" : undefined} onClick={() => selectPlugin(plugin.name, ASK_PANEL_ID)}>Ask</button> : null}
        </div>
      ) : null}
      {showAsk && askCommand
        ? <PluginConversation plugin={plugin} askCommand={askCommand} />
        : panel
          ? panel.kind === "web"
            ? <PluginWebPanel key={`${plugin.name}/${panel.id}`} plugin={plugin} panel={panel} />
            : <PluginMarkdownPanel key={`${plugin.name}/${panel.id}`} plugin={plugin} panel={panel} />
          : <p className="zorai-shortcuts-note">This plugin adds commands only. Run them with the buttons above or from the command palette.</p>}
    </section>
  );
}

function PluginMarkdownPanel({ plugin, panel }: { plugin: ZoraiPluginUiManifest; panel: MarkdownPanel }) {
  const storageKey = `${plugin.name}/${panel.id}`;
  const [values, setValues] = useState<Record<string, string>>(() => readStoredInputs(storageKey));
  const [state, setState] = useState<{ status: "idle" | "loading" | "done" | "error"; text: string }>({ status: "idle", text: "" });
  const ready = panel.inputs.every((input) => (values[input.key] ?? input.default).trim() !== "");

  const run = async () => {
    storeInputs(storageKey, values);
    setState({ status: "loading", text: "" });
    try {
      const defaults = Object.fromEntries(panel.inputs.map((input) => [input.key, values[input.key] ?? input.default]));
      setState({ status: "done", text: await callPluginEndpoint(plugin.name, panel.endpoint, commandParams(panel, defaults)) });
    } catch (error) {
      setState({ status: "error", text: error instanceof Error ? error.message : String(error) });
    }
  };

  const autoRan = useRef(false);
  useEffect(() => {
    if (autoRan.current || !ready) return;
    autoRan.current = true;
    void run();
  }, []);

  return (
    <div className="zorai-plugin-panel">
      <form className="zorai-plugin-panel__controls" onSubmit={(event) => { event.preventDefault(); void run(); }}>
        <InputFields inputs={panel.inputs} values={values} onChange={setValues} />
        <button type="submit" className="zorai-ghost-button" disabled={!ready || state.status === "loading"}>{state.status === "loading" ? "Loading…" : state.status === "idle" ? "Run" : "Refresh"}</button>
      </form>
      {state.status === "error" ? <p className="zorai-theme-errors" role="alert">{state.text}</p> : null}
      {state.status === "done" ? <div className="zorai-plugin-panel__result"><MarkdownContent content={state.text} /></div> : null}
      {state.status === "idle" && !ready ? <p className="zorai-shortcuts-note">Fill in the fields above, then Run.</p> : null}
    </div>
  );
}

function PluginWebPanel({ plugin, panel }: { plugin: ZoraiPluginUiManifest; panel: WebPanel }) {
  const approvalKey = panelApprovalKey(plugin, panel);
  const [approved, setApproved] = useState(() => readPanelApprovals()[approvalKey] === panel.permissions.join(","));
  const frameRef = useRef<HTMLIFrameElement | null>(null);
  const inFlight = useRef(0);
  const terminalKey = `plugin:${plugin.name}`;
  const terminalOpen = useThreadTerminalStore((state) => Boolean(state.open[terminalKey]));

  useEffect(() => {
    if (!approved) return;
    const post = (message: unknown) => frameRef.current?.contentWindow?.postMessage(message, "*");
    const context = {
      pluginName: plugin.name,
      permissions: panel.permissions,
      writeEndpoints: plugin.writeEndpoints ?? [],
      call: async (endpoint: string, params: Record<string, unknown>) => {
        if (inFlight.current >= 4) throw new Error("Too many calls at once. Wait for earlier calls to finish.");
        inFlight.current += 1;
        try {
          return await callPluginEndpoint(plugin.name, endpoint, params);
        } finally {
          inFlight.current -= 1;
        }
      },
      insertText: (text: string) => {
        appendToComposer(text);
        pushToast(`${plugin.name}: added to the message box.`, "info");
      },
      ask: (text: string) => {
        if (!plugin.commands.some((command) => command.prompt)) {
          appendToComposer(text);
          return;
        }
        useComposerDraftStore.getState().setInput(text);
        usePluginPrefsStore.getState().setCurrentConversation(plugin.name, null);
        selectPlugin(plugin.name, ASK_PANEL_ID);
      },
      toast: (text: string) => pushToast(`${plugin.name}: ${text}`, "info"),
      pickFolder: async (title: string) => (await getBridge()?.pluginPickFolder?.(title)) ?? null,
      mediaOptions: (kind: MediaKind) => {
        const state = useAgentStore.getState();
        return mediaOptions(kind, state.agentSettings as unknown as Record<string, unknown>, state.providerAuthStates);
      },
      openTerminal: ({ title, cwd, command }: { title: string; cwd?: string; command: string }) => {
        openThreadShell(terminalKey, cwd, { title, command });
        setThreadTerminalOpen(terminalKey, true);
      },
      theme: panelThemeSnapshot,
    };
    const onMessage = (event: MessageEvent) => {
      if (!frameRef.current || event.source !== frameRef.current.contentWindow) return;
      void handlePanelMessage(event.data, context).then((reply) => { if (reply) post(reply); });
    };
    let themeTimer: number | undefined;
    const observer = new MutationObserver(() => {
      window.clearTimeout(themeTimer);
      themeTimer = window.setTimeout(() => post({ zorai: "plugin-panel", event: "theme", theme: panelThemeSnapshot() }), 150);
    });
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["style", "data-theme", "data-appearance"] });
    window.addEventListener("message", onMessage);
    return () => {
      window.removeEventListener("message", onMessage);
      observer.disconnect();
      window.clearTimeout(themeTimer);
    };
  }, [approved, plugin.name, panel.permissions, terminalKey]);

  if (!approved) {
    return (
      <div className="zorai-remote-theme__preview zorai-plugin-approval" role="group" aria-label={`Allow ${plugin.name} to run ${panel.title}`}>
        <strong>Run {plugin.name}'s “{panel.title}” panel?</strong>
        <span>This panel runs the plugin's own code in an isolated frame. It cannot read your files, reach the internet directly, or use zorai's other features.</span>
        <ul>
          <li>It can call {plugin.name}'s own endpoints, using the settings you gave this plugin.</li>
          {panel.permissions.includes("composer") ? <li>It can put text into the message box.</li> : null}
          {panel.permissions.includes("toast") ? <li>It can show short notifications.</li> : null}
          {panel.permissions.includes("files") ? <li>It can ask you to pick a folder, and learns the folder you pick.</li> : null}
          {panel.permissions.includes("write") ? <li>It can make changes in {plugin.name} when you use its buttons, such as sending, deleting or editing.</li> : null}
          {panel.permissions.includes("terminal") ? <li>It can open terminal tabs below the panel and start commands in them, with your user's permissions.</li> : null}
        </ul>
        <div className="zorai-panel__heading-actions">
          <button type="button" className="zorai-primary-button" onClick={() => { writePanelApproval(approvalKey, panel.permissions); setApproved(true); }}>Allow and open</button>
        </div>
        <span className="zorai-shortcuts-note">You'll be asked again if the plugin is updated or asks for different permissions.</span>
      </div>
    );
  }

  return (
    <div className="zorai-plugin-frame-stack">
      <iframe
        ref={frameRef}
        className="zorai-plugin-frame"
        title={`${plugin.name}: ${panel.title}`}
        src={panel.url}
        sandbox="allow-scripts"
        referrerPolicy="no-referrer"
        onLoad={() => frameRef.current?.contentWindow?.postMessage({ zorai: "plugin-panel", event: "theme", theme: panelThemeSnapshot() }, "*")}
      />
      {terminalOpen ? <ComposerTerminalPanel sessionKey={terminalKey} /> : null}
    </div>
  );
}

type PluginUiState = ReturnType<typeof usePluginUiStore.getState>;

export function PluginCommandHost() {
  const pending = usePluginUiStore((state) => state.pendingCommand);
  const result = usePluginUiStore((state) => state.commandResult);
  const localRun = usePluginUiStore((state) => state.localRunRequest);
  return (
    <Presence value={localRun || pending || result ? { localRun, pending, result } : null}>
      {(shown) => <PluginCommandDialogs {...shown} />}
    </Presence>
  );
}

function PluginCommandDialogs({ pending, result, localRun }: { pending: PluginUiState["pendingCommand"]; result: PluginUiState["commandResult"]; localRun: PluginUiState["localRunRequest"] }) {
  const [values, setValues] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const commandKey = pending ? `${pending.plugin.name}/${pending.command.id}` : "";
  useEffect(() => { if (commandKey) setValues(readStoredInputs(`command:${commandKey}`)); }, [commandKey]);
  const ready = useMemo(() => pending?.command.inputs.every((input) => (values[input.key] ?? input.default).trim() !== "") ?? false, [pending, values]);
  const close = () => usePluginUiStore.setState({ pendingCommand: null, commandResult: null });

  if (localRun) {
    return (
      <div className="zorai-code-dialog-backdrop" role="presentation">
        <div className="zorai-code-close-dialog" role="alertdialog" aria-modal="true" aria-label={`Allow ${localRun.plugin} to run programs`}>
          <h2>Allow {localRun.plugin} to run programs on this computer?</h2>
          <p>{localRun.plugin} {localRun.version} wants to run its own scripts here, for example to render audio or run a model. They run with your user's permissions and are not sandboxed, so only allow plugins you trust.</p>
          <p>This covers version {localRun.version} only. You can take it back in Settings, Plugins.</p>
          <footer>
            <button type="button" onClick={() => answerLocalRunRequest(false)}>Don't allow</button>
            <button type="button" className="is-primary" onClick={() => answerLocalRunRequest(true)}>Allow</button>
          </footer>
        </div>
      </div>
    );
  }
  if (pending) {
    return (
      <div className="zorai-code-dialog-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) close(); }}>
        <form className="zorai-code-close-dialog" role="dialog" aria-modal="true" aria-label={pending.command.title} onSubmit={(event) => {
          event.preventDefault();
          setBusy(true);
          storeInputs(`command:${commandKey}`, values);
          const filled = Object.fromEntries(pending.command.inputs.map((input) => [input.key, values[input.key] ?? input.default]));
          void executePluginCommand(pending.plugin, pending.command, filled).finally(() => setBusy(false));
        }}>
          <h2>{pending.plugin.name}: {pending.command.title}</h2>
          <InputFields inputs={pending.command.inputs} values={values} onChange={setValues} />
          <footer>
            <button type="button" disabled={busy} onClick={close}>Cancel</button>
            <button type="submit" className="is-primary" disabled={busy || !ready}>{busy ? "Running…" : "Run"}</button>
          </footer>
        </form>
      </div>
    );
  }
  if (result) {
    return (
      <div className="zorai-code-dialog-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) close(); }}>
        <div className="zorai-code-close-dialog zorai-plugin-result" role="dialog" aria-modal="true" aria-label={result.title}>
          <h2>{result.title}</h2>
          {result.error ? <p role="alert">{result.markdown}</p> : <div className="zorai-plugin-panel__result"><MarkdownContent content={result.markdown} /></div>}
          <footer>
            {!result.error ? <button type="button" onClick={() => { appendToComposer(result.markdown); close(); }}>Add to message</button> : null}
            <button type="button" className="is-primary" onClick={close}>Close</button>
          </footer>
        </div>
      </div>
    );
  }
  return null;
}
