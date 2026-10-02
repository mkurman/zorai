import { allLeafIds, findLeaf } from "./bspTree";
import { getBridge } from "./bridge";
import type { Workspace } from "./types";
import { findPaneIdsForSession } from "./workspace-store/helpers";
import { useWorkspaceStore } from "./workspaceStore";

export type AgentWorkspaceProvision = {
    workspaceId: string;
    surfaceId: string;
    paneId: string;
    sessionId: string | null;
    cwd: string | null;
};

function findWorkspaceById(workspaceId: string): Workspace | null {
    return useWorkspaceStore.getState().workspaces.find((workspace) => workspace.id === workspaceId) ?? null;
}

export function resolvePaneSessionId(paneId: string): string | null {
    for (const workspace of useWorkspaceStore.getState().workspaces) {
        for (const surface of workspace.surfaces) {
            if (!allLeafIds(surface.layout).includes(paneId)) {
                continue;
            }
            const leafSessionId = findLeaf(surface.layout, paneId)?.sessionId ?? null;
            const panelSessionId = surface.canvasPanels.find((panel) => panel.paneId === paneId)?.sessionId ?? null;
            return panelSessionId ?? leafSessionId;
        }
    }
    return null;
}

async function waitForTerminalReady(paneId: string, timeoutMs = 4000): Promise<string | null> {
    const bridge = getBridge();
    const onTerminalEvent = bridge?.onTerminalEvent;
    if (!onTerminalEvent) {
        return null;
    }

    return await new Promise((resolve) => {
        let settled = false;
        const finish = (value: string | null) => {
            if (settled) {
                return;
            }
            settled = true;
            window.clearTimeout(timer);
            unsubscribe?.();
            resolve(value);
        };

        const unsubscribe = onTerminalEvent((event: any) => {
            if (event?.paneId !== paneId) {
                return;
            }

            if (event.type === "ready" && typeof event.sessionId === "string" && event.sessionId) {
                finish(event.sessionId);
                return;
            }

            if (event.type === "error" || event.type === "session-exited") {
                finish(null);
            }
        });

        const timer = window.setTimeout(() => {
            finish(resolvePaneSessionId(paneId));
        }, timeoutMs);
    });
}

async function startPaneSession(opts: {
    paneId: string;
    workspaceId: string;
    cwd?: string | null;
    sessionId?: string | null;
}): Promise<string | null> {
    const bridge = getBridge();
    if (!bridge?.startTerminalSession) {
        return opts.sessionId ?? resolvePaneSessionId(opts.paneId);
    }

    const response = await bridge.startTerminalSession({
        paneId: opts.paneId,
        sessionId: opts.sessionId ?? undefined,
        workspaceId: opts.workspaceId,
        cwd: opts.cwd ?? undefined,
    });
    const immediateSessionId = typeof response?.sessionId === "string" && response.sessionId
        ? response.sessionId
        : opts.sessionId ?? resolvePaneSessionId(opts.paneId);
    const sessionId = immediateSessionId ?? await waitForTerminalReady(opts.paneId);
    if (sessionId) {
        useWorkspaceStore.getState().setPaneSessionId(opts.paneId, sessionId);
    }
    return sessionId;
}

export async function provisionTerminalPaneInWorkspace(opts: {
    workspaceId: string;
    paneName: string;
    cwd?: string | null;
    sessionId?: string | null;
    reusePrimaryPane?: boolean;
}): Promise<AgentWorkspaceProvision | null> {
    const store = useWorkspaceStore.getState();
    let workspace = findWorkspaceById(opts.workspaceId);
    if (!workspace) {
        return null;
    }

    if (opts.cwd) {
        store.updateWorkspaceCwd(workspace.id, opts.cwd);
        workspace = findWorkspaceById(workspace.id);
    }

    let surface = workspace?.surfaces.find((entry) => entry.layoutMode === "canvas")
        ?? null;
    if (!surface) {
        const surfaceId = store.createSurface(workspace?.id, { layoutMode: "canvas", makeActive: false });
        if (!surfaceId) {
            return null;
        }
        store.renameSurface(surfaceId, "Agent Workspace");
        workspace = findWorkspaceById(workspace?.id ?? opts.workspaceId);
        surface = workspace?.surfaces.find((entry) => entry.id === surfaceId) ?? null;
    }
    if (!workspace || !surface) {
        return null;
    }

    let paneId: string | null = null;
    const existingPaneIds = allLeafIds(surface.layout);
    if (opts.reusePrimaryPane && existingPaneIds.length === 1) {
        const candidatePaneId = existingPaneIds[0];
        const existingSessionId = resolvePaneSessionId(candidatePaneId);
        if (!existingSessionId) {
            paneId = candidatePaneId;
        }
    }

    if (!paneId) {
        paneId = store.createCanvasPanel(surface.id, {
            paneName: opts.paneName,
            sessionId: opts.sessionId ?? null,
        });
    }
    if (!paneId) {
        return null;
    }

    store.setPaneName(paneId, opts.paneName);
    const sessionId = await startPaneSession({
        paneId,
        workspaceId: workspace.id,
        cwd: opts.cwd ?? workspace.cwd ?? null,
        sessionId: opts.sessionId ?? null,
    });

    return {
        workspaceId: workspace.id,
        surfaceId: surface.id,
        paneId,
        sessionId,
        cwd: opts.cwd ?? workspace.cwd ?? null,
    };
}

