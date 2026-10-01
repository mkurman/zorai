import { useEffect, useMemo, useState } from "react";
import type { AgentChatPanelRuntimeValue } from "@/components/agent-chat-panel/runtime/types";
import { SUB_AGENT_ROLE_PRESET_IDS } from "@/components/settings-panel/subAgentRolePresets";
import type { AgentProviderConfig } from "@/lib/agentStore/types";
import type { GoalAgentAssignment, StartGoalRunPayload } from "@/lib/goalRuns";

type GoalLaunchPanelProps = {
  runtime: AgentChatPanelRuntimeValue;
  supported: boolean;
  starting: boolean;
  message: string | null;
  onLaunch: (payload: StartGoalRunPayload) => void | Promise<void>;
  onClose: () => void;
};

const MAIN_ROLE_ID = "svarog";
const personaRoles = ["swarozyc", "radogost", "domowoj", "swietowit", "perun", "mokosh", "dazhbog", "rod"];
const reasoningOptions = ["none", "minimal", "low", "medium", "high", "xhigh", "max"];

export function GoalLaunchPanel({
  runtime,
  supported,
  starting,
  message,
  onLaunch,
  onClose,
}: GoalLaunchPanelProps) {
  const mainAssignment = useMemo(
    () => buildMainAssignment(runtime),
    [
      runtime.activeThread?.profileProvider,
      runtime.activeThread?.profileModel,
      runtime.activeThread?.profileReasoningEffort,
      runtime.agentSettings,
    ],
  );
  const [prompt, setPrompt] = useState("");
  const [saveAsDefaultPending, setSaveAsDefaultPending] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [assignments, setAssignments] = useState<GoalAgentAssignment[]>(() => [mainAssignment]);

  useEffect(() => {
    setAssignments((current) => {
      if (current.length > 0) return current;
      return [mainAssignment];
    });
  }, [mainAssignment]);

  const selectedAssignment = assignments[selectedIndex] ?? assignments[0] ?? mainAssignment;
  const providers = providerIds(runtime);
  const updateSelected = (update: Partial<GoalAgentAssignment>) => {
    setAssignments((current) => current.map((assignment, index) => (
      index === selectedIndex ? { ...assignment, ...update } : assignment
    )));
  };

  const addAgent = () => {
    const roleId = nextRoleId(assignments);
    setAssignments((current) => [
      ...current,
      {
        ...mainAssignment,
        role_id: roleId,
        inherit_from_main: false,
      },
    ]);
    setSelectedIndex(assignments.length);
  };

  const removeSelectedAgent = () => {
    if (assignments.length <= 1) return;
    setAssignments((current) => current.filter((_, index) => index !== selectedIndex));
    setSelectedIndex((current) => Math.max(0, Math.min(current, assignments.length - 2)));
  };

  const launch = () => {
    const goal = prompt.trim();
    if (!goal || !supported || starting) return;
    void onLaunch({
      goal,
      title: null,
      priority: null,
      threadId: null,
      sessionId: null,
      launchAssignments: assignments.length > 0 ? assignments : [mainAssignment],
      requiresApproval: true,
    });
  };

  return (
    <section className="zorai-goal-launch" aria-label="Start goal">
      <div className="zorai-goal-launch__header">
        <div>
          <div className="zorai-section-label">New Goal</div>
          <h2>Start goal</h2>
        </div>
        <button type="button" className="zorai-ghost-button" onClick={onClose}>Close</button>
      </div>

      <div className="zorai-goal-launch__content">
        <section className="zorai-goal-launch__section zorai-goal-launch__prompt">
          <div className="zorai-section-label">Prompt</div>
          <div className="zorai-goal-launch__section-body">
            <div className="zorai-goal-launch__accent">Goal prompt</div>
            <textarea
              className="zorai-goal-launch__textarea"
              value={prompt}
              onChange={(event) => setPrompt(event.target.value)}
              placeholder="Describe the goal, constraints, and acceptance criteria..."
              rows={3}
            />
          </div>
        </section>

        <section className="zorai-goal-launch__section zorai-goal-launch__main-agent">
          <div className="zorai-section-label">Main Agent</div>
          <div className="zorai-goal-launch__section-body">
            <div className="zorai-goal-launch__accent">Main model</div>
            <span>Provider: <strong>{mainAssignment.provider}</strong></span>
            <span>Model: <strong>{mainAssignment.model}</strong> Reasoning: <strong>{mainAssignment.reasoning_effort || "none"}</strong></span>
            <span>Preset source: <strong>{presetSourceLabel(runtime)}</strong></span>
            <span>Save as default: <strong>{saveAsDefaultPending ? "pending" : "off"}</strong></span>
          </div>
        </section>

        <section className="zorai-goal-launch__section zorai-goal-launch__roster">
          <div className="zorai-section-label">Role Assignments</div>
          <div className="zorai-goal-launch__section-body">
            <div className="zorai-goal-launch__accent">Agent roster</div>
            <div className="zorai-goal-launch__assignments">
              {assignments.map((assignment, index) => (
                <button
                  type="button"
                  key={`${assignment.role_id}-${index}`}
                  className={["zorai-goal-launch__assignment", index === selectedIndex ? "zorai-goal-launch__assignment--active" : ""].filter(Boolean).join(" ")}
                  onClick={() => setSelectedIndex(index)}
                >
                  <span>{index === selectedIndex ? "> " : ""}{assignment.role_id}: {assignment.provider} / {assignment.model} / {assignment.reasoning_effort || "none"} ({assignment.inherit_from_main ? "inherits main" : "custom"})</span>
                  <span>selected</span>
                </button>
              ))}
            </div>

            <div className="zorai-goal-launch__editor">
              <select value={selectedAssignment.role_id} onChange={(event) => updateSelected({ role_id: event.target.value })}>
                {[...SUB_AGENT_ROLE_PRESET_IDS, ...personaRoles].map((role) => <option key={role} value={role}>{role}</option>)}
              </select>
              <select value={selectedAssignment.provider} onChange={(event) => updateSelected({ provider: event.target.value })}>
                {providers.map((provider) => <option key={provider} value={provider}>{provider}</option>)}
              </select>
              <input value={selectedAssignment.model} onChange={(event) => updateSelected({ model: event.target.value })} aria-label="Model" />
              <select value={selectedAssignment.reasoning_effort || "none"} onChange={(event) => updateSelected({ reasoning_effort: event.target.value === "none" ? null : event.target.value })}>
                {reasoningOptions.map((effort) => <option key={effort} value={effort}>{effort}</option>)}
              </select>
              <label className="zorai-goal-launch__toggle">
                <input type="checkbox" checked={selectedAssignment.inherit_from_main} onChange={(event) => updateSelected({ inherit_from_main: event.target.checked })} />
                Inherit main
              </label>
            </div>
          </div>
        </section>
      </div>

      <footer className="zorai-goal-launch__footer">
        <button type="button" className="zorai-primary-button" onClick={launch} disabled={!supported || !prompt.trim() || starting}>
          {starting ? "Starting..." : "Start goal"}
        </button>
        <button type="button" className="zorai-ghost-button" onClick={addAgent}>Add agent</button>
        <button type="button" className="zorai-ghost-button" onClick={removeSelectedAgent} disabled={assignments.length <= 1}>
          Remove agent
        </button>
        <button type="button" className="zorai-ghost-button" onClick={() => setSaveAsDefaultPending((current) => !current)}>
          {saveAsDefaultPending ? "Default pending" : "Save as default"}
        </button>
      </footer>
      {message ? <div className="zorai-inline-note">{message}</div> : null}
    </section>
  );
}

