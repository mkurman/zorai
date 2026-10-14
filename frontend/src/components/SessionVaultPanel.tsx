import { useState, type CSSProperties } from "react";
import { getBridge } from "../lib/bridge";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { useCommandLogStore } from "../lib/commandLogStore";
import { useTranscriptStore } from "../lib/transcriptStore";
import { getTerminalController } from "../lib/terminalRegistry";
import { openPersistedPath, revealPersistedPath } from "../lib/persistence";
import { SessionVaultContent } from "./session-vault-panel/SessionVaultContent";
import { SessionVaultFilters } from "./session-vault-panel/SessionVaultFilters";
import { SessionVaultHeader } from "./session-vault-panel/SessionVaultHeader";
import { buildTimeline, filterTranscripts, type MemoryProvenanceReport, type SessionVaultMode } from "./session-vault-panel/shared";

/**
 * Session Vault panel (Ctrl+Shift+V) — browse captured transcripts.
 */
type SessionVaultPanelProps = {
  style?: CSSProperties;
  className?: string;
};

export function SessionVaultPanel({ style, className }: SessionVaultPanelProps = {}) {
  const open = useWorkspaceStore((s) => s.sessionVaultOpen);
  const toggle = useWorkspaceStore((s) => s.toggleSessionVault);
  const commandEntries = useCommandLogStore((s) => s.entries);
  const transcripts = useTranscriptStore((s) => s.transcripts);
  const search = useTranscriptStore((s) => s.search);
  const getById = useTranscriptStore((s) => s.getById);
  const addTranscript = useTranscriptStore((s) => s.addTranscript);
  const removeTranscript = useTranscriptStore((s) => s.removeTranscript);
  const clearAll = useTranscriptStore((s) => s.clearAll);
  const activePaneId = useWorkspaceStore((s) => s.activePaneId());
  const activeSurface = useWorkspaceStore((s) => s.activeSurface());
  const activeWorkspace = useWorkspaceStore((s) => s.activeWorkspace());
  const workspaces = useWorkspaceStore((s) => s.workspaces);

  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [workspaceFilter, setWorkspaceFilter] = useState("all");
  const [surfaceFilter, setSurfaceFilter] = useState("all");
  const [paneFilter, setPaneFilter] = useState("all");
  const [reasonFilter, setReasonFilter] = useState("all");
  const [dateFilter, setDateFilter] = useState("all");
  const [timelineMode, setTimelineMode] = useState<SessionVaultMode>("timeline");
  const [timelineIndex, setTimelineIndex] = useState(0);
  const [memoryReport, setMemoryReport] = useState<MemoryProvenanceReport | null>(null);
  const [memoryStatus, setMemoryStatus] = useState<string | null>(null);
  const [loadingMemory, setLoadingMemory] = useState(false);

  if (!open) return null;

  const workspaceLabels = new Map<string, string>();
  const surfaceLabels = new Map<string, string>();
  for (const workspace of workspaces) {
    workspaceLabels.set(workspace.id, workspace.name);
    for (const surface of workspace.surfaces) {
      surfaceLabels.set(surface.id, surface.name);
    }
  }

  const searchResults = query.trim() ? search(query) : transcripts;
  const display = filterTranscripts(transcripts, searchResults, {
    query,
    workspaceFilter,
    surfaceFilter,
    paneFilter,
    reasonFilter,
    dateFilter,
  });
  const selected = getById(selectedId ?? display[0]?.id ?? "") ?? display[0] ?? null;
  const uniquePaneIds = [...new Set(transcripts.map((tx) => tx.paneId).filter(Boolean))] as string[];
  const uniqueReasons = [...new Set(transcripts.map((tx) => tx.reason))];
  const workspaceOptions = [...workspaceLabels.entries()].map(([id, name]) => ({ id, name }));
  const surfaceOptions = [...surfaceLabels.entries()].map(([id, name]) => ({ id, name }));
  const timeline = buildTimeline(commandEntries, display, {
    query,
    workspaceFilter,
    surfaceFilter,
    paneFilter,
    reasonFilter,
    dateFilter,
  });

  const copySelected = async () => {
    if (!selected) return;
    await navigator.clipboard.writeText(selected.content);
  };

  const sendSelectedToActivePane = async (execute = false) => {
    if (!selected) return;

    const controller = getTerminalController(activePaneId);
    if (!controller) return;

    await controller.sendText(selected.content, { execute, trackHistory: execute });
  };

  const exportSelected = () => {
    if (!selected) return;

    const blob = new Blob([selected.content], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = selected.filename.replace(/\//g, "_");
    anchor.click();
    URL.revokeObjectURL(url);
  };

  const openSelectedFile = async () => {
    if (!selected) return;
    await openPersistedPath(selected.filePath);
  };

  const revealSelectedFile = async () => {
    if (!selected) return;
    await revealPersistedPath(selected.filePath);
  };

  const captureActivePane = () => {
    const controller = getTerminalController(activePaneId);
    if (!controller) return;

    const content = controller.getSnapshot().trim();
    if (!content) return;

    addTranscript({
      content,
      reason: "manual",
      workspaceId: activeWorkspace?.id ?? null,
      surfaceId: activeSurface?.id ?? null,
      paneId: activePaneId ?? null,
      cwd: activeWorkspace?.cwd ?? null,
    });
  };

  const runTimelineCommand = async (command: string, execute: boolean) => {
    const controller = getTerminalController(activePaneId);
    if (!controller) return;
    await controller.sendText(command, {
      execute,
      trackHistory: execute,
      managed: execute,
      rationale: "Replay command from the Session Vault timeline",
      source: "replay",
    });
  };

  const loadMemoryProvenance = async () => {
    const bridge = getBridge();
    if (!bridge?.agentGetMemoryProvenanceReport) {
      setMemoryStatus("Memory provenance bridge unavailable.");
      return;
    }
    setLoadingMemory(true);
    try {
      const result = await bridge.agentGetMemoryProvenanceReport("MEMORY.md", 50) as { report?: MemoryProvenanceReport; target?: string | null } | MemoryProvenanceReport | { error?: string };
      if (result && typeof result === "object" && "error" in result && result.error) {
        throw new Error(result.error);
      }
      const report = result && typeof result === "object" && "report" in result
        ? (result.report ?? null)
        : result as MemoryProvenanceReport;
      setMemoryReport(report ?? null);
      setMemoryStatus(report ? `Loaded ${report.total_entries} memory provenance entries.` : "No memory provenance entries found.");
    } catch (error) {
      setMemoryStatus(error instanceof Error ? error.message : "Failed to load memory provenance.");
    } finally {
      setLoadingMemory(false);
    }
  };

  const confirmMemoryEntry = async (entryId: string) => {
    const bridge = getBridge();
    if (!bridge?.agentConfirmMemoryProvenanceEntry) {
      setMemoryStatus("Memory confirmation bridge unavailable.");
      return;
    }
    setLoadingMemory(true);
    try {
      const result = await bridge.agentConfirmMemoryProvenanceEntry(entryId) as { entry_id?: string; confirmed_at?: number; error?: string };
      if (result && typeof result === "object" && "error" in result && result.error) {
        throw new Error(result.error);
      }
      setMemoryStatus(`Confirmed ${result?.entry_id ?? entryId}.`);
      await loadMemoryProvenance();
    } catch (error) {
      setMemoryStatus(error instanceof Error ? error.message : "Failed to confirm memory provenance entry.");
      setLoadingMemory(false);
    }
  };

  const retractMemoryEntry = async (entryId: string) => {
    const bridge = getBridge();
    if (!bridge?.agentRetractMemoryProvenanceEntry) {
      setMemoryStatus("Memory retraction bridge unavailable.");
      return;
    }
    setLoadingMemory(true);
    try {
      const result = await bridge.agentRetractMemoryProvenanceEntry(entryId) as { entry_id?: string; retracted_at?: number; error?: string };
      if (result && typeof result === "object" && "error" in result && result.error) {
        throw new Error(result.error);
      }
      setMemoryStatus(`Retracted ${result?.entry_id ?? entryId}.`);
      await loadMemoryProvenance();
    } catch (error) {
      setMemoryStatus(error instanceof Error ? error.message : "Failed to retract memory provenance entry.");
      setLoadingMemory(false);
    }
  };

  return (
    <div
      onClick={toggle}
      style={{
        position: "fixed",
        inset: 0,
        background: "color-mix(in srgb, var(--shade) 72%, transparent)",
        display: "flex",
        alignItems: "flex-start",
        justifyContent: "center",
        padding: "4vh 2vw",
        zIndex: 940,
        backdropFilter: "none",
        ...(style ?? {}),
      }}
      className={className}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          background: "var(--zorai-bg)",
          border: "1px solid var(--glass-border)",
          borderRadius: 0,
          width: "min(1500px, 96vw)",
          maxHeight: "88vh",
          overflow: "hidden",
          display: "flex",
          flexDirection: "column",
        }}
        className="zorai-shell-card"
      >
        <SessionVaultHeader
          visibleCount={display.length}
          totalCount={transcripts.length}
          timelineCount={timeline.length}
          scopeLabel={activeWorkspace?.name ?? "all workspaces"}
          captureActivePane={captureActivePane}
          clearAll={clearAll}
          close={toggle}
        />

        <SessionVaultFilters
          query={query}
          setQuery={setQuery}
          workspaceFilter={workspaceFilter}
          setWorkspaceFilter={setWorkspaceFilter}
          surfaceFilter={surfaceFilter}
          setSurfaceFilter={setSurfaceFilter}
          paneFilter={paneFilter}
          setPaneFilter={setPaneFilter}
          reasonFilter={reasonFilter}
          setReasonFilter={setReasonFilter}
          dateFilter={dateFilter}
          setDateFilter={setDateFilter}
          workspaceOptions={workspaceOptions}
          surfaceOptions={surfaceOptions}
          paneOptions={uniquePaneIds}
          reasonOptions={uniqueReasons}
          close={toggle}
        />

        <SessionVaultContent
          timeline={timeline}
          timelineMode={timelineMode}
          setTimelineMode={setTimelineMode}
          memoryReport={memoryReport}
          memoryStatus={memoryStatus}
          loadingMemory={loadingMemory}
          loadMemoryProvenance={loadMemoryProvenance}
          confirmMemoryEntry={confirmMemoryEntry}
          retractMemoryEntry={retractMemoryEntry}
          selected={selected}
          setSelectedId={setSelectedId}
          display={display}
          workspaceLabels={workspaceLabels}
          surfaceLabels={surfaceLabels}
          runTimelineCommand={runTimelineCommand}
          sendSelectedToActivePane={sendSelectedToActivePane}
          copySelected={copySelected}
          removeTranscript={removeTranscript}
          openSelectedFile={openSelectedFile}
          revealSelectedFile={revealSelectedFile}
          exportSelected={exportSelected}
          timelineIndex={timelineIndex}
          setTimelineIndex={setTimelineIndex}
        />
      </div>
    </div>
  );
}
