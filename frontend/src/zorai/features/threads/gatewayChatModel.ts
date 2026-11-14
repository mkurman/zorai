import { getBridge } from "@/lib/bridge";

export type GatewayChat = {
  channel_key: string;
  platform: string;
  channel_id: string;
  thread_id: string | null;
  thread_title: string | null;
  last_thread_id: string;
  last_thread_title: string | null;
  updated_at: number;
};

export type GatewayChatLinkState = "this" | "other" | "none";

function optionalString(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value : null;
}

export function parseGatewayChats(value: unknown): GatewayChat[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((entry) => {
    if (!entry || typeof entry !== "object") return [];
    const record = entry as Record<string, unknown>;
    const channelKey = optionalString(record.channel_key);
    if (!channelKey) return [];
    return [{
      channel_key: channelKey,
      platform: optionalString(record.platform) ?? channelKey.split(":")[0],
      channel_id: optionalString(record.channel_id) ?? "",
      thread_id: optionalString(record.thread_id),
      thread_title: optionalString(record.thread_title),
      last_thread_id: optionalString(record.last_thread_id) ?? "",
      last_thread_title: optionalString(record.last_thread_title),
      updated_at: typeof record.updated_at === "number" ? record.updated_at : 0,
    }];
  });
}

function chatName(platform: string, title: string | null): string {
  let name = (title ?? "").trim().replace(/^\[(.*)\]?$/s, "$1").replace(/\]$/, "").trim();
  const prefix = `${platform.toLowerCase()} `;
  if (name.toLowerCase().startsWith(prefix)) name = name.slice(prefix.length).trim();
  name = name.replace(/^message from\s+/i, "").replace(/\s*\([^()]*\)?$/, "").trim();
  return name;
}

export function gatewayChatName(chat: GatewayChat): string {
  return chatName(chat.platform, chat.last_thread_title) || chat.channel_id;
}

export function gatewayChatLabel(chat: GatewayChat): string {
  return `${chat.platform} ${gatewayChatName(chat)}`.trim();
}

export function gatewayChatOtherThread(chat: GatewayChat): string | null {
  const title = chat.thread_title?.trim();
  if (!title || chatName(chat.platform, title) === gatewayChatName(chat)) return null;
  return title;
}

export function gatewayChatLinkState(chat: GatewayChat, daemonThreadId: string | null | undefined): GatewayChatLinkState {
  if (!chat.thread_id) return "none";
  return daemonThreadId && chat.thread_id === daemonThreadId ? "this" : "other";
}

export function linkedGatewayChat(chats: GatewayChat[], daemonThreadId: string | null | undefined): GatewayChat | null {
  return chats.find((chat) => gatewayChatLinkState(chat, daemonThreadId) === "this") ?? null;
}

export function gatewayLinkedThreadIds(chats: GatewayChat[]): Set<string> {
  return new Set(chats.flatMap((chat) => (chat.thread_id ? [chat.thread_id] : [])));
}

export async function fetchGatewayChats(): Promise<GatewayChat[]> {
  const list = getBridge()?.agentListGatewayChats;
  return list ? parseGatewayChats(await list()) : [];
}

export async function linkThreadToGatewayChat(daemonThreadId: string, channelKey: string | null): Promise<GatewayChat[]> {
  const link = getBridge()?.agentLinkThreadToGatewayChat;
  if (!link) throw new Error("Gateway chat linking is not available in this build.");
  return parseGatewayChats(await link(daemonThreadId, channelKey));
}
