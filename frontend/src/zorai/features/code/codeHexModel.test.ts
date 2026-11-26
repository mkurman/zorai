import { describe, expect, it } from "vitest";
import { hexRow } from "./codeHexModel";

describe("hex viewer rows", () => {
  it("shows the offset, two 8-byte hex groups and printable ASCII with dots for the rest", () => {
    const row = hexRow(new Uint8Array([0x48, 0x69, 0x00, 0xff, 0x41]), 32);
    expect(row.address).toBe("00000020");
    expect(row.hex.startsWith("48 69 00 ff 41")).toBe(true);
    expect(row.hex.length).toBe(16 * 3);
    expect(row.ascii).toBe("Hi..A");
  });
});