function buildMainAssignment(runtime: AgentChatPanelRuntimeValue): GoalAgentAssignment {
  const provider = runtime.activeThread?.profileProvider || runtime.agentSettings.active_provider || "openai";
  const providerConfig = runtime.agentSettings[provider as keyof typeof runtime.agentSettings] as AgentProviderConfig | undefined;
  const model = runtime.activeThread?.profileModel || providerConfig?.custom_model_name || providerConfig?.model || "";
  const reasoning = runtime.activeThread?.profileReasoningEffort || runtime.agentSettings.reasoning_effort || null;
  return {
    role_id: MAIN_ROLE_ID,
    enabled: true,
    provider,
    model,
    reasoning_effort: reasoning === "none" ? null : reasoning,
    inherit_from_main: false,
  };
}

function providerIds(runtime: AgentChatPanelRuntimeValue): string[] {
  const providers = Object.entries(runtime.agentSettings)
    .filter(([, value]) => value && typeof value === "object" && "model" in value)
    .map(([provider]) => provider);
  if (!providers.includes(runtime.agentSettings.active_provider)) providers.unshift(runtime.agentSettings.active_provider);
  return providers;
}

function presetSourceLabel(runtime: AgentChatPanelRuntimeValue): string {
  return runtime.activeThread?.profileProvider ? "Active thread profile" : "Main agent inheritance";
}

function nextRoleId(assignments: GoalAgentAssignment[]): string {
  for (const role of SUB_AGENT_ROLE_PRESET_IDS) {
    if (!assignments.some((assignment) => assignment.role_id === role)) return role;
  }
  let suffix = assignments.length + 1;
  while (assignments.some((assignment) => assignment.role_id === `specialist_${suffix}`)) suffix += 1;
  return `specialist_${suffix}`;
}
