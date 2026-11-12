import { create } from "zustand";
import { getBridge } from "@/lib/bridge";
import { pushToast } from "@/lib/toastStore";
import { useComposerDraftStore } from "@/zorai/features/threads/composerDraftStore";
import { navigateZorai } from "@/zorai/shell/zoraiNavigationEvents";

export const ASK_PANEL_ID = "__ask__";

export function fillPromptTemplate(template: string, values: Record<string, string>): string {
  return template.replace(/\{\{\s*([a-zA-Z0-9_-]+)\s*\}\}/g, (_match, key: string) => (values[key] ?? "").trim()).trim();
}

function appendToComposer(text: string) {
  useComposerDraftStore.getState().setInput((current) => (current.trim() ? `${current}\n\n${text}` : text));
  navigateZorai({ view: "threads" });
}

export type PluginCommandRequest = { plugin: ZoraiPluginUiManifest; command: ZoraiPluginUiCommand };
export type PluginCommandResult = { title: string; markdown: string; error: boolean };

type PluginUiState = {
  loaded: boolean;
  plugins: ZoraiPluginUiManifest[];
  disabled: string[];
  pendingCommand: PluginCommandRequest | null;
  commandResult: PluginCommandResult | null;
  selected: { plugin: string; panel: string | null } | null;
  focusPlugin: string | null;
  query: string;
  askRequest: { plugin: string; text: string; title: string } | null;
};

export const usePluginUiStore = create<PluginUiState>(() => ({
  loaded: false,
  plugins: [],
  disabled: [],
  pendingCommand: null,
  commandResult: null,
  selected: null,
  focusPlugin: null,
  query: "",
  askRequest: null,
}));

export function enabledPluginUi(manifests: ZoraiPluginUiManifest[], daemonPlugins: Array<{ name: string; enabled: boolean }> | null): { plugins: ZoraiPluginUiManifest[]; disabled: string[] } {
  if (!daemonPlugins) return { plugins: manifests, disabled: [] };
  const enabled = new Set(daemonPlugins.filter((plugin) => plugin.enabled).map((plugin) => plugin.name));
  return {
    plugins: manifests.filter((plugin) => enabled.has(plugin.name)),
    disabled: manifests.filter((plugin) => !enabled.has(plugin.name)).map((plugin) => plugin.name),
  };
}

export async function refreshPluginUi(): Promise<void> {
  const bridge = getBridge();
  if (!bridge?.pluginUiManifests) {
    usePluginUiStore.setState({ loaded: true });
    return;
  }
  const [manifests, daemon] = await Promise.all([
    bridge.pluginUiManifests().catch(() => [] as ZoraiPluginUiManifest[]),
    bridge.pluginDaemonList?.().then((result) => result.plugins).catch(() => null) ?? Promise.resolve(null),
  ]);
  usePluginUiStore.setState({ loaded: true, ...enabledPluginUi(manifests, daemon) });
}

export async function callPluginEndpoint(pluginName: string, endpoint: string, params: Record<string, unknown>): Promise<string> {
  const bridge = getBridge();
  if (!bridge?.pluginApiCall) throw new Error("Plugin calls need the desktop app.");
  const response = await bridge.pluginApiCall(pluginName, endpoint, params);
  if (!response?.success) throw new Error(response?.result || `${pluginName} could not run ${endpoint}.`);
  return response.result;
}

export function commandParams(command: ZoraiPluginUiCommand | Extract<ZoraiPluginUiPanel, { kind: "markdown" }>, values: Record<string, string>): Record<string, unknown> {
  const filled = Object.fromEntries(Object.entries(values).filter(([, value]) => value.trim() !== ""));
  return { ...command.params, ...filled };
}

export function runPluginCommand(plugin: ZoraiPluginUiManifest, command: ZoraiPluginUiCommand): void {
  if (command.inputs.length > 0) {
    usePluginUiStore.setState({ pendingCommand: { plugin, command }, commandResult: null });
    return;
  }
  void executePluginCommand(plugin, command, {});
}

export async function executePluginCommand(plugin: ZoraiPluginUiManifest, command: ZoraiPluginUiCommand, values: Record<string, string>): Promise<void> {
  const title = `${plugin.name}: ${command.title}`;
  if (command.prompt) {
    const text = fillPromptTemplate(command.prompt, values);
    usePluginUiStore.setState((state) => ({
      pendingCommand: null,
      askRequest: { plugin: plugin.name, text, title: Object.values(values).map((value) => value.trim()).filter(Boolean).join(" ") || text },
      selected: { plugin: plugin.name, panel: ASK_PANEL_ID },
      focusPlugin: state.focusPlugin === plugin.name ? plugin.name : null,
    }));
    navigateZorai({ view: "plugins" });
    return;
  }
  try {
    const result = await callPluginEndpoint(plugin.name, command.endpoint, commandParams(command, values));
    usePluginUiStore.setState({ pendingCommand: null });
    if (command.output === "composer") {
      appendToComposer(result);
      pushToast(`${title}: added to the message box.`, "info");
    } else if (command.output === "toast") {
      pushToast(result.length > 280 ? `${result.slice(0, 280)}…` : result, "info");
    } else {
      usePluginUiStore.setState({ commandResult: { title, markdown: result, error: false } });
    }
  } catch (error) {
    usePluginUiStore.setState({ pendingCommand: null, commandResult: { title, markdown: error instanceof Error ? error.message : String(error), error: true } });
  }
}
