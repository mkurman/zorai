import { afterEach, describe, expect, it, vi } from "vitest";
import { useAgentMissionStore } from "@/lib/agentMissionStore";
import { resolveApprovalAtSource, shouldBypassApprovalPrompt } from "@/components/approvalResolution";
import { useUpdateStore } from "@/components/UpdatePrompt";
import { previewApproval } from "./developerTools";

const withBridge = (bridge: Record<string, unknown>) => {
  (globalThis as { window?: unknown }).window = { zorai: bridge };
};

afterEach(() => {
  delete (globalThis as { window?: unknown }).window;
  useAgentMissionStore.setState({ approvals: [], sessionAllowlist: {} });
  useUpdateStore.setState({ state: { phase: "hidden" }, manualCheck: "idle", simulation: null });
});

describe("developer tools fake approvals", () => {
  it("never write to the audit history or the session allowlist", () => {
    const dbUpsertAgentEvent = vi.fn(async () => undefined);
    withBridge({ dbUpsertAgentEvent });
    const before = useAgentMissionStore.getState().operationalEvents.length;
    const approval = previewApproval("routine", null);

    useAgentMissionStore.getState().upsertDaemonApproval(approval);
    expect(useAgentMissionStore.getState().approvals.map((entry) => entry.id)).toEqual([approval.id]);
    useAgentMissionStore.getState().resolveApproval(approval.id, "approved-session");

    expect(useAgentMissionStore.getState().approvals).toEqual([]);
    expect(useAgentMissionStore.getState().sessionAllowlist).toEqual({});
    expect(useAgentMissionStore.getState().operationalEvents.length).toBe(before);
    expect(dbUpsertAgentEvent).not.toHaveBeenCalled();
  });

  it("resolve locally without asking the daemon or a terminal to run anything", async () => {
    const agentResolveTaskApproval = vi.fn(async () => ({ ok: true }));
    const resolveManagedApproval = vi.fn(async () => ({ ok: true }));
    const approval = { ...previewApproval("high-impact", null), createdAt: 0, workspaceId: null, surfaceId: null, sessionId: null, status: "pending" as const, handledAt: null };

    await expect(resolveApprovalAtSource({ agentResolveTaskApproval, resolveManagedApproval }, approval, "approved-once")).resolves.toBe(true);
    expect(agentResolveTaskApproval).not.toHaveBeenCalled();
    expect(resolveManagedApproval).not.toHaveBeenCalled();
  });

  it("are always shown, even when yolo mode would auto-approve real ones", () => {
    const approval = { ...previewApproval("routine", null), createdAt: 0, workspaceId: null, surfaceId: null, sessionId: null, status: "pending" as const, handledAt: null };
    expect(shouldBypassApprovalPrompt(approval, "yolo", "yolo")).toBe(false);
  });

  it("attach the thread variant to the current thread so it shows inline there and as a banner elsewhere", () => {
    expect(previewApproval("thread", "thread_1").threadId).toBe("thread_1");
    expect(previewApproval("routine", "thread_1").threadId).toBeNull();
    expect(previewApproval("high-impact", "thread_1").riskLevel).toBe("critical");
  });
});

describe("developer tools fake update", () => {
  it("walks the real prompt through install and restart without calling the installer or restarting", async () => {
    vi.useFakeTimers();
    const appUpdateInstall = vi.fn();
    const appUpdateRestart = vi.fn();
    withBridge({ appUpdateInstall, appUpdateRestart });

    useUpdateStore.getState().simulate("success", "1.0.41");
    expect(useUpdateStore.getState().state).toEqual({ phase: "available", currentVersion: "1.0.41", latestVersion: "99.0.0" });
    const install = useUpdateStore.getState().install();
    expect(useUpdateStore.getState().state.phase).toBe("installing");
    await vi.advanceTimersByTimeAsync(1500);
    await install;
    expect(useUpdateStore.getState().state.phase).toBe("installed");
    useUpdateStore.getState().restart();

    expect(useUpdateStore.getState().state.phase).toBe("hidden");
    expect(appUpdateInstall).not.toHaveBeenCalled();
    expect(appUpdateRestart).not.toHaveBeenCalled();
    vi.useRealTimers();
  });

  it("can show the failure state without anything having failed", async () => {
    vi.useFakeTimers();
    useUpdateStore.getState().simulate("failure", "1.0.41");
    const install = useUpdateStore.getState().install();
    await vi.advanceTimersByTimeAsync(1500);
    await install;
    expect(useUpdateStore.getState().state.phase).toBe("failed");
    vi.useRealTimers();
  });
});
