import { describe, expect, it } from "vitest";
import { gitCredentialPromptKind, gitCredentialPromptTitle } from "./gitCredentialPromptModel";

describe("git credential prompts", () => {
  it("asks to trust an unknown host instead of collecting a secret", () => {
    const prompt = "The authenticity of host 'gitlab.com (1.2.3.4)' can't be established.\nED25519 key fingerprint is SHA256:abc.\nAre you sure you want to continue connecting (yes/no/[fingerprint])? ";
    expect(gitCredentialPromptKind(prompt)).toBe("confirm");
    expect(gitCredentialPromptTitle(prompt)).toBe("Trust this host?");
  });

  it("shows a username in clear text but hides passwords and passphrases", () => {
    expect(gitCredentialPromptKind("Username for 'https://gitlab.com': ")).toBe("username");
    expect(gitCredentialPromptKind("Password for 'https://me@gitlab.com': ")).toBe("secret");
    expect(gitCredentialPromptKind("Enter passphrase for key '/home/me/.ssh/id_ed25519': ")).toBe("secret");
    expect(gitCredentialPromptTitle("Enter passphrase for key '/home/me/.ssh/id_ed25519': ")).toBe("SSH key passphrase");
  });
});
