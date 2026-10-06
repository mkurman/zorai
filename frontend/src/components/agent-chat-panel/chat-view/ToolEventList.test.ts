import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ToolEventList, advanceToolTitleFrame } from "./ToolEventList";
import { TOOL_MARK_ENTER_DELAY_MS, TOOL_MARK_LEAVE_MS, toolMarkPresence } from "./ZoraiToolMark";
import type { ToolEventGroup } from "./types";

function toolGroup(status: ToolEventGroup["status"]): ToolEventGroup {
  return {
    key: "call-1",
    toolCallId: "call-1",
    toolName: "read_file",
    toolArguments: "{}",
    status,
    resultContent: "",
    createdAt: 1,
  };
}

describe("advanceToolTitleFrame", () => {
  it("keeps the previous working tool mounted so it can leave upward while the next enters from below", () => {
    expect(advanceToolTitleFrame({ current: "Read File", previous: null }, "Python", false)).toEqual({
      current: "Python",
      previous: "Read File",
    });
  });

  it("swaps the working tool immediately when motion is reduced", () => {
    expect(advanceToolTitleFrame({ current: "Read File", previous: null }, "Python", true)).toEqual({
      current: "Python",
      previous: null,
    });
  });

  it("does not restart the rise when the working tool name stays the same", () => {
    const frame = { current: "Python", previous: null };
    expect(advanceToolTitleFrame(frame, "Python", false)).toBe(frame);
  });
});

describe("tool list mark", () => {
  it("waits out the fade after work ends, and drops the mark immediately when motion is reduced", () => {
    expect(toolMarkPresence(false, true, false)).toEqual({
      present: true,
      shown: false,
      showAfterMs: null,
      hideAfterMs: TOOL_MARK_LEAVE_MS,
    });
    expect(toolMarkPresence(true, false, false).showAfterMs).toBe(TOOL_MARK_ENTER_DELAY_MS);
    expect(toolMarkPresence(false, false, false).present).toBe(false);
    expect(toolMarkPresence(false, true, true)).toEqual({
      present: false,
      shown: false,
      showAfterMs: null,
      hideAfterMs: null,
    });
  });

  it("draws the Zorai mark beside the list only while a tool call is still being generated", () => {
    const working = renderToStaticMarkup(createElement(ToolEventList, { groups: [toolGroup("executing")] }));
    expect(working).toContain("acp-tool-mark");
    expect(working).toContain("Read File");

    const finished = renderToStaticMarkup(createElement(ToolEventList, { groups: [toolGroup("done")] }));
    expect(finished).not.toContain("acp-tool-mark");
    expect(finished).toContain("Read File");
  });
});
