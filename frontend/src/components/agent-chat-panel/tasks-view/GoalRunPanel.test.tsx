import { Children, isValidElement, type ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { GoalRunPanel } from "./GoalRunPanel";

function resolveTree(node: ReactNode): ReactNode {
  if (node == null || typeof node === "boolean" || typeof node === "string" || typeof node === "number") {
    return node;
  }
  if (Array.isArray(node)) {
    return node.map((child) => resolveTree(child));
  }
  if (!isValidElement(node)) {
    return node;
  }
  if (typeof node.type === "function") {
    return resolveTree(node.type(node.props));
  }

  return {
    ...node,
    props: {
      ...node.props,
      children: Children.toArray(node.props.children).map((child) => resolveTree(child)),
    },
  };
}

function elementText(node: ReactNode): string {
  if (node == null || typeof node === "boolean") {
    return "";
  }
  if (typeof node === "string" || typeof node === "number") {
    return String(node);
  }
  if (Array.isArray(node)) {
    return node.map((child) => elementText(child)).join("");
  }
  if (!isValidElement(node)) {
    return "";
  }
  return elementText(node.props.children);
}

function findButton(node: ReactNode, label: string): any {
  if (node == null || typeof node === "boolean" || typeof node === "string" || typeof node === "number") {
    return null;
  }
  if (Array.isArray(node)) {
    for (const child of node) {
      const found = findButton(child, label);
      if (found) {
        return found;
      }
    }
    return null;
  }
  if (!isValidElement(node)) {
    return null;
  }
  const accessibleName = `${elementText(node.props.children)} ${node.props["aria-label"] ?? ""} ${node.props.title ?? ""}`;
  if (node.type === "button" && accessibleName.includes(label)) {
    return node;
  }
  return findButton(node.props.children, label);
}

describe("GoalRunPanel", () => {
  it("renders a refresh button for the goal run list", () => {
    const onRefreshGoalRuns = vi.fn();
    const resolved = resolveTree(
      <GoalRunPanel
        goalRunsSupported
        newGoalPrompt=""
        setNewGoalPrompt={vi.fn()}
        newGoalTitle=""
        setNewGoalTitle={vi.fn()}
        newGoalSessionId=""
        setNewGoalSessionId={vi.fn()}
        goalStartError={null}
        onAddGoalRun={vi.fn()}
        onRefreshGoalRuns={onRefreshGoalRuns}
        activeGoalRuns={[]}
        historicalGoalRuns={[]}
        completedGoalRuns={[]}
        selectedGoalRun={null}
        selectedGoalRunAgentRuns={[]}
        selectedGoalRunId={null}
        goalActionId={null}
        onSelectGoalRun={vi.fn()}
        onChangeGoalRunState={vi.fn()}
        historyFailureQuery=""
        setHistoryFailureQuery={vi.fn()}
        historyMinReplans={0}
        setHistoryMinReplans={vi.fn()}
        historyMinChildTasks={0}
        setHistoryMinChildTasks={vi.fn()}
        historyMinApprovals={0}
        setHistoryMinApprovals={vi.fn()}
        historyMinDurationMinutes={0}
        setHistoryMinDurationMinutes={vi.fn()}
        totalGoalRunCount={0}
      />,
    );

    const refreshButton = findButton(resolved, "Refresh");
    expect(refreshButton).toBeTruthy();

    refreshButton.props.onClick();

    expect(onRefreshGoalRuns).toHaveBeenCalledTimes(1);
  });
});
