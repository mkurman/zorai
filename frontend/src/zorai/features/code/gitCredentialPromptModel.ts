export type GitCredentialPromptKind = "confirm" | "username" | "secret";

export function gitCredentialPromptKind(prompt: string): GitCredentialPromptKind {
  if (/\(yes\/no/i.test(prompt)) return "confirm";
  if (/^\s*username\b/i.test(prompt)) return "username";
  return "secret";
}

export function gitCredentialPromptTitle(prompt: string): string {
  const kind = gitCredentialPromptKind(prompt);
  if (kind === "confirm") return "Trust this host?";
  if (kind === "username") return "Git username";
  if (/passphrase/i.test(prompt)) return "SSH key passphrase";
  return "Git password";
}
