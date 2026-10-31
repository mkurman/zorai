import type { AgentMessage } from "@/lib/agentStore";
import type {
  SpawnedAgentTree,
  SpawnedAgentTreeNode,
  SpawnedAgentTreeSource,
} from "@/lib/spawnedAgentTree";

export function formatThreadBudgetExceededNotice(threadId: string): string {
  return `Thread budget exceeded for ${threadId}. Review completed work here; continue from the parent thread or respawn with a larger child budget.`;
}

export function isBudgetExceededStatus(status: string | null | undefined): boolean {
  return status === "budget_exceeded";
}

export function isThreadBudgetExceededSystemContent(content: string): boolean {
  return content.trimStart().startsWith("Task budget exceeded for this thread");
}

export function isThreadBudgetExtendedSystemContent(content: string): boolean {
  return content.trimStart().startsWith("Execution budget extended by");
}

export function isSubagentBudgetExceededSystemContent(content: string): boolean {
  const text = content.trimStart();
  return /spawned thread/i.test(text) && /exhausted its execution budget/i.test(text);
}

function nodeLocksThread<T extends SpawnedAgentTreeSource>(
  threadId: string,
  node: SpawnedAgentTreeNode<T>,
): boolean {
  if (node.item.thread_id === threadId && isBudgetExceededStatus(node.item.status)) {
    return true;
  }
  return node.children.some((child) => nodeLocksThread(threadId, child));
}

export function spawnedTreeLocksThread<T extends SpawnedAgentTreeSource>(
  threadId: string,
  tree: SpawnedAgentTree<T> | null,
): boolean {
  if (!tree) return false;
  if (tree.anchor && nodeLocksThread(threadId, tree.anchor)) return true;
  return tree.roots.some((root) => nodeLocksThread(threadId, root));
}

export function activeThreadBudgetExceededNotice<T extends SpawnedAgentTreeSource>(
  daemonThreadId: string | null | undefined,
  messages: readonly Pick<AgentMessage, "role" | "content">[],
  tree: SpawnedAgentTree<T> | null,
): string | null {
  if (!daemonThreadId) return null;
  const latestBudgetMessage = [...messages].reverse().find(
    (message) => message.role === "system"
      && (isThreadBudgetExceededSystemContent(message.content) || isThreadBudgetExtendedSystemContent(message.content)),
  );
  const lockedByMessage = Boolean(latestBudgetMessage && isThreadBudgetExceededSystemContent(latestBudgetMessage.content));
  if (!lockedByMessage && !spawnedTreeLocksThread(daemonThreadId, tree)) {
    return null;
  }
  return formatThreadBudgetExceededNotice(daemonThreadId);
}
