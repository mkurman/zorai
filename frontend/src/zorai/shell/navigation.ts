import { zoraiTools, type ZoraiToolId } from "../features/tools/tools";

export type ZoraiViewId =
  | "code"
  | "threads"
  | "goals"
  | "workspaces"
  | "database"
  | "tools"
  | "skills"
  | "activity"
  | "plugins"
  | "settings";

export type ZoraiNavItem = {
  id: ZoraiViewId;
  label: string;
  railLabel: string;
  icon: ZoraiNavIconId;
  description: string;
};

export type ZoraiNavIconId =
  | "code"
  | "threads"
  | "goals"
  | "workspaces"
  | "database"
  | "tools"
  | "skills"
  | "activity"
  | "plugins"
  | "settings"
  | "nightMode"
  | "lightMode";

export const zoraiNavItems: ZoraiNavItem[] = [
  {
    id: "code",
    label: "Code",
    railLabel: "Code Agent",
    icon: "code",
    description: "Explore repository files and work with the code agent.",
  },
  {
    id: "threads",
    label: "Threads",
    railLabel: "Conversation Threads",
    icon: "threads",
    description: "Talk with agents, route participants, and launch goals.",
  },
  {
    id: "goals",
    label: "Goals",
    railLabel: "Mission Control",
    icon: "goals",
    description: "Inspect durable goals, steps, approvals, and active execution.",
  },
  {
    id: "workspaces",
    label: "Workspaces",
    railLabel: "Workspace Board",
    icon: "workspaces",
    description: "Coordinate board-owned tasks across thread and goal targets.",
  },
  {
    id: "database",
    label: "Database",
    railLabel: "SQLite Tables",
    icon: "database",
    description: "Inspect and edit Zorai database tables with paged row updates.",
  },
  {
    id: "tools",
    label: "Tools",
    railLabel: "Operator Tools",
    icon: "tools",
    description: "Open terminal, files, browser, history, system, and vault tools.",
  },
  {
    id: "skills",
    label: "Skills",
    railLabel: "Skills",
    icon: "skills",
    description: "Browse, edit, lock, disable, and prefer skills for agents and subagents.",
  },
  {
    id: "activity",
    label: "Activity",
    railLabel: "Activity Feed",
    icon: "activity",
    description: "Review events, approvals, notifications, and audit state.",
  },
  {
    id: "plugins",
    label: "Plugins",
    railLabel: "Plugin Panels",
    icon: "plugins",
    description: "Panels and commands added by installed plugins.",
  },
  {
    id: "settings",
    label: "Settings",
    railLabel: "Settings",
    icon: "settings",
    description: "Configure providers, models, tools, gateways, audio, and runtime behavior.",
  },
];

export const LAST_ZORAI_VIEW_STORAGE_KEY = "zorai.lastView";

export const RAIL_ITEM_IDS = zoraiNavItems.filter((item) => item.id !== "settings").map((item) => item.id);

export const STARTUP_VIEW_OPTIONS: ReadonlyArray<{ id: "threads" | "code" | "goals" | "workspaces" | "last"; label: string }> = [
  { id: "threads", label: "Threads" },
  { id: "code", label: "Code" },
  { id: "goals", label: "Goals" },
  { id: "workspaces", label: "Workspaces" },
  { id: "last", label: "Last opened tab" },
];

export function getDefaultZoraiView(startupView?: unknown, lastView?: string | null): ZoraiViewId {
  if (startupView === "last") {
    return zoraiNavItems.some((item) => item.id === lastView) ? lastView as ZoraiViewId : "threads";
  }
  return STARTUP_VIEW_OPTIONS.some((option) => option.id === startupView && option.id !== "last") ? startupView as ZoraiViewId : "threads";
}

export type ZoraiContextPanelLabels = {
  title: string;
  collapsed: string;
};

export function contextPanelLabels(view: ZoraiViewId): ZoraiContextPanelLabels {
  if (view === "code") {
    return { title: "Code Agent", collapsed: "Agent" };
  }
  return { title: "Orchestration Context", collapsed: "Context" };
}

export type ZoraiToolNavigationInput = {
  view?: string | null;
  tool?: string | null;
};

export function isZoraiViewId(value: string): value is ZoraiViewId {
  return zoraiNavItems.some((item) => item.id === value);
}

export function isZoraiToolId(value: string): value is ZoraiToolId {
  return zoraiTools.some((tool) => tool.id === value);
}

export function normalizeZoraiToolNavigation(input: ZoraiToolNavigationInput): {
  view?: ZoraiViewId;
  tool?: ZoraiToolId;
} {
  if (input.tool === "workspace") {
    return { view: "code" };
  }
  const normalized: { view?: ZoraiViewId; tool?: ZoraiToolId } = {};
  if (input.view && isZoraiViewId(input.view)) {
    normalized.view = input.view;
  }
  if (input.tool && isZoraiToolId(input.tool)) {
    normalized.tool = input.tool;
  }
  return normalized;
}