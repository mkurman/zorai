import { useEffect, useMemo, useState, type CSSProperties } from "react";
import { RefreshButton } from "@/zorai/shell/RefreshButton";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { useFileManagerStore, type SshProfile } from "../lib/fileManagerStore";
import { AppConfirmDialog } from "./AppConfirmDialog";
import { AppPromptDialog } from "./AppPromptDialog";
import { PaneView } from "./file-manager-panel/PaneView";
import { SshProfilesPanel } from "./file-manager-panel/SshProfilesPanel";
import {
    DEFAULT_LEFT_PATH,
    DEFAULT_RIGHT_PATH,
    actionButtonStyle,
    buildPaneRows,
    encodeToBase64,
    getBridge,
    getParentPath,
    joinPath,
    secondaryButtonStyle,
    type FsEntry,
    type PaneKey,
    type PaneState,
} from "./file-manager-panel/shared";

type FileManagerPanelProps = {
    style?: CSSProperties;
    className?: string;
};

export function FileManagerPanel({ style, className }: FileManagerPanelProps = {}) {
    const fileManagerOpen = useWorkspaceStore((s) => s.fileManagerOpen);
    const toggleFileManager = useWorkspaceStore((s) => s.toggleFileManager);
    const activePaneId = useWorkspaceStore((s) => s.activePaneId());

    const sshProfiles = useFileManagerStore((s) => s.sshProfiles);
    const addSshProfile = useFileManagerStore((s) => s.addSshProfile);
    const updateSshProfile = useFileManagerStore((s) => s.updateSshProfile);
    const removeSshProfile = useFileManagerStore((s) => s.removeSshProfile);
    const buildSshCommand = useFileManagerStore((s) => s.buildSshCommand);

    const [activePane, setActivePane] = useState<PaneKey>("left");
    const [leftPane, setLeftPane] = useState<PaneState>({
        path: DEFAULT_LEFT_PATH,
        entries: [],
        selectedPath: null,
        loading: false,
        error: null,
    });
    const [rightPane, setRightPane] = useState<PaneState>({
        path: DEFAULT_RIGHT_PATH,
        entries: [],
        selectedPath: null,
        loading: false,
        error: null,
    });
    const [leftPathInput, setLeftPathInput] = useState(DEFAULT_LEFT_PATH);
    const [rightPathInput, setRightPathInput] = useState(DEFAULT_RIGHT_PATH);
    const [statusMessage, setStatusMessage] = useState<string>("");
    const [selectedProfileId, setSelectedProfileId] = useState<string | null>(null);
    const [fullscreen, setFullscreen] = useState(true);
    const [pendingDelete, setPendingDelete] = useState<{ name: string; path: string } | null>(null);
    const [newFolderDialogOpen, setNewFolderDialogOpen] = useState(false);
    const [newFolderDraft, setNewFolderDraft] = useState("new-folder");

    const sourcePane = activePane === "left" ? leftPane : rightPane;
    const targetPane = activePane === "left" ? rightPane : leftPane;

    const selectedSourceEntry = useMemo(
        () => sourcePane.entries.find((entry) => entry.path === sourcePane.selectedPath) ?? null,
        [sourcePane.entries, sourcePane.selectedPath],
    );

    const selectedProfile = useMemo(
        () => sshProfiles.find((profile) => profile.id === selectedProfileId) ?? null,
        [sshProfiles, selectedProfileId],
    );

    useEffect(() => {
        void refreshPane("left", leftPane.path);
        void refreshPane("right", rightPane.path);
    }, []);

    useEffect(() => {
        if (sshProfiles.length === 0) {
            setSelectedProfileId(null);
            return;
        }
        if (!selectedProfileId || !sshProfiles.some((p) => p.id === selectedProfileId)) {
            setSelectedProfileId(sshProfiles[0].id);
        }
    }, [sshProfiles, selectedProfileId]);

    useEffect(() => {
        const onKeyDown = (event: KeyboardEvent) => {
            const target = event.target as HTMLElement | null;
            const tag = target?.tagName.toLowerCase();
            if (tag === "input" || tag === "textarea" || tag === "select" || target?.isContentEditable) {
                return;
            }

            if (event.key === "F5") {
                event.preventDefault();
                void handleCopy();
            } else if (event.key === "F6") {
                event.preventDefault();
                void handleMove();
            } else if (event.key === "F7") {
                event.preventDefault();
                void handleCreateDirectory();
            } else if (event.key === "F8") {
                event.preventDefault();
                void handleDelete();
            } else if (event.key === "Tab") {
                event.preventDefault();
                setActivePane((prev) => (prev === "left" ? "right" : "left"));
            } else if (event.key === "ArrowDown") {
                event.preventDefault();
                moveSelection(activePane, 1);
            } else if (event.key === "ArrowUp") {
                event.preventDefault();
                moveSelection(activePane, -1);
            } else if (event.key === "ArrowRight" || event.key === "Enter") {
                event.preventDefault();
                void openSelected(activePane);
            } else if (event.key === "ArrowLeft" || event.key === "Backspace") {
                event.preventDefault();
                void goParent(activePane);
            }
        };

        window.addEventListener("keydown", onKeyDown);
        return () => window.removeEventListener("keydown", onKeyDown);
    }, [
        sourcePane.path,
        sourcePane.selectedPath,
        sourcePane.entries,
        targetPane.path,
        leftPane.path,
        leftPane.selectedPath,
        leftPane.entries,
        rightPane.path,
        rightPane.selectedPath,
        rightPane.entries,
        activePane,
    ]);

    function getPane(which: PaneKey): PaneState {
        return which === "left" ? leftPane : rightPane;
    }

    function moveSelection(which: PaneKey, delta: number) {
        const pane = getPane(which);
        const rows = buildPaneRows(pane);
        if (rows.length === 0) return;

        const currentIndex = rows.findIndex((row) => row.path === pane.selectedPath);
        const nextIndex = currentIndex < 0
            ? (delta > 0 ? 0 : rows.length - 1)
            : Math.min(rows.length - 1, Math.max(0, currentIndex + delta));

        setPaneSelection(which, rows[nextIndex].path);
    }

    async function openSelected(which: PaneKey) {
        const pane = getPane(which);
        const rows = buildPaneRows(pane);
        if (rows.length === 0) return;

        const selectedRow = rows.find((row) => row.path === pane.selectedPath) ?? rows[0];
        if (!pane.selectedPath) {
            setPaneSelection(which, selectedRow.path);
            return;
        }

        if (selectedRow.type === "parent") {
            await refreshPane(which, selectedRow.path);
            return;
        }

        if (selectedRow.entry) {
            await openEntry(which, selectedRow.entry);
        }
    }

    async function goParent(which: PaneKey) {
        const pane = getPane(which);
        const parent = getParentPath(pane.path);
        if (parent) {
            await refreshPane(which, parent);
        }
    }

    async function refreshPane(which: PaneKey, requestedPath?: string) {
        const bridge = getBridge();
        if (!bridge?.listFsDir) {
            setStatusMessage("Filesystem bridge is not available.");
            return;
        }

        const nextPath = requestedPath ?? (which === "left" ? leftPane.path : rightPane.path);
        const setPane = which === "left" ? setLeftPane : setRightPane;
        const setInput = which === "left" ? setLeftPathInput : setRightPathInput;

        setPane((prev) => ({ ...prev, path: nextPath, loading: true, error: null }));

        try {
            const entries = await bridge.listFsDir(nextPath);
            entries.sort((a, b) => {
                if (a.isDirectory !== b.isDirectory) return a.isDirectory ? -1 : 1;
                return a.name.localeCompare(b.name);
            });

            setPane({
                path: nextPath,
                entries,
                selectedPath: null,
                loading: false,
                error: null,
            });
            setInput(nextPath);
        } catch (error: any) {
            setPane((prev) => ({
                ...prev,
                loading: false,
                error: error?.message ?? String(error),
            }));
        }
    }

    async function openEntry(which: PaneKey, entry: FsEntry) {
        const bridge = getBridge();
        if (entry.isDirectory) {
            await refreshPane(which, entry.path);
            return;
        }

        if (!bridge?.openFsPath) {
            setStatusMessage("Cannot open files in this environment.");
            return;
        }

        await bridge.openFsPath(entry.path);
        setStatusMessage(`Opened ${entry.name}`);
    }

    function setPaneSelection(which: PaneKey, path: string | null) {
        const setPane = which === "left" ? setLeftPane : setRightPane;
        setActivePane(which);
        setPane((prev) => ({ ...prev, selectedPath: path }));
    }

    async function handleCopy() {
        const bridge = getBridge();
        if (!bridge?.copyFsPath || !selectedSourceEntry) return;

        const destination = joinPath(targetPane.path, selectedSourceEntry.name);
        await bridge.copyFsPath(selectedSourceEntry.path, destination);
        await Promise.all([refreshPane("left"), refreshPane("right")]);
        setStatusMessage(`Copied ${selectedSourceEntry.name}`);
    }

    async function handleMove() {
        const bridge = getBridge();
        if (!bridge?.moveFsPath || !selectedSourceEntry) return;

        const destination = joinPath(targetPane.path, selectedSourceEntry.name);
        await bridge.moveFsPath(selectedSourceEntry.path, destination);
        await Promise.all([refreshPane("left"), refreshPane("right")]);
        setStatusMessage(`Moved ${selectedSourceEntry.name}`);
    }

    async function handleDelete() {
        const bridge = getBridge();
        if (!bridge?.deleteFsPath || !selectedSourceEntry) return;
        setPendingDelete({
            name: selectedSourceEntry.name,
            path: selectedSourceEntry.path,
        });
    }

    async function handleCreateDirectory() {
        const bridge = getBridge();
        if (!bridge?.createFsDirectory) return;
        setNewFolderDraft("new-folder");
        setNewFolderDialogOpen(true);
    }

    async function handleReveal() {
        const bridge = getBridge();
        if (!bridge?.revealFsPath || !selectedSourceEntry) return;
        await bridge.revealFsPath(selectedSourceEntry.path);
    }

    async function handleSwapPanes() {
        const leftPath = leftPane.path;
        const rightPath = rightPane.path;
        await Promise.all([
            refreshPane("left", rightPath),
            refreshPane("right", leftPath),
        ]);
        setStatusMessage("Swapped panes");
    }

    function addProfile() {
        const id = addSshProfile({
            name: `SSH ${sshProfiles.length + 1}`,
            host: "",
            user: "",
            port: 22,
            remotePath: "~",
        });
        setSelectedProfileId(id);
        setStatusMessage("New SSH profile created");
    }

    function updateProfile<K extends keyof SshProfile>(key: K, value: SshProfile[K]) {
        if (!selectedProfile) return;
        updateSshProfile(selectedProfile.id, { [key]: value } as Partial<SshProfile>);
    }

    async function runSshProfile(profileId: string) {
        const command = buildSshCommand(profileId);
        if (!command) {
            setStatusMessage("SSH profile is missing host information.");
            return;
        }

        const bridge = getBridge();
        if (bridge?.sendTerminalInput && activePaneId) {
            await bridge.sendTerminalInput(activePaneId, encodeToBase64(`${command}\r`));
            setStatusMessage("SSH command sent to active terminal pane.");
            return;
        }

        await navigator.clipboard.writeText(command);
        setStatusMessage("SSH command copied to clipboard.");
    }

    const canActOnSelection = Boolean(selectedSourceEntry);
    if (!fileManagerOpen) return null;

    return (
        <div
            style={{
                width: fullscreen ? "100%" : 860,
                minWidth: fullscreen ? 0 : 560,
                maxWidth: fullscreen ? "100%" : "70vw",
                height: fullscreen ? "100%" : "100%",
                display: "flex",
                flexDirection: "column",
                borderLeft: fullscreen ? "none" : "1px solid var(--border)",
                border: fullscreen ? "1px solid var(--border)" : undefined,
                background: "var(--zorai-bg)",
                overflow: "hidden",
                position: "relative",
                inset: fullscreen ? 0 : undefined,
                zIndex: fullscreen ? 1400 : undefined,
                ...(style ?? {}),
            }}
            className={className}
        >
            <div
                style={{
                    padding: "var(--space-3)",
                    borderBottom: "1px solid var(--border)",
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    gap: "var(--space-2)",
                    flexWrap: "wrap",
                }}
            >
                <div style={{ display: "flex", gap: "var(--space-2)", alignItems: "center" }}>
                    <span className="zorai-agent-indicator">Dual Pane Commander</span>
                    <span className="zorai-chip">F5 Copy</span>
                    <span className="zorai-chip">F6 Move</span>
                    <span className="zorai-chip">F7 Mkdir</span>
                    <span className="zorai-chip">F8 Delete</span>
                    <span className="zorai-chip">Arrows Navigate</span>
                    <span className="zorai-chip">Enter Open</span>
                    <span className="zorai-chip">Tab Switch Pane</span>
                </div>

                <div style={{ display: "flex", gap: "var(--space-2)" }}>
                    <button type="button" onClick={() => setFullscreen((v) => !v)} style={secondaryButtonStyle}>
                        {fullscreen ? "Docked" : "Full Screen"}
                    </button>
                    <button type="button" onClick={toggleFileManager} style={secondaryButtonStyle}>
                        Close
                    </button>
                </div>
            </div>

            <div
                style={{
                    padding: "var(--space-2) var(--space-3)",
                    borderBottom: "1px solid var(--border)",
                    display: "flex",
                    gap: "var(--space-2)",
                    flexWrap: "wrap",
                }}
            >
                <button type="button" style={actionButtonStyle} disabled={!canActOnSelection} onClick={() => void handleCopy()}>
                    Copy -&gt;
                </button>
                <button type="button" style={actionButtonStyle} disabled={!canActOnSelection} onClick={() => void handleMove()}>
                    Move -&gt;
                </button>
                <button type="button" style={actionButtonStyle} disabled={!canActOnSelection} onClick={() => void handleDelete()}>
                    Delete
                </button>
                <button type="button" style={actionButtonStyle} onClick={() => void handleCreateDirectory()}>
                    Mkdir
                </button>
                <button type="button" style={actionButtonStyle} disabled={!canActOnSelection} onClick={() => void handleReveal()}>
                    Reveal
                </button>
                <button type="button" style={secondaryButtonStyle} onClick={() => void handleSwapPanes()}>
                    Swap Panes
                </button>
                <RefreshButton label="Refresh Both" onClick={() => void Promise.all([refreshPane("left"), refreshPane("right")])} />

                {statusMessage && (
                    <span style={{ color: "var(--text-secondary)", fontSize: "var(--text-xs)", marginLeft: "auto" }}>
                        {statusMessage}
                    </span>
                )}
            </div>

            <div style={{ flex: 1, minHeight: 0, display: "grid", gridTemplateColumns: "1fr 1fr", gap: "var(--space-2)", padding: "var(--space-2)" }}>
                <PaneView
                    title="Left"
                    active={activePane === "left"}
                    pane={leftPane}
                    inputPath={leftPathInput}
                    onPathInputChange={setLeftPathInput}
                    onGo={() => void refreshPane("left", leftPathInput.trim() || leftPane.path)}
                    onSelect={(path) => setPaneSelection("left", path)}
                    onOpen={(entry) => void openEntry("left", entry)}
                    onParent={() => {
                        const parent = getParentPath(leftPane.path);
                        if (parent) {
                            void refreshPane("left", parent);
                        }
                    }}
                />

                <PaneView
                    title="Right"
                    active={activePane === "right"}
                    pane={rightPane}
                    inputPath={rightPathInput}
                    onPathInputChange={setRightPathInput}
                    onGo={() => void refreshPane("right", rightPathInput.trim() || rightPane.path)}
                    onSelect={(path) => setPaneSelection("right", path)}
                    onOpen={(entry) => void openEntry("right", entry)}
                    onParent={() => {
                        const parent = getParentPath(rightPane.path);
                        if (parent) {
                            void refreshPane("right", parent);
                        }
                    }}
                />
            </div>

            <SshProfilesPanel
                sshProfiles={sshProfiles}
                selectedProfileId={selectedProfileId}
                selectedProfile={selectedProfile}
                setSelectedProfileId={setSelectedProfileId}
                addProfile={addProfile}
                updateProfile={updateProfile}
                buildSshCommand={buildSshCommand}
                runSshProfile={runSshProfile}
                removeSshProfile={removeSshProfile}
                setStatusMessage={setStatusMessage}
            />

            <AppConfirmDialog
                open={Boolean(pendingDelete)}
                title={pendingDelete ? `Delete '${pendingDelete.name}'?` : ""}
                message="This action permanently deletes the selected file or directory."
                confirmLabel="Delete"
                tone="danger"
                onCancel={() => setPendingDelete(null)}
                onConfirm={() => {
                    if (!pendingDelete) return;
                    const bridge = getBridge();
                    if (!bridge?.deleteFsPath) {
                        setPendingDelete(null);
                        return;
                    }
                    void bridge.deleteFsPath(pendingDelete.path)
                        .then(() => Promise.all([refreshPane("left"), refreshPane("right")]))
                        .then(() => setStatusMessage(`Deleted ${pendingDelete.name}`))
                        .finally(() => setPendingDelete(null));
                }}
            />

            <AppPromptDialog
                open={newFolderDialogOpen}
                title="Create New Folder"
                message={`Create a new directory in '${sourcePane.path}'.`}
                confirmLabel="Create"
                tone="neutral"
                defaultValue={newFolderDraft}
                placeholder="Folder name"
                onCancel={() => setNewFolderDialogOpen(false)}
                onConfirm={(value) => {
                    const bridge = getBridge();
                    const nextName = value.trim();
                    if (!bridge?.createFsDirectory || !nextName) {
                        setNewFolderDialogOpen(false);
                        return;
                    }

                    const destination = joinPath(sourcePane.path, nextName);
                    void bridge.createFsDirectory(destination)
                        .then(() => refreshPane(activePane))
                        .then(() => setStatusMessage(`Created ${nextName}`))
                        .finally(() => setNewFolderDialogOpen(false));
                }}
            />
        </div>
    );
}
