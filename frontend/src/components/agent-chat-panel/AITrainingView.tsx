import { useEffect, useMemo, useState } from "react";
import { allLeafIds } from "../../lib/bspTree";
import { useWorkspaceStore } from "../../lib/workspaceStore";
import {
    buildAITrainingInstallCommand,
    buildAITrainingLaunchCommand,
    getAITrainingLaunchMode,
    getAITrainingLaunchModes,
} from "../../plugins/ai-training/definitions";
import { useAITrainingStore } from "../../plugins/ai-training/store";
import { RefreshButton } from "@/zorai/shell/RefreshButton";
import { ContextCard, EmptyPanel, MetricRibbon, SectionTitle, inputStyle } from "./shared";

export function AITrainingView() {
    const workspaces = useWorkspaceStore((state) => state.workspaces);
    const activeWorkspaceId = useWorkspaceStore((state) => state.activeWorkspaceId);
    const activeSurface = useWorkspaceStore((state) => state.activeSurface());
    const activePaneId = useWorkspaceStore((state) => state.activePaneId());

    const {
        profiles,
        status,
        error,
        selectedProfileId,
        selectedWorkspaceId,
        selectedSurfaceId,
        selectedPaneId,
        selectedLaunchModeId,
        launchPrompt,
        launchState,
        launchError,
        lastLaunchCommand,
        installState,
        installError,
        lastInstallCommand,
        refreshProfiles,
        setSelectedProfileId,
        setSelectedWorkspaceId,
        setSelectedSurfaceId,
        setSelectedPaneId,
        setSelectedLaunchModeId,
        setLaunchPrompt,
        syncTargetSelection,
        launchSelectedProfile,
        installSelectedProfile,
    } = useAITrainingStore();
    const [confirmInstall, setConfirmInstall] = useState(false);

    useEffect(() => {
        if (status === "idle") {
            void refreshProfiles(activeWorkspaceId);
        }
    }, [activeWorkspaceId, refreshProfiles, status]);

    useEffect(() => {
        syncTargetSelection(activeWorkspaceId, activeSurface?.id ?? null, activePaneId);
    }, [activePaneId, activeSurface?.id, activeWorkspaceId, syncTargetSelection]);

    useEffect(() => {
        setConfirmInstall(false);
    }, [selectedProfileId, selectedWorkspaceId, selectedSurfaceId, selectedPaneId]);

    const selectedWorkspace = useMemo(() => {
        return workspaces.find((workspace) => workspace.id === selectedWorkspaceId)
            ?? workspaces.find((workspace) => workspace.id === activeWorkspaceId)
            ?? workspaces[0]
            ?? null;
    }, [activeWorkspaceId, selectedWorkspaceId, workspaces]);

    const selectedSurface = useMemo(() => {
        if (!selectedWorkspace) {
            return null;
        }

        return selectedWorkspace.surfaces.find((surface) => surface.id === selectedSurfaceId)
            ?? selectedWorkspace.surfaces.find((surface) => surface.id === selectedWorkspace.activeSurfaceId)
            ?? selectedWorkspace.surfaces[0]
            ?? null;
    }, [selectedSurfaceId, selectedWorkspace]);

    const paneOptions = useMemo(() => {
        if (!selectedSurface) {
            return [] as Array<{ id: string; label: string }>;
        }

        return allLeafIds(selectedSurface.layout).map((paneId) => ({
            id: paneId,
            label: selectedSurface.paneNames[paneId] ?? paneId,
        }));
    }, [selectedSurface]);

    useEffect(() => {
        void refreshProfiles(selectedWorkspace?.id ?? activeWorkspaceId ?? null);
    }, [activeWorkspaceId, refreshProfiles, selectedWorkspace?.id, selectedWorkspace?.cwd]);

    const selectedProfile = profiles.find((profile) => profile.id === selectedProfileId) ?? null;
    const selectedLaunchMode = selectedProfile ? getAITrainingLaunchMode(selectedProfile, selectedLaunchModeId) : null;
    const selectedLaunchModes = selectedProfile ? getAITrainingLaunchModes(selectedProfile) : [];
    const installCommand = selectedProfile ? buildAITrainingInstallCommand(selectedProfile, selectedWorkspace?.cwd ?? null) : "";
    const actionLabel = selectedProfile?.readiness === "needs-setup" ? "Setup" : "Install";
    const requiresAction = Boolean(selectedProfile && selectedProfile.readiness !== "ready");
    const canInstall = Boolean(requiresAction && installCommand);
    const availableCount = profiles.filter((profile) => profile.available).length;
    const readyCount = profiles.filter((profile) => profile.readiness === "ready").length;
    const workspaceBoundCount = profiles.filter((profile) => profile.kind === "repository-workflow").length;

    return (
        <div style={{ padding: "var(--space-4)", overflow: "auto", height: "100%" }}>
            <MetricRibbon
                items={[
                    { label: "Supported", value: String(profiles.length || 0) },
                    { label: "Available", value: String(availableCount), accent: availableCount > 0 ? "var(--accent)" : "var(--text-muted)" },
                    { label: "Ready", value: String(readyCount), accent: readyCount > 0 ? "var(--accent)" : "var(--text-muted)" },
                    { label: "Workspace-Bound", value: String(workspaceBoundCount), accent: workspaceBoundCount > 0 ? "var(--accent)" : "var(--text-muted)" },
                    { label: "Workspace", value: selectedWorkspace?.cwd?.trim() || "not set" },
                ]}
            />

            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: "var(--space-3)", marginBottom: "var(--space-4)" }}>
                <div>
                    <div style={{ fontSize: "var(--text-lg)", fontWeight: 700 }}>AI Training</div>
                    <div style={{ fontSize: "var(--text-sm)", color: "var(--text-muted)", marginTop: 4 }}>
                        Inspect supported training runtimes and repository workflows, verify prerequisites, and launch setup or execution commands into a pane.
                    </div>
                </div>
                <RefreshButton
                    busy={status === "loading"}
                    label={status === "loading" ? "Scanning..." : "Refresh"}
                    onClick={() => void refreshProfiles(selectedWorkspace?.id ?? activeWorkspaceId ?? null)}
                />
            </div>

            <SectionTitle title="Target Surface" subtitle="Choose the workspace, surface, and pane for launch commands." />
            <div style={{ display: "grid", gridTemplateColumns: "repeat(3, minmax(0, 1fr))", gap: "var(--space-3)", marginBottom: "var(--space-4)" }}>
                <label style={{ display: "flex", flexDirection: "column", gap: 6, fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
                    Workspace
                    <select
                        value={selectedWorkspace?.id ?? ""}
                        onChange={(event) => setSelectedWorkspaceId(event.target.value || null)}
                        style={{ ...inputStyle, width: "100%" }}
                    >
                        {workspaces.map((workspace) => (
                            <option key={workspace.id} value={workspace.id}>{workspace.name}</option>
                        ))}
                    </select>
                </label>
                <label style={{ display: "flex", flexDirection: "column", gap: 6, fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
                    Surface
                    <select
                        value={selectedSurface?.id ?? ""}
                        onChange={(event) => setSelectedSurfaceId(event.target.value || null)}
                        style={{ ...inputStyle, width: "100%" }}
                    >
                        {(selectedWorkspace?.surfaces ?? []).map((surface) => (
                            <option key={surface.id} value={surface.id}>{surface.name}</option>
                        ))}
                    </select>
                </label>
                <label style={{ display: "flex", flexDirection: "column", gap: 6, fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
                    Pane
                    <select
                        value={selectedPaneId ?? selectedSurface?.activePaneId ?? paneOptions[0]?.id ?? ""}
                        onChange={(event) => setSelectedPaneId(event.target.value || null)}
                        style={{ ...inputStyle, width: "100%" }}
                    >
                        {paneOptions.map((pane) => (
                            <option key={pane.id} value={pane.id}>{pane.label}</option>
                        ))}
                    </select>
                </label>
            </div>

            <SectionTitle title="Training Profiles" subtitle="System checks and repo-shape checks are evaluated against the selected workspace cwd." />
            {profiles.length === 0 && status === "loading" ? <EmptyPanel message="Scanning AI training runtimes and repository workflows..." /> : null}
            {profiles.length === 0 && status !== "loading" ? <EmptyPanel message="No AI Training profiles are registered." /> : null}

            <div style={{ display: "grid", gap: "var(--space-3)" }}>
                {profiles.map((profile) => {
                    const isSelected = profile.id === selectedProfileId;
                    const systemChecks = profile.checks.filter((check) => check.scope === "system");
                    const workspaceChecks = profile.checks.filter((check) => check.scope === "workspace");

                    return (
                        <button
                            key={profile.id}
                            type="button"
                            onClick={() => setSelectedProfileId(profile.id)}
                            style={{
                                display: "grid",
                                gap: "var(--space-2)",
                                textAlign: "left",
                                padding: "var(--space-3)",
                                borderRadius: "var(--radius-lg)",
                                border: `1px solid ${isSelected ? "var(--accent)" : "var(--glass-border)"}`,
                                background: isSelected ? "var(--accent-soft)" : "var(--bg-secondary)",
                                cursor: "pointer",
                            }}
                        >
                            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: "var(--space-3)", flexWrap: "wrap" }}>
                                <div>
                                    <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)", flexWrap: "wrap" }}>
                                        <div style={{ fontSize: "var(--text-sm)", fontWeight: 700 }}>{profile.label}</div>
                                        <span style={{ padding: "2px 8px", borderRadius: 999, fontSize: "var(--text-xs)", border: "1px solid var(--glass-border)", color: "var(--text-muted)" }}>
                                            {profile.kind === "training-runtime" ? "Training Runtime" : "Repository Workflow"}
                                        </span>
                                    </div>
                                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginTop: 4 }}>{profile.description}</div>
                                </div>
                                <span
                                    style={{
                                        padding: "4px 10px",
                                        borderRadius: 999,
                                        fontSize: "var(--text-xs)",
                                        border: "1px solid var(--border)",
                                        color: profile.available ? "var(--success, #86efac)" : "var(--text-muted)",
                                    }}
                                >
                                    {profile.available ? "Available" : "Unavailable"}
                                </span>
                                <span
                                    style={{
                                        padding: "4px 10px",
                                        borderRadius: 999,
                                        fontSize: "var(--text-xs)",
                                        border: "1px solid var(--glass-border)",
                                        color: profile.readiness === "ready"
                                            ? "var(--success, #86efac)"
                                            : profile.readiness === "needs-setup"
                                                ? "var(--warning, #facc15)"
                                                : "var(--text-muted)",
                                    }}
                                >
                                    {profile.readiness === "ready" ? "Ready" : profile.readiness === "needs-setup" ? "Needs Setup" : "Missing"}
                                </span>
                            </div>

                            <div style={{ display: "grid", gridTemplateColumns: "repeat(3, minmax(0, 1fr))", gap: "var(--space-2)" }}>
                                <ContextCard label={profile.kind === "repository-workflow" ? "Runner" : "Executable"} value={profile.executable ?? profile.executables.join(", ")} />
                                <ContextCard label="Version" value={profile.version ?? "Not detected"} />
                                <ContextCard label={profile.kind === "repository-workflow" ? "Workspace Path" : "Path"} value={profile.path ?? profile.error ?? "Not found on PATH"} />
                            </div>

                            {profile.runtimeNotes?.length ? <ContextCard label="Runtime Status" value={profile.runtimeNotes.join(" | ")} /> : null}
                            {systemChecks.length ? <ContextCard label="System Checks" value={systemChecks.map((check) => `${check.exists ? "yes" : "no"}: ${check.label}`).join(" | ")} /> : null}
                            {workspaceChecks.length ? <ContextCard label="Workspace Checks" value={workspaceChecks.map((check) => `${check.exists ? "yes" : "no"}: ${check.path}`).join(" | ")} /> : null}
                        </button>
                    );
                })}
            </div>

            <SectionTitle title="Launch" subtitle="Run setup and execution commands in the selected pane." />
            <div style={{ display: "grid", gap: "var(--space-3)" }}>
                {selectedProfile ? (
                    <div style={{ display: "grid", gridTemplateColumns: "repeat(3, minmax(0, 1fr))", gap: "var(--space-2)" }}>
                        <ContextCard label="Homepage" value={selectedProfile.homepage ?? "Built-in runtime profile"} href={selectedProfile.homepage} />
                        <ContextCard label="Workspace" value={selectedWorkspace?.cwd?.trim() || "No workspace cwd configured"} />
                        <ContextCard label="Modes" value={selectedLaunchModes.map((mode) => mode.label).join(" | ")} />
                    </div>
                ) : null}

                {selectedProfile ? (
                    <label style={{ display: "flex", flexDirection: "column", gap: 6, fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
                        Launch Mode
                        <select
                            value={selectedLaunchMode?.id ?? ""}
                            onChange={(event) => setSelectedLaunchModeId(event.target.value || null)}
                            style={{ ...inputStyle, width: "100%" }}
                        >
                            {selectedLaunchModes.map((mode) => (
                                <option key={`${selectedProfile.id}-${mode.id}`} value={mode.id}>{mode.label}</option>
                            ))}
                        </select>
                    </label>
                ) : null}

                {selectedLaunchMode ? <ContextCard label="Mode Summary" value={selectedLaunchMode.description} /> : null}

                {selectedLaunchMode?.requiresPrompt ? (
                    <label style={{ display: "flex", flexDirection: "column", gap: 6, fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
                        Input
                        <textarea
                            value={launchPrompt}
                            onChange={(event) => setLaunchPrompt(event.target.value)}
                            placeholder={selectedLaunchMode.promptPlaceholder ?? "Enter a value."}
                            style={{
                                ...inputStyle,
                                width: "100%",
                                minHeight: 96,
                                resize: "vertical",
                                fontFamily: "inherit",
                            }}
                        />
                    </label>
                ) : null}

                <ContextCard
                    label="Command Preview"
                    value={selectedProfile
                        ? buildAITrainingLaunchCommand(selectedProfile, selectedLaunchMode?.id ?? null, launchPrompt, selectedWorkspace?.cwd ?? null) || "Unavailable"
                        : "Select a training profile"}
                />

                {requiresAction ? (
                    <ContextCard
                        label={`${actionLabel} Preview`}
                        value={installCommand || `No ${actionLabel.toLowerCase()} command is defined for this training profile yet.`}
                    />
                ) : null}

                {selectedProfile?.requirements?.length ? <ContextCard label="Requirements" value={selectedProfile.requirements.join(" | ")} /> : null}
                {selectedProfile?.installHints?.length ? <ContextCard label="Setup Hints" value={selectedProfile.installHints.join(" | ")} /> : null}
                {error ? <EmptyPanel message={error} /> : null}
                {launchError ? <EmptyPanel message={launchError} /> : null}
                {installError ? <EmptyPanel message={installError} /> : null}
                {launchState === "success" && lastLaunchCommand ? <EmptyPanel message={`Launched: ${lastLaunchCommand}`} /> : null}
                {installState === "success" && lastInstallCommand ? <EmptyPanel message={`Install command sent: ${lastInstallCommand}`} /> : null}
                {confirmInstall && canInstall ? <EmptyPanel message={`This will send the ${actionLabel.toLowerCase()} command to the selected pane. Continue only if you trust the runtime source and want to ${actionLabel.toLowerCase()} the missing prerequisites on this machine.`} /> : null}
                <div style={{ display: "flex", gap: "var(--space-2)", justifyContent: "flex-end" }}>
                    {confirmInstall && canInstall ? (
                        <button
                            type="button"
                            onClick={() => setConfirmInstall(false)}
                            style={{
                                padding: "var(--space-2) var(--space-4)",
                                borderRadius: "var(--radius-md)",
                                border: "1px solid var(--border)",
                                background: "var(--bg-secondary)",
                                color: "var(--text-primary)",
                                cursor: "pointer",
                                fontWeight: 600,
                            }}
                        >
                            Cancel
                        </button>
                    ) : null}
                    <button
                        type="button"
                        onClick={() => {
                            if (requiresAction) {
                                if (!canInstall) {
                                    return;
                                }

                                if (!confirmInstall) {
                                    setConfirmInstall(true);
                                    return;
                                }

                                void installSelectedProfile().then((ok) => {
                                    if (ok) {
                                        setConfirmInstall(false);
                                    }
                                });
                                return;
                            }

                            void launchSelectedProfile();
                        }}
                        disabled={requiresAction
                            ? !canInstall || installState === "installing"
                            : !selectedProfile || !selectedProfile.available || launchState === "launching"}
                        style={{
                            padding: "var(--space-2) var(--space-4)",
                            borderRadius: "var(--radius-md)",
                            border: "1px solid var(--accent)",
                            background: "var(--accent)",
                            color: "var(--zorai-bg)",
                            cursor: requiresAction
                                ? !canInstall || installState === "installing" ? "not-allowed" : "pointer"
                                : !selectedProfile || !selectedProfile.available || launchState === "launching" ? "not-allowed" : "pointer",
                            opacity: requiresAction
                                ? !canInstall || installState === "installing" ? 0.6 : 1
                                : !selectedProfile || !selectedProfile.available || launchState === "launching" ? 0.6 : 1,
                            fontWeight: 700,
                        }}
                    >
                        {requiresAction
                            ? installState === "installing"
                                ? `${actionLabel}...`
                                : confirmInstall
                                    ? `Are You Sure? ${actionLabel}`
                                    : canInstall
                                        ? `${actionLabel} in Pane`
                                        : `${actionLabel} Unavailable`
                            : launchState === "launching"
                                ? "Launching..."
                                : "Launch in Pane"}
                    </button>
                </div>
            </div>
        </div>
    );
}