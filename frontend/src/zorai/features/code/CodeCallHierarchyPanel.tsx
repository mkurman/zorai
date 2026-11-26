import { useEffect, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { lspUriToRelativePath } from "./codeLspEdits";
import { callHierarchyChildren, type CallDirection, type CallHierarchyItem } from "./lspProviders";

type LspItem = CallHierarchyItem;
type Node = { key: string; item: LspItem; children: Node[] | null; open: boolean };
type Target = { root: string; path: string; language: string; line: number; column: number };

function lspCall(target: Target, method: "prepareCallHierarchy" | "incomingCalls" | "outgoingCalls", extra?: Record<string, unknown>) {
  return getBridge()?.workspaceLspRequest?.(target.root, target.path, target.language, method, { line: target.line - 1, character: target.column - 1 }, extra) ?? Promise.resolve(null);
}

async function loadCalls(target: Target, direction: CallDirection, item: LspItem, key: string): Promise<Node[]> {
  const response = await lspCall(target, direction === "incoming" ? "incomingCalls" : "outgoingCalls", { item }).catch(() => null);
  return callHierarchyChildren(response?.result, direction).map((child, index) => ({ key: `${key}/${index}`, item: child, children: null, open: false }));
}

export function CodeCallHierarchyPanel({ root, path, language, line, column, onOpen, onClose }: {
  root: string;
  path: string;
  language: string;
  line: number;
  column: number;
  onOpen: (path: string, line: number, column: number) => void;
  onClose: () => void;
}) {
  const [direction, setDirection] = useState<CallDirection>("incoming");
  const [tree, setTree] = useState<Node[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const target = { root, path, language, line, column };
    setTree(null);
    setError(null);
    void lspCall(target, "prepareCallHierarchy").then(async (response) => {
      const items = Array.isArray(response?.result) ? (response.result as LspItem[]) : [];
      if (items.length === 0) {
        if (!cancelled) setError(response && (response as { unsupported?: boolean }).unsupported ? "This language server has no call hierarchy." : "No callable symbol at the cursor.");
        return;
      }
      const roots = await Promise.all(items.map(async (item, index) => ({ key: String(index), item, children: await loadCalls(target, direction, item, String(index)), open: true })));
      if (!cancelled) setTree(roots);
    }).catch((reason) => { if (!cancelled) setError(reason instanceof Error ? reason.message : String(reason)); });
    return () => { cancelled = true; };
  }, [direction, root, path, language, line, column]);

  const toggle = async (target: Node) => {
    const children = target.children ?? await loadCalls({ root, path, language, line, column }, direction, target.item, target.key);
    const update = (nodes: Node[]): Node[] => nodes.map((node) => (node.key === target.key ? { ...node, children, open: !node.open } : { ...node, children: node.children ? update(node.children) : null }));
    setTree((current) => (current ? update(current) : current));
  };
  const open = (item: LspItem) => {
    const target = lspUriToRelativePath(item.uri, root);
    const start = (item.selectionRange ?? item.range).start;
    if (target) onOpen(target, start.line + 1, start.character + 1);
  };
  const render = (nodes: Node[], depth: number) => nodes.map((node) => (
    <div key={node.key}>
      <div className="zorai-call-hierarchy__row" style={{ paddingLeft: 8 + depth * 14 }}>
        <button type="button" className="zorai-call-hierarchy__twisty" aria-label={node.open ? "Collapse" : "Expand"} onClick={() => void toggle(node)}>{node.open ? "▾" : "▸"}</button>
        <button type="button" className="zorai-call-hierarchy__item" onClick={() => open(node.item)}>
          <strong>{node.item.name}</strong>
          <span>{node.item.detail ?? lspUriToRelativePath(node.item.uri, root) ?? ""}</span>
        </button>
      </div>
      {node.open && node.children ? (node.children.length === 0 && depth > 0 ? <div className="zorai-call-hierarchy__none" style={{ paddingLeft: 30 + depth * 14 }}>No {direction} calls</div> : render(node.children, depth + 1)) : null}
    </div>
  ));

  return (
    <div className="zorai-call-hierarchy" role="dialog" aria-label="Call hierarchy" onKeyDown={(event) => { if (event.key === "Escape") onClose(); }}>
      <div className="zorai-call-hierarchy__header">
        <span>Call Hierarchy</span>
        <button type="button" className={direction === "incoming" ? "is-on" : undefined} onClick={() => setDirection("incoming")}>Callers</button>
        <button type="button" className={direction === "outgoing" ? "is-on" : undefined} onClick={() => setDirection("outgoing")}>Calls</button>
        <button type="button" aria-label="Close call hierarchy" onClick={onClose}>×</button>
      </div>
      <div className="zorai-call-hierarchy__body">
        {error ? <p className="zorai-call-hierarchy__none">{error}</p> : tree === null ? <p className="zorai-call-hierarchy__none">Loading…</p> : render(tree, 0)}
      </div>
    </div>
  );
}
