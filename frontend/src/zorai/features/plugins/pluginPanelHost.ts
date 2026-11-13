import { MEDIA_KINDS, type MediaKind } from "./pluginMediaOptions";
export const PANEL_MESSAGE_TAG = "plugin-panel";
const APPROVALS_KEY = "zorai-plugin-panel-approvals";
const MAX_TEXT = 20000;
const MAX_TOAST = 280;
const MAX_COMMAND = 4000;
const THEME_VARIABLES = ["--bg-primary", "--bg-secondary", "--bg-elevated", "--text-primary", "--text-secondary", "--text-muted", "--accent", "--border", "--font-ui", "--font-mono"];

export type PanelHostContext = {
  pluginName: string;
  permissions: string[];
  writeEndpoints: string[];
  call: (endpoint: string, params: Record<string, unknown>) => Promise<string>;
  insertText: (text: string) => void;
  ask: (text: string) => void;
  toast: (text: string) => void;
  pickFolder: (title: string) => Promise<string | null>;
  openTerminal: (request: { title: string; cwd?: string; command: string }) => void;
  mediaOptions: (kind: MediaKind) => unknown;
  theme: () => Record<string, string>;
};

type PanelReply = { zorai: typeof PANEL_MESSAGE_TAG; id: string | number; ok: boolean; result?: unknown; error?: string };

export async function handlePanelMessage(data: unknown, context: PanelHostContext): Promise<PanelReply | null> {
  if (!data || typeof data !== "object") return null;
  const message = data as { zorai?: unknown; id?: unknown; method?: unknown; params?: unknown };
  if (message.zorai !== PANEL_MESSAGE_TAG || (typeof message.id !== "string" && typeof message.id !== "number")) return null;
  const id = message.id;
  const params = message.params && typeof message.params === "object" && !Array.isArray(message.params) ? message.params as Record<string, unknown> : {};
  const fail = (error: string): PanelReply => ({ zorai: PANEL_MESSAGE_TAG, id, ok: false, error });
  const ok = (result?: unknown): PanelReply => ({ zorai: PANEL_MESSAGE_TAG, id, ok: true, result });
  try {
    if (message.method === "call") {
      if (typeof params.endpoint !== "string" || !params.endpoint) return fail("call needs an endpoint name.");
      if (context.writeEndpoints.includes(params.endpoint) && !context.permissions.includes("write")) {
        return fail(`${context.pluginName} did not ask for the "write" permission, so this panel cannot use ${params.endpoint}.`);
      }
      const callParams = params.params && typeof params.params === "object" && !Array.isArray(params.params) ? params.params as Record<string, unknown> : {};
      return ok(await context.call(params.endpoint, callParams));
    }
    if (message.method === "theme") return ok(context.theme());
    if (message.method === "mediaOptions") {
      if (typeof params.kind !== "string" || !(MEDIA_KINDS as readonly string[]).includes(params.kind)) return fail(`mediaOptions needs a kind: ${MEDIA_KINDS.join(", ")}.`);
      return ok(context.mediaOptions(params.kind as MediaKind));
    }
    if (message.method === "insertText") {
      if (!context.permissions.includes("composer")) return fail(`${context.pluginName} did not ask for the "composer" permission.`);
      if (typeof params.text !== "string" || !params.text.trim()) return fail("insertText needs text.");
      context.insertText(params.text.slice(0, MAX_TEXT));
      return ok();
    }
    if (message.method === "ask") {
      if (!context.permissions.includes("composer")) return fail(`${context.pluginName} did not ask for the "composer" permission.`);
      if (typeof params.text !== "string") return fail("ask needs text.");
      context.ask(params.text.slice(0, MAX_TEXT));
      return ok();
    }
    if (message.method === "pickFolder") {
      if (!context.permissions.includes("files")) return fail(`${context.pluginName} did not ask for the "files" permission.`);
      const title = typeof params.title === "string" && params.title.trim() ? params.title.trim().slice(0, 120) : `${context.pluginName}: choose a folder`;
      return ok(await context.pickFolder(title));
    }
    if (message.method === "terminal") {
      if (!context.permissions.includes("terminal")) return fail(`${context.pluginName} did not ask for the "terminal" permission.`);
      if (typeof params.command !== "string" || !params.command.trim()) return fail("terminal needs a command.");
      if (params.command.length > MAX_COMMAND) return fail(`terminal commands are limited to ${MAX_COMMAND} characters.`);
      if (params.cwd !== undefined && (typeof params.cwd !== "string" || !params.cwd.startsWith("/"))) return fail("terminal cwd must be an absolute folder path.");
      const title = typeof params.title === "string" && params.title.trim() ? params.title.trim().slice(0, 60) : context.pluginName;
      context.openTerminal({ title, cwd: params.cwd as string | undefined, command: params.command.trim() });
      return ok({ opened: true });
    }
    if (message.method === "toast") {
      if (!context.permissions.includes("toast")) return fail(`${context.pluginName} did not ask for the "toast" permission.`);
      if (typeof params.text !== "string" || !params.text.trim()) return fail("toast needs text.");
      context.toast(params.text.slice(0, MAX_TOAST));
      return ok();
    }
    return fail(`Unknown method "${String(message.method)}".`);
  } catch (error) {
    return fail(error instanceof Error ? error.message : String(error));
  }
}

export function panelApprovalKey(plugin: { name: string; version: string }, panel: { id: string }): string {
  return `${plugin.name}@${plugin.version}/${panel.id}`;
}

export function readPanelApprovals(): Record<string, string> {
  try {
    return JSON.parse(localStorage.getItem(APPROVALS_KEY) ?? "{}");
  } catch {
    return {};
  }
}

export function writePanelApproval(key: string, permissions: string[]): void {
  localStorage.setItem(APPROVALS_KEY, JSON.stringify({ ...readPanelApprovals(), [key]: permissions.join(",") }));
}

export function panelThemeSnapshot(): Record<string, string> {
  if (typeof document === "undefined") return {};
  const style = getComputedStyle(document.documentElement);
  return {
    appearance: document.documentElement.dataset.appearance ?? "dark",
    ...Object.fromEntries(THEME_VARIABLES.map((name) => [name, style.getPropertyValue(name).trim()])),
  };
}
