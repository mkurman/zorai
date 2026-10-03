import { useEffect, useMemo, useState } from "react";
import { allLeafIds } from "../../lib/bspTree";
import { useWorkspaceStore } from "../../lib/workspaceStore";
import {
    buildCodingAgentInstallCommand,
    buildCodingAgentLaunchCommand,
    CODING_AGENT_CAPABILITY_LABELS,
    getCodingAgentLaunchMode,
    getCodingAgentLaunchModes,
} from "../../plugins/coding-agents/agentDefinitions";
import { useCodingAgentsStore } from "../../plugins/coding-agents/store";
import { RefreshButton } from "@/zorai/shell/RefreshButton";
import { ContextCard, EmptyPanel, MetricRibbon, SectionTitle, inputStyle } from "./shared";

export function CodingAgentsView() {
    const workspaces = useWorkspaceStore((state) => state.workspaces);
    const activeWorkspaceId = useWorkspaceStore((state) => state.activeWorkspaceId);
    const activeSurface = useWorkspaceStore((state) => state.activeSurface());
    const activePaneId = useWorkspaceStore((state) => state.activePaneId());

    const {
        agents,
        status,
        error,
        selectedAgentId,
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
        refreshAgents,
        setSelectedAgentId,
        setSelectedWorkspaceId,
        setSelectedSurfaceId,
        setSelectedPaneId,
        setSelectedLaunchModeId,
        setLaunchPrompt,
        syncTargetSelection,
        launchSelectedAgent,
        installSelectedAgent,
    } = useCodingAgentsStore();
    const [confirmInstall, setConfirmInstall] = useState(false);

    useEffect(() => {
        if (status === "idle") {
            void refreshAgents();
        }
    }, [refreshAgents, status]);

    useEffect(() => {
        syncTargetSelection(activeWorkspaceId, activeSurface?.id ?? null, activePaneId);
    }, [activePaneId, activeSurface?.id, activeWorkspaceId, syncTargetSelection]);

    useEffect(() => {
        setConfirmInstall(false);
    }, [selectedAgentId, selectedWorkspaceId, selectedSurfaceId, selectedPaneId]);

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

    const selectedAgent = agents.find((agent) => agent.id === selectedAgentId) ?? null;
    const selectedLaunchMode = selectedAgent ? getCodingAgentLaunchMode(selectedAgent, selectedLaunchModeId) : null;
    const selectedLaunchModes = selectedAgent ? getCodingAgentLaunchModes(selectedAgent) : [];
    const installCommand = selectedAgent ? buildCodingAgentInstallCommand(selectedAgent) : "";
    const actionLabel = selectedAgent?.readiness === "needs-setup" ? "Setup" : "Install";
    const canInstall = Boolean(selectedAgent && selectedAgent.readiness !== "ready" && installCommand);
    const availableCount = agents.filter((agent) => agent.available).length;
    const configuredCount = agents.filter((agent) => agent.available && (agent.configPaths?.length ?? 0) > 0).length;
    const readyCount = agents.filter((agent) => agent.readiness === "ready").length;

    return (
        <div style={{ padding: "var(--space-4)", overflow: "auto", height: "100%" }}>
            <MetricRibbon
                items={[
                    { label: "Supported", value: String(agents.length || 0) },
                    { label: "Available", value: String(availableCount), accent: availableCount > 0 ? "var(--accent)" : "var(--text-muted)" },
                    { label: "Profiles", value: String(configuredCount), accent: configuredCount > 0 ? "var(--accent)" : "var(--text-muted)" },
                    { label: "Ready", value: String(readyCount), accent: readyCount > 0 ? "var(--accent)" : "var(--text-muted)" },
                    { label: "Target", value: selectedSurface ? `${selectedSurface.name} · ${selectedPaneId ?? selectedSurface.activePaneId ?? "none"}` : "none" },
                ]}
            />

            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: "var(--space-3)", marginBottom: "var(--space-4)" }}>
                <div>
                    <div style={{ fontSize: "var(--text-lg)", fontWeight: 700 }}>Coding Agents</div>
                    <div style={{ fontSize: "var(--text-sm)", color: "var(--text-muted)", marginTop: 4 }}>
                        Discover supported agent runtimes, inspect their capabilities, and launch them into a selected terminal pane.
                    </div>
                </div>
                <RefreshButton
                    busy={status === "loading"}
                    label={status === "loading" ? "Scanning..." : "Refresh"}
                    onClick={() => void refreshAgents()}
                />
            </div>

            <SectionTitle title="Target Surface" subtitle="Choose where the coding agent should start." />
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

            <SectionTitle title="Available Agents" subtitle="Detected CLIs can be launched directly into the selected pane." />
            {agents.length === 0 && status === "loading" ? (
                <EmptyPanel message="Scanning PATH for known coding-agent CLIs..." />
            ) : null}

            {agents.length === 0 && status !== "loading" ? (
                <EmptyPanel message="No coding-agent definitions are registered." />
            ) : null}

            <div style={{ display: "grid", gap: "var(--space-3)" }}>
                {agents.map((agent) => {
                    const isSelected = agent.id === selectedAgentId;
                    return (
                        <button
                            key={agent.id}
                            type="button"
                            onClick={() => setSelectedAgentId(agent.id)}
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
                            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: "var(--space-3)" }}>
                                <div>
                                    <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)", flexWrap: "wrap" }}>
                                        <div style={{ fontSize: "var(--text-sm)", fontWeight: 700 }}>{agent.label}</div>
                                        <span style={{ padding: "2px 8px", borderRadius: 999, fontSize: "var(--text-xs)", border: "1px solid var(--glass-border)", color: "var(--text-muted)" }}>
                                            {agent.kind === "agent-runtime" ? "Agent Runtime" : agent.kind === "gateway-runtime" ? "Gateway Runtime" : "Coding CLI"}
                                        </span>
                                    </div>
                                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginTop: 4 }}>{agent.description}</div>
                                </div>
                                <span
                                    style={{
                                        padding: "4px 10px",
                                        borderRadius: 999,
                                        fontSize: "var(--text-xs)",
                                        border: "1px solid var(--border)",
                                        color: agent.available ? "var(--success, #86efac)" : "var(--text-muted)",
                                    }}
                                >
                                    {agent.available ? "Available" : "Unavailable"}
                                </span>
                                <span
                                    style={{
                                        padding: "4px 10px",
                                        borderRadius: 999,
                                        fontSize: "var(--text-xs)",
                                        border: "1px solid var(--glass-border)",
                                        color: agent.readiness === "ready"
                                            ? "var(--success, #86efac)"
                                            : agent.readiness === "needs-setup"
                                                ? "var(--warning, #facc15)"
                                                : "var(--text-muted)",
                                    }}
                                >
                                    {agent.readiness === "ready" ? "Ready" : agent.readiness === "needs-setup" ? "Needs Setup" : "Missing"}
                                </span>
                            </div>
                            <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
                                {(agent.capabilities ?? []).map((capability) => (
                                    <span
                                        key={`${agent.id}-${capability}`}
                                        style={{
                                            padding: "4px 10px",
                                            borderRadius: 999,
                                            fontSize: "var(--text-xs)",
                                            border: "1px solid var(--glass-border)",
                                            color: "var(--text-muted)",
                                        }}
                                    >
                                        {CODING_AGENT_CAPABILITY_LABELS[capability]}
                                    </span>
                                ))}
                            </div>
                            <div style={{ display: "grid", gridTemplateColumns: "repeat(3, minmax(0, 1fr))", gap: "var(--space-2)" }}>
                                <ContextCard label="Executable" value={agent.executable ?? agent.executables.join(", ")} />
                                <ContextCard label="Version" value={agent.version ?? "Not detected"} />
                                <ContextCard label="Path" value={agent.path ?? agent.error ?? "Not found on PATH"} />
                            </div>
                            {agent.runtimeNotes?.length ? <ContextCard label="Runtime Status" value={agent.runtimeNotes.join(" | ")} /> : null}
                        </button>
                    );
                })}
            </div>

            <SectionTitle title="Launch" subtitle="Start the selected coding agent inside the selected pane." />
            <div style={{ display: "grid", gap: "var(--space-3)" }}>
                {selectedAgent ? (
                    <div style={{ display: "grid", gridTemplateColumns: "repeat(3, minmax(0, 1fr))", gap: "var(--space-2)" }}>
                        <ContextCard label="Homepage" value={selectedAgent.homepage ?? "Built-in runtime profile"} href={selectedAgent.homepage} />
                        <ContextCard label="Config" value={selectedAgent.configPaths?.join(" | ") ?? "No dedicated config path declared"} />
                        <ContextCard label="Modes" value={selectedLaunchModes.map((mode) => mode.label).join(" | ")} />
                    </div>
                ) : null}

                {selectedAgent?.checks?.length ? (
                    <ContextCard
                        label="Detected Setup"
                        value={selectedAgent.checks.map((check) => `${check.exists ? "yes" : "no"}: ${check.path}`).join(" | ")}
                    />
                ) : null}

                {selectedAgent?.gatewayLabel ? (
                    <ContextCard
                        label="Gateway"
                        value={`${selectedAgent.gatewayLabel} · ${selectedAgent.gatewayReachable ? "reachable" : "not reachable"}`}
                    />
                ) : null}

                {selectedAgent ? (
                    <label style={{ display: "flex", flexDirection: "column", gap: 6, fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
                        Launch Mode
                        <select
                            value={selectedLaunchMode?.id ?? ""}
                            onChange={(event) => setSelectedLaunchModeId(event.target.value || null)}
                            style={{ ...inputStyle, width: "100%" }}
                        >
                            {selectedLaunchModes.map((mode) => (
                                <option key={`${selectedAgent.id}-${mode.id}`} value={mode.id}>{mode.label}</option>
                            ))}
                        </select>
                    </label>
                ) : null}

                {selectedLaunchMode ? <ContextCard label="Mode Summary" value={selectedLaunchMode.description} /> : null}

                {selectedLaunchMode?.requiresPrompt ? (
                    <label style={{ display: "flex", flexDirection: "column", gap: 6, fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
                        Prompt
                        <textarea
                            value={launchPrompt}
                            onChange={(event) => setLaunchPrompt(event.target.value)}
                            placeholder={selectedLaunchMode.promptPlaceholder ?? "Enter a task or prompt."}
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
                    value={selectedAgent ? buildCodingAgentLaunchCommand(selectedAgent, selectedLaunchMode?.id ?? null, launchPrompt) || "Unavailable" : "Select an agent"}
                />

                {!selectedAgent?.available ? (
                    <ContextCard
                        label={`${actionLabel} Preview`}
                        value={installCommand || "No install command is defined for this runtime yet."}
                    />
                ) : selectedAgent?.readiness === "needs-setup" ? (
                    <ContextCard
                        label={`${actionLabel} Preview`}
                        value={installCommand || "No setup command is defined for this runtime yet."}
                    />
                ) : null}

                {selectedAgent?.requirements?.length ? <ContextCard label="Requirements" value={selectedAgent.requirements.join(" | ")} /> : null}
                {selectedAgent?.installHints?.length ? <ContextCard label="Setup Hints" value={selectedAgent.installHints.join(" | ")} /> : null}
                {error ? <EmptyPanel message={error} /> : null}
                {launchError ? <EmptyPanel message={launchError} /> : null}
                {installError ? <EmptyPanel message={installError} /> : null}
                {launchState === "success" && lastLaunchCommand ? <EmptyPanel message={`Launched: ${lastLaunchCommand}`} /> : null}
                {installState === "success" && lastInstallCommand ? <EmptyPanel message={`Install command sent: ${lastInstallCommand}`} /> : null}
                {confirmInstall && canInstall ? <EmptyPanel message={`This will send the ${actionLabel.toLowerCase()} command to the selected pane. Continue only if you trust the runtime source and want to ${actionLabel.toLowerCase()} it on this machine.`} /> : null}
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
                            if (canInstall) {
                                if (!confirmInstall) {
                                    setConfirmInstall(true);
                                    return;
                                }

                                void installSelectedAgent().then((ok) => {
                                    if (ok) {
                                        setConfirmInstall(false);
                                    }
                                });
                                return;
                            }

                            void launchSelectedAgent();
                        }}
                        disabled={canInstall
                            ? installState === "installing"
                            : !selectedAgent || !selectedAgent.available || launchState === "launching"}
                        style={{
                            padding: "var(--space-2) var(--space-4)",
                            borderRadius: "var(--radius-md)",
                            border: "1px solid var(--accent)",
                            background: "var(--accent)",
                            color: "var(--zorai-bg)",
                            cursor: canInstall
                                ? installState === "installing" ? "not-allowed" : "pointer"
                                : !selectedAgent || !selectedAgent.available || launchState === "launching" ? "not-allowed" : "pointer",
                            opacity: canInstall
                                ? installState === "installing" ? 0.6 : 1
                                : !selectedAgent || !selectedAgent.available || launchState === "launching" ? 0.6 : 1,
                            fontWeight: 700,
                        }}
                    >
                        {canInstall
                            ? installState === "installing"
                                ? `${actionLabel}...`
                                : confirmInstall
                                    ? `Are You Sure? ${actionLabel}`
                                    : `${actionLabel} in Pane`
                            : launchState === "launching"
                                ? "Launching..."
                                : selectedLaunchMode?.requiresPrompt
                                    ? "Launch Task in Pane"
                                    : "Launch in Pane"}
                    </button>
                </div>
            </div>
        </div>
    );
}