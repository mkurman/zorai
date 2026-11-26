import { describe, expect, it } from "vitest";
import { NAV_HISTORY_LIMIT, emptyNavHistory, navigateBack, navigateForward, recordNavLocation } from "./codeNavHistory";

const at = (path: string, line: number) => ({ path, line, column: 1 });

describe("Go Back / Go Forward", () => {
  it("returns to where you were before a jump to another file, then forward again", () => {
    let history = recordNavLocation(emptyNavHistory(), at("a.ts", 10));
    const back = navigateBack(history, at("b.ts", 3));
    expect(back.target).toEqual(at("a.ts", 10));
    history = back.history;
    const forward = navigateForward(history, at("a.ts", 10));
    expect(forward.target).toEqual(at("b.ts", 3));
  });

  it("treats small moves in the same file as one stop, so Back does not crawl line by line", () => {
    let history = emptyNavHistory();
    for (const line of [10, 12, 14]) history = recordNavLocation(history, at("a.ts", line));
    history = recordNavLocation(history, at("a.ts", 80));
    expect(history.back).toEqual([at("a.ts", 14), at("a.ts", 80)]);
  });

  it("skips stops that are where you already are, and does nothing when there is nowhere to go", () => {
    const history = recordNavLocation(recordNavLocation(emptyNavHistory(), at("a.ts", 10)), at("b.ts", 5));
    expect(navigateBack(history, at("b.ts", 6)).target).toEqual(at("a.ts", 10));
    expect(navigateBack(emptyNavHistory(), at("a.ts", 1)).target).toBeNull();
    expect(navigateForward(emptyNavHistory(), at("a.ts", 1)).target).toBeNull();
  });

  it("drops the forward list after a fresh jump, like a browser", () => {
    let history = recordNavLocation(emptyNavHistory(), at("a.ts", 1));
    history = navigateBack(history, at("b.ts", 1)).history;
    expect(history.forward).toHaveLength(1);
    expect(recordNavLocation(history, at("c.ts", 1)).forward).toEqual([]);
  });

  it("stays bounded", () => {
    let history = emptyNavHistory();
    for (let index = 0; index < NAV_HISTORY_LIMIT + 5; index += 1) history = recordNavLocation(history, at(`f${index}.ts`, 1));
    expect(history.back).toHaveLength(NAV_HISTORY_LIMIT);
  });
});
