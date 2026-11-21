import { describe, expect, it } from "vitest";
import { readableMath, restoreMathText } from "./mathText";

describe("math in exported table text", () => {
  it("turns the hidden formula placeholders back into readable text so CSV and PNG exports show D₄, not boxes", () => {
    expect(restoreMathText("0 Len64 / OOD128", ["$D_4$"])).toBe("D₄ Len64 / OOD128");
  });

  it("renders common notation as plain characters", () => {
    expect(readableMath("\\(x^2 \\pm \\sigma_{t}\\)")).toBe("x² ± \\sigma_t");
    expect(readableMath("$$O(n^{2})$$")).toBe("O(n²)");
    expect(readableMath("$\\mathrm{acc}_{ood}$")).toBe("acc_ood");
  });

  it("leaves text without placeholders unchanged", () => {
    expect(restoreMathText("plain", [])).toBe("plain");
  });
});
