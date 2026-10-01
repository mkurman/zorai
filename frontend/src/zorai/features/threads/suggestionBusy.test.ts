import { describe, expect, it } from "vitest";
import { retainBusySuggestions, suggestionActionLabel } from "./suggestionBusy";

describe("queued suggestion progress", () => {
  it("keeps Send busy until that suggestion leaves the queue", () => {
    const busy = { "suggestion-1": "send" as const };
    expect(retainBusySuggestions(busy, [{ id: "suggestion-1", status: "queued" }])).toEqual(busy);
    expect(retainBusySuggestions(busy, [])).toEqual({});
    expect(suggestionActionLabel("send")).toBe("Sending…");
  });

  it("stops the spinner when the send fails and the suggestion stays visible", () => {
    expect(retainBusySuggestions(
      { "suggestion-1": "send" },
      [{ id: "suggestion-1", status: "failed" }],
    )).toEqual({});
  });
});
