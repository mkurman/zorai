import { allLeafIds } from "../../lib/bspTree";
import { discoverCodingAgents } from "./bridge";
import { useCodingAgentsStore } from "./store";
import { useWorkspaceStore } from "../../lib/workspaceStore";

function resolveLaunchModeId(agentId: string | null, modeRef: unknown) {
    const agent = useCodingAgentsStore.getState().agents.find((entry) => entry.id === agentId);
    const ref = String(modeRef ?? "").trim().toLowerCase();
    if (!agent) {
        return null;
    }

    if (!ref) {
        return useCodingAgentsStore.getState().selectedLaunchModeId ?? agent.launchModes?.find((mode) => mode.recommended)?.id ?? agent.launchModes?.[0]?.id ?? null;
    }

    return agent.launchModes?.find((mode) => mode.id.toLowerCase() === ref || mode.label.trim().toLowerCase() === ref)?.id ?? null;
}

function resolveAgentId(agentRef: unknown) {
    const ref = String(agentRef ?? "").trim().toLowerCase();
    const agents = useCodingAgentsStore.getState().agents;
    if (!ref) {
        return useCodingAgentsStore.getState().selectedAgentId
            ?? agents.find((agent) => agent.available)?.id
            ?? null;
    }

    return agents.find((agent) => agent.id === ref)
        ?.id ?? agents.find((agent) => agent.label.trim().toLowerCase() === ref)
            ?.id ?? agents.find((agent) => agent.executables.some((entry) => entry.trim().toLowerCase() === ref))
            ?.id ?? null;
}

function resolveWorkspaceSurfacePane(args: Record<string, unknown>) {
    const workspaceStore = useWorkspaceStore.getState();
    const workspaceRef = String(args.workspace ?? "").trim().toLowerCase();
    const surfaceRef = String(args.surface ?? "").trim().toLowerCase();
    const paneRef = String(args.pane ?? "").trim().toLowerCase();

    const workspace = workspaceRef
        ? workspaceStore.workspaces.find((entry) => entry.id.toLowerCase() === workspaceRef || entry.name.trim().toLowerCase() === workspaceRef)
        : workspaceStore.activeWorkspace();
    if (!workspace) {
        return null;
    }

    const surface = surfaceRef
        ? workspace.surfaces.find((entry) => entry.id.toLowerCase() === surfaceRef || entry.name.trim().toLowerCase() === surfaceRef)
        : workspace.surfaces.find((entry) => entry.id === workspace.activeSurfaceId) ?? workspace.surfaces[0];
    if (!surface) {
        return null;
    }

    const paneIds = allLeafIds(surface.layout);
    const paneId = paneRef
        ? paneIds.find((entry) => entry.toLowerCase() === paneRef || (surface.paneNames[entry] ?? "").trim().toLowerCase() === paneRef)
        : surface.activePaneId ?? paneIds[0] ?? null;
    if (!paneId) {
        return null;
    }

    return {
        workspaceId: workspace.id,
        surfaceId: surface.id,
        paneId,
    };
}

let registered = false;

