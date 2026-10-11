import { describe, expect, it } from "vitest";
import {
  gatewayChatLabel,
  gatewayChatLinkState,
  gatewayLinkedThreadIds,
  linkedGatewayChat,
  parseGatewayChats,
} from "./gatewayChatModel";

const telegram = {
  channel_key: "Telegram:42",
  platform: "Telegram",
  channel_id: "42",
  thread_id: "thread_a",
  thread_title: "Planner",
  last_thread_id: "thread_a",
  last_thread_title: "telegram alice",
  updated_at: 10,
};
const discord = {
  channel_key: "Discord:7",
  platform: "Discord",
  channel_id: "7",
  thread_id: null,
  thread_title: null,
  last_thread_id: "thread_old",
  last_thread_title: null,
  updated_at: 5,
};

describe("gateway chat model", () => {
  it("labels chats by platform and sender so operators recognise their phone chat, falling back to the channel id", () => {
    const [first, second] = parseGatewayChats([telegram, discord]);
    expect(gatewayChatLabel(first)).toBe("Telegram alice");
    expect(gatewayChatLabel(second)).toBe("Discord 7");
  });

  it("marks only the chat linked to this daemon thread as linked, and others as busy on another thread", () => {
    const chats = parseGatewayChats([telegram, discord]);
    expect(gatewayChatLinkState(chats[0], "thread_a")).toBe("this");
    expect(gatewayChatLinkState(chats[0], "thread_b")).toBe("other");
    expect(gatewayChatLinkState(chats[0], null)).toBe("other");
    expect(gatewayChatLinkState(chats[1], "thread_a")).toBe("none");
    expect(linkedGatewayChat(chats, "thread_a")?.channel_key).toBe("Telegram:42");
    expect(linkedGatewayChat(chats, "thread_old")).toBeNull();
  });

  it("collects only currently linked thread ids, ignoring unlinked chats after !new", () => {
    expect([...gatewayLinkedThreadIds(parseGatewayChats([telegram, discord]))]).toEqual(["thread_a"]);
  });

  it("drops malformed bridge payloads instead of crashing the picker", () => {
    expect(parseGatewayChats(null)).toEqual([]);
    expect(parseGatewayChats([{ platform: "Slack" }, "junk"])).toEqual([]);
  });
});
