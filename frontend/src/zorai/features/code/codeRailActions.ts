/**
 * Tiny event bus bridging `CodeRail` (rendered by `ZoraiShell.renderRail`) and
 * `CodeView` (rendered by `ZoraiShell.renderMain`). They are siblings — there
 * is no shared ancestor that owns code-tab state, and prop threading through
 * `ZoraiShell` for a single tab is invasive. Subscribers are invoked with the
 * action descriptor and are expected to no-op when no Code view is mounted.
 */

export type CodeRailAction =
  | { kind: "open-file" }
  | { kind: "open-folder" }
  | { kind: "open-recent"; root: string }
  | { kind: "open-settings" };

export type CodeRailActionHandler = (action: CodeRailAction) => void;

const listeners = new Set<CodeRailActionHandler>();

export function emitCodeRailAction(action: CodeRailAction): void {
  for (const handler of [...listeners]) {
    try {
      handler(action);
    } catch {
      // Subscriber failures must not break sibling handlers.
    }
  }
}

export function subscribeCodeRailActions(handler: CodeRailActionHandler): () => void {
  listeners.add(handler);
  return () => {
    listeners.delete(handler);
  };
}

/** Test-only helper: clear registered handlers between test cases. */
export function __resetCodeRailActionsForTests(): void {
  listeners.clear();
}
