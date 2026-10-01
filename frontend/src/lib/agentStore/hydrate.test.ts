import { describe, expect, it } from "vitest";
import { retainActiveThreadMessages } from "./hydrate";

describe("startup message retention", () => {
  it("keeps the open thread's transcript and drops every other thread's messages", () => {
    const messages = {
      active: [{ id: "a" }],
      other: [{ id: "b" }, { id: "c" }],
    };
    expect(retainActiveThreadMessages(messages, "active")).toEqual({ active: [{ id: "a" }] });
    expect(retainActiveThreadMessages(messages, null)).toEqual({});
    expect(messages.other).toHaveLength(2);
  });
});