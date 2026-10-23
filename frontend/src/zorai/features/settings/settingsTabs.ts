export const zoraiSettingsGroups = [
  { id: "agents", title: "Agents" },
  { id: "capabilities", title: "Capabilities" },
  { id: "automation", title: "Automation" },
  { id: "runtime", title: "Runtime" },
  { id: "system", title: "System" },
] as const;

export type ZoraiSettingsGroupId = (typeof zoraiSettingsGroups)[number]["id"];

export const zoraiSettingsTabs = [
  { id: "auth", group: "agents", icon: "key", title: "Providers", description: "Provider sign-in, API keys, and connection tests." },
  { id: "model", group: "agents", icon: "spark", title: "Svarog", description: "Main agent provider, model, routing, and identity." },
  { id: "concierge", group: "agents", icon: "compass", title: "Rarog", description: "Welcome agent and operational assistant behavior." },
  { id: "subagents", group: "agents", icon: "users", title: "Sub-agents", description: "Delegated roles attached to the orchestration runtime." },
  { id: "tools", group: "capabilities", icon: "wrench", title: "Tools", description: "Which tools agents may call, and generated tools." },
  { id: "skills", group: "capabilities", icon: "puzzle", title: "Skills", description: "Skill recommendation and community skill scouting." },
  { id: "search", group: "capabilities", icon: "globe", title: "Web", description: "Web search and browsing provider settings." },
  { id: "media", group: "capabilities", icon: "wave", title: "Audio & Images", description: "Speech-to-text, text-to-speech, and image generation." },
  { id: "memory", group: "capabilities", icon: "brain", title: "Memory", description: "Conversation memory, Honcho, consolidation, and embeddings." },
  { id: "heartbeat", group: "automation", icon: "pulse", title: "Heartbeat", description: "Background checks, schedule, quiet hours, and notifications." },
  { id: "proactive", group: "automation", icon: "radar", title: "Proactivity", description: "Anticipatory support, operator model, and feature tier." },
  { id: "workflows", group: "automation", icon: "flow", title: "Workflows", description: "Collaboration, thread titles, and code review." },
  { id: "gateway", group: "automation", icon: "message", title: "Gateway", description: "Slack, Discord, Telegram, and WhatsApp bridge." },
  { id: "runtime", group: "runtime", icon: "chat", title: "Chat", description: "Streaming, visible history, and session restore." },
  { id: "compaction", group: "runtime", icon: "layers", title: "Compaction", description: "When and how long conversations are compacted." },
  { id: "execution", group: "runtime", icon: "gauge", title: "Execution", description: "Tool loops, retries, delays, and timeouts." },
  { id: "safety", group: "runtime", icon: "shield", title: "Safety", description: "Managed command sandbox, approvals, and compliance." },
  { id: "mlflow", group: "runtime", icon: "activity", title: "MLflow", description: "Conversation tracing export to an MLflow tracking server." },
  { id: "storage", group: "system", icon: "disk", title: "Storage", description: "Snapshot retention and workspace git monitoring." },
  { id: "plugins", group: "system", icon: "plug", title: "Plugins", description: "Installed plugin runtime and enabled extensions." },
  { id: "interface", group: "system", icon: "palette", title: "Interface", description: "UI appearance, typography, and terminal shell preferences." },
  { id: "migration", group: "system", icon: "import", title: "Import", description: "Bring configuration over from Hermes or OpenClaw." },
  { id: "about", group: "system", icon: "info", title: "About", description: "Local runtime identity and shell status." },
] as const;

export type ZoraiSettingsTabId = (typeof zoraiSettingsTabs)[number]["id"];
export type ZoraiSettingsIconId = (typeof zoraiSettingsTabs)[number]["icon"];

export function getDefaultZoraiSettingsTab(): ZoraiSettingsTabId {
  return "auth";
}

export function getZoraiSettingsTab(tabId: ZoraiSettingsTabId) {
  return zoraiSettingsTabs.find((tab) => tab.id === tabId) ?? zoraiSettingsTabs[0];
}

export function zoraiSettingsTabsByGroup() {
  return zoraiSettingsGroups.map((group) => ({
    ...group,
    tabs: zoraiSettingsTabs.filter((tab) => tab.group === group.id),
  }));
}
