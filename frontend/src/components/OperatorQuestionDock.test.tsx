import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";

const storeState = vi.hoisted(() => ({
  question: null as null | { question_id: string; content: string; options: string[]; threadId: string | null },
  inlineHosts: 0,
  registerInlineHost: () => () => {},
}));

vi.mock("@/lib/bridge", () => ({ getBridge: () => null }));
vi.mock("@/lib/operatorQuestionStore", () => ({
  useOperatorQuestionStore: (selector: (state: typeof storeState) => unknown) => selector(storeState),
}));

import { OperatorQuestionDock } from "./OperatorQuestionDock";

describe("OperatorQuestionDock", () => {
  beforeEach(() => {
    storeState.question = { question_id: "oq_1", content: "Pick one:\nA. Alpha\nB. Beta", options: ["A", "B"], threadId: "thread-1" };
    storeState.inlineHosts = 0;
  });

  it("always offers a typed answer next to the agent's options", () => {
    const html = renderToStaticMarkup(<OperatorQuestionDock inline threadId="thread-1" />);

    expect(html).toContain(">A</button>");
    expect(html).toContain(">B</button>");
    expect(html).toContain("Other…");
  });

  it("leaves the question to the composer when one is mounted so the composer is never pushed aside", () => {
    expect(renderToStaticMarkup(<OperatorQuestionDock />)).toContain("Operator question");

    storeState.inlineHosts = 1;
    expect(renderToStaticMarkup(<OperatorQuestionDock />)).toBe("");
    expect(renderToStaticMarkup(<OperatorQuestionDock inline threadId="thread-1" />)).toContain("zorai-question-dock--inline");
  });

  it("keeps a question inside the thread that asked it so another thread's composer is not hijacked", () => {
    expect(renderToStaticMarkup(<OperatorQuestionDock inline threadId="thread-2" />)).toBe("");
    expect(renderToStaticMarkup(<OperatorQuestionDock inline threadId="thread-1" />)).toContain("Pick one");
  });
});
