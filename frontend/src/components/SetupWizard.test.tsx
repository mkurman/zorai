import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { SetupWizard } from "./SetupWizard";

describe("SetupWizard", () => {
  it("opens on the same first question as `zorai setup` and walks the same eight steps", () => {
    const html = renderToStaticMarkup(<SetupWizard onClose={() => undefined} />);
    expect(html).toContain("How familiar are you with AI agents?");
    expect(html).toContain("Just getting started");
    expect(html).toContain("I build agent systems");
    expect(html).toContain("Step 1 of 8");
    expect(html).not.toContain(">Back<");
  });
});