export function closePanesForSession(sessionId: string): void {
    if (!sessionId) {
        return;
    }
    const store = useWorkspaceStore.getState();
    for (const paneId of findPaneIdsForSession(store.workspaces, sessionId)) {
        store.closePane(paneId, { captureTranscript: false });
    }
}

/**
 * Create a sandboxed workspace directory for an agent run.
 * Uses $HOME to build an absolute path. Returns null if HOME is unknown
 * (the shell will expand $HOME at runtime via the mkdir command).
 */
function buildWorkspaceDirSlug(title: string): string {
    const slug = title
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "-")
        .replace(/^-|-$/g, "")
        .slice(0, 40);
    const ts = Math.floor(Date.now() / 1000);
    return `${slug}-${ts}`;
}

export async function provisionAgentWorkspaceTerminals(opts: {
    title: string;
    cwd?: string | null;
    sandbox?: boolean;
}): Promise<{
    workspaceId: string;
    surfaceId: string;
    coordinatorPaneId: string;
    coordinatorSessionId: string | null;
    workPaneId: string;
    workSessionId: string | null;
    cwd: string | null;
    workspaceDir: string | null;
} | null> {
    const store = useWorkspaceStore.getState();
    const trimmedTitle = opts.title.trim() || "Agent Run";
    const workspaceName = `Agent - ${trimmedTitle.slice(0, 40)}`;

    // Create a dedicated workspace directory for sandboxed agent runs
    const shouldSandbox = opts.sandbox !== false; // default true
    const workspaceDirSlug = shouldSandbox ? buildWorkspaceDirSlug(trimmedTitle) : null;
    // Use $HOME expansion — the actual absolute path will be resolved in the shell
    let workspaceDir: string | null = workspaceDirSlug
        ? `$HOME/.zorai/workspaces/${workspaceDirSlug}`
        : null;

    // Don't use workspaceDir as cwd yet — it doesn't exist. Use opts.cwd for session start.
    const effectiveCwd = opts.cwd ?? null;

    const workspaceId = store.createWorkspace(workspaceName, {
        layoutMode: "canvas",
        makeActive: false,
        agentOwned: true,
    });
    const workspace = findWorkspaceById(workspaceId);
    const surfaceId = workspace?.activeSurfaceId ?? workspace?.surfaces[0]?.id ?? null;
    if (!workspace || !surfaceId) {
        return null;
    }

    store.renameSurface(surfaceId, "Agent Workspace");
    if (effectiveCwd) {
        store.updateWorkspaceCwd(workspaceId, effectiveCwd);
    }

    const coordinator = await provisionTerminalPaneInWorkspace({
        workspaceId,
        paneName: "Coordinator",
        cwd: effectiveCwd ?? workspace.cwd ?? null,
        reusePrimaryPane: true,
    });
    if (!coordinator) {
        return null;
    }

    // Create the workspace directory and cd into it via direct terminal input
    // Using $HOME expansion (not ~) so it works inside quotes
    if (workspaceDir && coordinator.sessionId) {
        const bridge = getBridge();
        try {
            // Write directly to the terminal — not via managed command to avoid approval
            await bridge?.sendTerminalInput?.(coordinator.paneId, `mkdir -p ${workspaceDir} && cd ${workspaceDir}\n`);
            // Small delay to let the cd complete
            await new Promise((r) => setTimeout(r, 500));
        } catch {
            // Non-fatal
        }
    }

    const work = await provisionTerminalPaneInWorkspace({
        workspaceId,
        paneName: "Work",
        cwd: effectiveCwd ?? workspace.cwd ?? null,
    });
    if (!work) {
        return null;
    }

    // CD the work terminal into the workspace directory
    if (workspaceDir && work.sessionId) {
        const bridge = getBridge();
        try {
            await bridge?.sendTerminalInput?.(work.paneId, `mkdir -p ${workspaceDir} && cd ${workspaceDir}\n`);
            await new Promise((r) => setTimeout(r, 500));
        } catch {
            // Non-fatal
        }
    }

    return {
        workspaceId,
        surfaceId: coordinator.surfaceId,
        coordinatorPaneId: coordinator.paneId,
        coordinatorSessionId: coordinator.sessionId,
        workPaneId: work.paneId,
        workSessionId: work.sessionId,
        cwd: effectiveCwd,
        workspaceDir,
    };
}
