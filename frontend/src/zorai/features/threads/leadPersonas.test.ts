import { describe, expect, it } from "vitest";
import { isLeadOnlyPersona } from "./leadPersonas";

describe("lead personas stay out of spawn", () => {
  it("rejects Svarog, Rarog, and Weles while keeping worker personas and provider reuse possible", () => {
    for (const alias of ["svarog", "Svarog", "swarog", "rarog", "Rarog", "weles", "WELES", "veles", "weles_builtin"]) {
      expect(isLeadOnlyPersona(alias)).toBe(true);
    }
    for (const alias of ["swarozyc", "radogost", "perun", "code_review", "openai", "gpt-5"]) {
      expect(isLeadOnlyPersona(alias)).toBe(false);
    }
  });
});
