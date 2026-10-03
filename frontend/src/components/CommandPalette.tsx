import { useEffect, useRef, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { useSettingsStore } from "../lib/settingsStore";
import { useKeybindStore } from "../lib/keybindStore";
import { themesForAppearance } from "../lib/themes";
import { normalizeNightMode, resolveAppearance, systemPrefersDark } from "../lib/uiInterfacePrefs";
import { handleZoraiAppCommand } from "../zorai/shell/zoraiAppCommands";
import { navigateZorai } from "../zorai/shell/zoraiNavigationEvents";
import { zoraiNavItems } from "../zorai/shell/navigation";
import { zoraiTools } from "../zorai/features/tools/tools";
import { CommandPaletteHeader } from "./command-palette/CommandPaletteHeader";
import { CommandPaletteResults } from "./command-palette/CommandPaletteResults";
import type { Command, CommandPaletteProps } from "./command-palette/shared";

export function CommandPalette({ style, className }: CommandPaletteProps = {}) {
  const open = useWorkspaceStore((s) => s.commandPaletteOpen);
  const toggle = useWorkspaceStore((s) => s.toggleCommandPalette);
  const updateSetting = useSettingsStore((s) => s.updateSetting);
  const settings = useSettingsStore((s) => s.settings);
  const bindings = useKeybindStore((s) => s.bindings);

  const inputRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);

  const shortcutFor = (action: string) => bindings.find((binding) => binding.action === action)?.combo;
  const composeImagePrompt = (prompt = "") => {
    useWorkspaceStore.setState({ agentPanelOpen: true, commandPaletteOpen: false });
    window.setTimeout(() => {
      const detail = prompt.trim() ? { prompt: prompt.trim() } : {};
      window.dispatchEvent(new CustomEvent("zorai-agent-compose-image", { detail }));
      window.dispatchEvent(new CustomEvent("zorai-agent-compose-image", { detail }));
    }, 0);
  };

  const commands: Command[] = [
    ...zoraiNavItems.map((item) => ({
      id: `view-${item.id}`,
      label: item.label,
      category: "Navigate",
      action: () => navigateZorai({ view: item.id }),
    })),
    { id: "new-thread", label: "+ New Thread", category: "Threads", shortcut: shortcutFor("newSurface"), action: () => { handleZoraiAppCommand("new-surface"); } },
    { id: "search-threads", label: "Search Threads", category: "Threads", shortcut: shortcutFor("toggleSearch"), action: () => { handleZoraiAppCommand("toggle-search"); } },
    { id: "toggle-context", label: "Toggle Context Panel", category: "View", shortcut: shortcutFor("toggleSidebar"), action: () => { handleZoraiAppCommand("toggle-sidebar"); } },
    ...zoraiTools.map((tool) => ({
      id: `tool-${tool.id}`,
      label: tool.title,
      category: "Tools",
      action: () => navigateZorai({ view: "tools" as const, tool: tool.id }),
    })),
    { id: "about", label: "About", category: "View", action: () => { handleZoraiAppCommand("about"); } },
    { id: "image-prompt", label: "🖼 Image Prompt", category: "Agent", action: () => composeImagePrompt() },
    { id: "time-travel", label: "Time Travel Snapshots", category: "View", shortcut: shortcutFor("toggleTimeTravel"), action: () => { handleZoraiAppCommand("toggle-time-travel"); } },
    { id: "verify-integrity", label: "Verify WORM Integrity", category: "Infrastructure", action: () => { getBridge()?.verifyIntegrity?.(); } },
    { id: "toggle-sandbox", label: "Toggle Sandbox", category: "Infrastructure", action: () => updateSetting("sandboxEnabled", !settings.sandboxEnabled) },
    ...themesForAppearance(
      resolveAppearance(normalizeNightMode(settings.nightMode), systemPrefersDark()),
    ).map((theme) => ({
      id: `theme-${theme.name}`,
      label: `Theme: ${theme.name}`,
      category: "Theme",
      action: () => updateSetting("themeName", theme.name),
    })),
  ];

  const inlineImagePrompt = query.trim().startsWith("/image")
    ? {
        id: "image-inline-prompt",
        label: query.trim() === "/image" ? "🖼 Image Prompt" : `🖼 Generate Image: ${query.trim().slice("/image".length).trim()}`,
        category: "Agent",
        action: () => composeImagePrompt(query.trim().slice("/image".length).trim()),
      } satisfies Command
    : null;
  const commandItems = inlineImagePrompt ? [inlineImagePrompt, ...commands] : commands;

  const filtered = commandItems.filter(
    (c) =>
      c.label.toLowerCase().includes(query.toLowerCase()) ||
      c.id.toLowerCase().includes(query.toLowerCase()) ||
      (c.category && c.category.toLowerCase().includes(query.toLowerCase()))
  );

  const grouped = filtered.reduce((acc, cmd) => {
    const cat = cmd.category || "Other";
    if (!acc[cat]) acc[cat] = [];
    acc[cat].push(cmd);
    return acc;
  }, {} as Record<string, Command[]>);

  const categories = Object.keys(grouped).sort();
  const flatFiltered = categories.flatMap((cat) => grouped[cat]);

  useEffect(() => {
    if (open) {
      setQuery("");
      setSelectedIndex(0);
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [open]);

  useEffect(() => {
    setSelectedIndex((current) => {
      if (flatFiltered.length === 0) return 0;
      return Math.min(current, flatFiltered.length - 1);
    });
  }, [flatFiltered.length]);

  if (!open) return null;

  const executeAndClose = (command: Command) => {
    command.action();
    useWorkspaceStore.setState({ commandPaletteOpen: false });
  };

  return (
    <div
      onClick={toggle}
      style={{
        position: "fixed",
        inset: 0,
        background: "var(--bg-overlay)",
        display: "flex",
        alignItems: "flex-start",
        justifyContent: "center",
        paddingTop: 80,
        zIndex: 5000,
        backdropFilter: "none",
        ...(style ?? {}),
      }}
      className={className}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          background: "var(--bg-void)",
          border: "1px solid var(--border)",
          borderRadius: "var(--radius-xl)",
          width: 640,
          maxWidth: "92vw",
          maxHeight: "70vh",
          overflow: "hidden",
          display: "flex",
          flexDirection: "column",
        }}
      >
        <CommandPaletteHeader commandCount={filtered.length} />

        <input
          ref={inputRef}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") toggle();
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setSelectedIndex((current) => Math.min(current + 1, flatFiltered.length - 1));
            }
            if (e.key === "ArrowUp") {
              e.preventDefault();
              setSelectedIndex((current) => Math.max(current - 1, 0));
            }
            if (e.key === "Enter" && flatFiltered.length > 0) {
              const command = flatFiltered[selectedIndex];
              if (command) {
                executeAndClose(command);
              }
            }
          }}
          placeholder="Find commands, or type /image <prompt>..."
          style={{
            width: "100%",
            padding: "var(--space-4)",
            background: "transparent",
            border: "none",
            borderBottom: "1px solid var(--border)",
            color: "var(--text-primary)",
            fontSize: "var(--text-md)",
            fontFamily: "inherit",
            outline: "none",
          }}
        />

        <CommandPaletteResults
          filtered={filtered}
          grouped={grouped}
          categories={categories}
          flatFiltered={flatFiltered}
          selectedIndex={selectedIndex}
          setSelectedIndex={setSelectedIndex}
          onExecute={executeAndClose}
        />
      </div>
    </div>
  );
}
