import { allLeafIds } from "../../lib/bspTree";
import { useWorkspaceStore } from "../../lib/workspaceStore";
import { discoverAITrainingProfiles } from "./bridge";
import { useAITrainingStore } from "./store";

function resolveProfileId(profileRef: unknown) {
    const ref = String(profileRef ?? "").trim().toLowerCase();
    const profiles = useAITrainingStore.getState().profiles;
    if (!ref) {
        return useAITrainingStore.getState().selectedProfileId
            ?? profiles.find((profile) => profile.available)?.id
            ?? profiles[0]?.id
            ?? null;
    }

    return profiles.find((profile) => profile.id === ref)?.id
        ?? profiles.find((profile) => profile.label.trim().toLowerCase() === ref)?.id
        ?? profiles.find((profile) => profile.executables.some((entry) => entry.trim().toLowerCase() === ref))?.id
        ?? null;
}

function resolveLaunchModeId(profileId: string | null, modeRef: unknown) {
    const profile = useAITrainingStore.getState().profiles.find((entry) => entry.id === profileId);
    const ref = String(modeRef ?? "").trim().toLowerCase();
    if (!profile) {
        return null;
    }

    if (!ref) {
        return useAITrainingStore.getState().selectedLaunchModeId ?? profile.launchModes?.find((mode) => mode.recommended)?.id ?? profile.launchModes?.[0]?.id ?? null;
    }

    return profile.launchModes?.find((mode) => mode.id.toLowerCase() === ref || mode.label.trim().toLowerCase() === ref)?.id ?? null;
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

export function registerAITrainingPlugin() {
    const pluginApi = window.ZoraiApi;
    if (registered || typeof window === "undefined" || !pluginApi) {
        return;
    }

    if (pluginApi.getPlugins().includes("ai-training")) {
        registered = true;
        return;
    }

    pluginApi.registerPlugin({
        id: "ai-training",
        name: "AI Training",
        version: "0.9.63",
        assistantTools: [
            {
                type: "function",
                function: {
                    name: "ai_training_list_available",
                    description: "List supported AI training runtimes and repository workflows for the current or selected workspace.",
                    parameters: {
                        type: "object",
                        properties: {
                            workspace: {
                                type: "string",
                                description: "Optional workspace id or name used for workspace-specific readiness checks.",
                            },
                        },
                    },
                },
            },
            {
                type: "function",
                function: {
                    name: "ai_training_launch",
                    description: "Prepare a supported AI training workflow in a selected terminal pane for user-initiated launch.",
                    parameters: {
                        type: "object",
                        properties: {
                            profile: {
                                type: "string",
                                description: "Profile id or label such as prime-verifiers, autoresearch, or autorl.",
                            },
                            mode: {
                                type: "string",
                                description: "Optional launch mode id or label such as lab-setup, train, evaluator, or eval-run.",
                            },
                            prompt: {
                                type: "string",
                                description: "Optional inline value for launch modes that need extra input, such as an environment name or description.",
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
            ai_training_list_available: async (call, args) => {
                const workspaceStore = useWorkspaceStore.getState();
                const workspaceRef = String(args.workspace ?? "").trim().toLowerCase();
                const workspace = workspaceRef
                    ? workspaceStore.workspaces.find((entry) => entry.id.toLowerCase() === workspaceRef || entry.name.trim().toLowerCase() === workspaceRef)
                    : workspaceStore.activeWorkspace();

                const profiles = await discoverAITrainingProfiles(workspace?.cwd ?? null);
                const lines = profiles.map((profile) => {
                    const details = [
                        `${profile.label} [${profile.id}] - ${profile.available ? "available" : "unavailable"} - ${profile.kind} - ${profile.readiness}`,
                        `  executable: ${profile.executable ?? profile.executables[0] ?? "unknown"}`,
                        `  version: ${profile.version ?? "not detected"}`,
                        `  path: ${profile.path ?? profile.error ?? "not found"}`,
                        `  workspace: ${workspace?.cwd ?? "not set"}`,
                        `  modes: ${(profile.launchModes ?? []).map((mode) => mode.id).join(", ") || "default"}`,
                        ...(profile.runtimeNotes?.length ? [`  runtime: ${profile.runtimeNotes.join(" | ")}`] : []),
                        ...(profile.checks?.length ? [`  checks: ${profile.checks.map((check) => `${check.scope}:${check.exists ? "yes" : "no"}:${check.path}`).join(" | ")}`] : []),
                    ];
                    return details.join("\n");
                });

                return {
                    toolCallId: call.id,
                    name: call.function.name,
                    content: lines.join("\n") || "No AI Training profiles are registered.",
                };
            },
            ai_training_launch: async (call, args) => {
                const target = resolveWorkspaceSurfacePane(args);
                if (!target) {
                    return {
                        toolCallId: call.id,
                        name: call.function.name,
                        content: "Error: Could not resolve the requested workspace, surface, or pane.",
                    };
                }

                await useAITrainingStore.getState().refreshProfiles(target.workspaceId);
                const profileId = resolveProfileId(args.profile);
                if (!profileId) {
                    return {
                        toolCallId: call.id,
                        name: call.function.name,
                        content: "Error: Could not resolve an AI Training profile. Use ai_training_list_available first.",
                    };
                }

                const store = useAITrainingStore.getState();
                store.setSelectedProfileId(profileId);
                store.setSelectedWorkspaceId(target.workspaceId);
                store.setSelectedSurfaceId(target.surfaceId);
                store.setSelectedPaneId(target.paneId);
                store.setSelectedLaunchModeId(resolveLaunchModeId(profileId, args.mode));
                store.setLaunchPrompt(String(args.prompt ?? ""));

                const nextState = useAITrainingStore.getState();
                const launchedProfile = nextState.profiles.find((profile) => profile.id === profileId);
                const selectedMode = launchedProfile?.launchModes?.find((mode) => mode.id === nextState.selectedLaunchModeId)
                    ?? launchedProfile?.launchModes?.find((mode) => mode.recommended)
                    ?? launchedProfile?.launchModes?.[0];
                return {
                    toolCallId: call.id,
                    name: call.function.name,
                    content: `Prepared ${launchedProfile?.label ?? profileId} for pane [${target.paneId}] on surface [${target.surfaceId}] in workspace [${target.workspaceId}]${selectedMode ? ` using ${selectedMode.label} mode` : ""}. Launch must be initiated by the user from the UI or through the managed terminal command tool.`,
                };
            },
        },
        commands: {
            refreshDiscovery: () => {
                void useAITrainingStore.getState().refreshProfiles();
            },
            launchSelected: () => {
                void useAITrainingStore.getState().launchSelectedProfile();
            },
        },
        onLoad: () => {
            void useAITrainingStore.getState().refreshProfiles();
        },
    });

    registered = true;
}
