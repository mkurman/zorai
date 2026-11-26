import { describe, expect, it } from "vitest";
import { extraRootOf, joinExtraRoot } from "./extraRoots";

describe("extra workspace folders", () => {
  it("gives a file in an extra folder that folder as its language-server root", () => {
    expect(extraRootOf("/work/lib/src/main.rs", ["/work/app", "/work/lib/"])).toEqual({ root: "/work/lib", relative: "src/main.rs" });
  });

  it("prefers the innermost folder so nested roots get their own server", () => {
    expect(extraRootOf("/work/lib/sub/a.ts", ["/work/lib", "/work/lib/sub"])?.root).toBe("/work/lib/sub");
  });

  it("does not match a sibling folder that shares a name prefix", () => {
    expect(extraRootOf("/work/library/a.ts", ["/work/lib"])).toBeNull();
  });

  it("maps a root-relative path back to the absolute path the editor tab uses", () => {
    expect(joinExtraRoot("/work/lib/", "src/a.ts")).toBe("/work/lib/src/a.ts");
  });
});
