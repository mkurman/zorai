import { describe, expect, it } from "vitest";
import {
  codeDocumentKey,
  createCodeDocumentController,
  type CodeLoadedDocument,
} from "./codeDocumentModel";

const loaded = (path: string, content = path): CodeLoadedDocument => ({
  root: "/work", path, content, original: content, hash: `h:${path}`, language: "typescript", byteSize: content.length, modifiedAt: 1, lineCount: 1,
});

describe("code document controller", () => {
  it("creates stable root/path keys", () => {
    expect(codeDocumentKey(" /work/ ", " src/a.ts ")).toBe("/work::src/a.ts");
  });

  it("deduplicates uncached reads and returns cached content without reread", async () => {
    let resolve!: (value: CodeLoadedDocument) => void;
    const read = () => new Promise<CodeLoadedDocument>((done) => { resolve = done; });
    const controller = createCodeDocumentController({ maxCachedDocuments: 3 });
    const first = controller.open("/work", "a.ts", read);
    const second = controller.open("/work", "a.ts", read);
    expect(first).toBe(second);
    resolve(loaded("a.ts"));
    await first;
    let called = 0;
    await controller.open("/work", "a.ts", async () => { called += 1; return loaded("a.ts"); });
    expect(called).toBe(0);
  });

  it("prevents a stale read from replacing newer document state", async () => {
    const controller = createCodeDocumentController({ maxCachedDocuments: 3 });
    let oldResolve!: (value: CodeLoadedDocument) => void;
    const old = controller.open("/work", "a.ts", () => new Promise((done) => { oldResolve = done; }));
    controller.invalidate("/work", "a.ts");
    await controller.open("/work", "a.ts", async () => loaded("a.ts", "new"));
    oldResolve(loaded("a.ts", "old"));
    await old;
    expect(controller.get("/work", "a.ts")?.content).toBe("new");
  });

  it("never evicts dirty documents and evicts least-recent clean entries", async () => {
    const controller = createCodeDocumentController({ maxCachedDocuments: 2 });
    await controller.open("/work", "a.ts", async () => loaded("a.ts"));
    await controller.open("/work", "b.ts", async () => loaded("b.ts"));
    controller.updateContent("/work", "a.ts", "dirty");
    await controller.open("/work", "c.ts", async () => loaded("c.ts"));
    expect(controller.get("/work", "a.ts")?.dirty).toBe(true);
    expect(controller.get("/work", "b.ts")).toBeNull();
    expect(controller.get("/work", "c.ts")).not.toBeNull();
  });

  it("honours a smaller Cached documents setting immediately, dropping the least recently used files", async () => {
    const controller = createCodeDocumentController({ maxCachedDocuments: 3 });
    for (const path of ["a.ts", "b.ts", "c.ts"]) await controller.open("/work", path, async () => loaded(path));
    controller.get("/work", "a.ts");
    controller.setMaxCachedDocuments(1);
    expect(controller.get("/work", "a.ts")).not.toBeNull();
    expect(controller.get("/work", "b.ts")).toBeNull();
    expect(controller.get("/work", "c.ts")).toBeNull();
  });
});