export function registerCodingAgentsPlugin() {
    const pluginApi = window.ZoraiApi;
    if (registered || typeof window === "undefined" || !pluginApi) {
        return;
    }

    if (pluginApi.getPlugins().includes("coding-agents")) {
        registered = true;
        return;
    }

    pluginApi.registerPlugin({
        id: "coding-agents",
        name: "Coding Agents",
        version: "0.9.65",
        assistantTools: [
            {
                type: "function",
                function: {
                    name: "coding_agents_list_available",
                    description: "List locally available coding-agent CLIs discovered on PATH, including availability, version, and executable path.",
                    parameters: {
                        type: "object",
                        properties: {},
                    },
                },
            },
            {
                type: "function",
                function: {
                    name: "coding_agents_launch",
                    description: "Prepare a discovered coding-agent CLI in a selected terminal pane for user-initiated launch. Accepts optional workspace, surface, and pane by id or name.",
                    parameters: {
                        type: "object",
                        properties: {
                            agent: {
                                type: "string",
                                description: "Coding agent id, label, or executable name such as claude, codex, opencode, kimi, aider, or goose.",
                            },
                            mode: {
                                type: "string",
                                description: "Optional launch mode id or label such as interactive, one-shot, direct-agent, or gateway.",
                            },
                            prompt: {
                                type: "string",
                                description: "Optional inline task prompt for launch modes that require one, such as one-shot or print mode.",
                            },
                            workspace: {
                                type: "string",
                                description: "Optional workspace id or name. Defaults to the active workspace.",
                            },
                            surface: {
                                type: "string",
                                description: "Optional surface id or name. Defaults to the active surface.",
                            },
                            pane: {
                                type: "string",
                                description: "Optional pane id or pane name. Defaults to the active pane on the selected surface.",
                            },
                        },
                    },
                },
            },
        ],
        assistantToolExecutors: {
            coding_agents_list_available: async (call) => {
                const agents = await discoverCodingAgents();
                const lines = agents.map((agent) => {
                    const status = agent.available ? "available" : "unavailable";
                    const details = [
                        `${agent.label} [${agent.id}] - ${status} - ${agent.kind} - ${agent.readiness ?? "missing"}`,
                        `  executable: ${agent.executable ?? agent.executables[0] ?? "unknown"}`,
                        `  version: ${agent.version ?? "not detected"}`,
                        `  path: ${agent.path ?? agent.error ?? "not found"}`,
                        `  modes: ${(agent.launchModes ?? []).map((mode) => mode.id).join(", ") || "interactive"}`,
                        `  capabilities: ${(agent.capabilities ?? []).join(", ") || "none declared"}`,
                        ...(agent.runtimeNotes?.length ? [`  runtime: ${agent.runtimeNotes.join(" | ")}`] : []),
                        ...(agent.gatewayLabel ? [`  gateway: ${agent.gatewayLabel} (${agent.gatewayReachable ? "reachable" : "not reachable"})`] : []),
                    ];
                    return details.join("\n");
                });

                return {
                    toolCallId: call.id,
                    name: call.function.name,
                    content: lines.join("\n") || "No coding agents are registered.",
                };
            },
            coding_agents_launch: async (call, args) => {
                await useCodingAgentsStore.getState().refreshAgents();

                const target = resolveWorkspaceSurfacePane(args);
                if (!target) {
                    return {
                        toolCallId: call.id,
                        name: call.function.name,
                        content: "Error: Could not resolve the requested workspace, surface, or pane.",
                    };
                }

                const agentId = resolveAgentId(args.agent);
                if (!agentId) {
                    return {
                        toolCallId: call.id,
                        name: call.function.name,
                        content: "Error: Could not resolve a coding agent. Use coding_agents_list_available first.",
                    };
                }

                const store = useCodingAgentsStore.getState();
                store.setSelectedAgentId(agentId);
                store.setSelectedWorkspaceId(target.workspaceId);
                store.setSelectedSurfaceId(target.surfaceId);
                store.setSelectedPaneId(target.paneId);
                store.setSelectedLaunchModeId(resolveLaunchModeId(agentId, args.mode));
                store.setLaunchPrompt(String(args.prompt ?? ""));

                const nextState = useCodingAgentsStore.getState();
                const launchedAgent = nextState.agents.find((agent) => agent.id === agentId);
                const selectedMode = launchedAgent?.launchModes?.find((mode) => mode.id === nextState.selectedLaunchModeId)
                    ?? launchedAgent?.launchModes?.find((mode) => mode.recommended)
                    ?? launchedAgent?.launchModes?.[0];
                return {
                    toolCallId: call.id,
                    name: call.function.name,
                    content: `Prepared ${launchedAgent?.label ?? agentId} for pane [${target.paneId}] on surface [${target.surfaceId}] in workspace [${target.workspaceId}]${selectedMode ? ` using ${selectedMode.label} mode` : ""}. Launch must be initiated by the user from the UI or through the managed terminal command tool.`,
                };
            },
        },
        commands: {
            refreshDiscovery: () => {
                void useCodingAgentsStore.getState().refreshAgents();
            },
            launchSelected: () => {
                void useCodingAgentsStore.getState().launchSelectedAgent();
            },
        },
        onLoad: () => {
            void useCodingAgentsStore.getState().refreshAgents();
        },
    });

    registered = true;
}
