export const CODE_SETTINGS_TABS = [
  { id: "editor", label: "Editor" },
  { id: "files", label: "Files" },
  { id: "appearance", label: "Appearance" },
  { id: "language-servers", label: "Language Servers" },
  { id: "shortcuts", label: "Shortcuts" },
  { id: "performance", label: "Performance" },
] as const;

export type CodeSettingsTab = (typeof CODE_SETTINGS_TABS)[number]["id"];
