import { useEffect, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { withRoot } from "./CodeVerifyToggle";

type Status = { enabled: boolean; embeddings_configured: boolean; embedding_model: string; estimate: { files: number; chunks: number; characters: number; approx_tokens: number } };
type Hit = { path: string; line: number; title: string; score: number; snippet: string };

async function setCodeIndexEnabled(root: string, enabled: boolean): Promise<void> {
  const bridge = getBridge();
  const config = (await bridge?.agentGetConfig?.()) as { code_index?: { roots?: string[] } } | null;
  await bridge?.agentSetConfigItem?.("/code_index/roots", withRoot(config?.code_index?.roots ?? [], root, enabled));
}

export function CodeSemanticSearch({ root, query, onOpen }: { root: string; query: string; onOpen: (path: string, line: number) => void }) {
  const [status, setStatus] = useState<Status | null>(null);
  const [hits, setHits] = useState<Hit[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refreshStatus = () => {
    void getBridge()?.agentCodeIndex?.("status", root).then((response) => {
      if (response.error) setError(response.error);
      else setStatus(response.result as Status);
    });
  };
  useEffect(refreshStatus, [root]);

  useEffect(() => {
    if (!status?.enabled || !query.trim()) {
      setHits([]);
      return;
    }
    let cancelled = false;
    const timer = window.setTimeout(() => {
      setBusy("Searching…");
      void getBridge()?.agentCodeIndex?.("search", root, query, 30).then((response) => {
        if (cancelled) return;
        setError(response.error);
        setHits(Array.isArray(response.result) ? response.result as Hit[] : []);
      }).finally(() => { if (!cancelled) setBusy(null); });
    }, 400);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [query, root, status?.enabled]);

  const toggle = async (enabled: boolean) => {
    setBusy(enabled ? "Indexing… the first pass embeds every chunk once" : "Turning off…");
    setError(null);
    try {
      await setCodeIndexEnabled(root, enabled);
      if (enabled) {
        const response = await getBridge()?.agentCodeIndex?.("sync", root);
        if (response?.error) setError(response.error);
      }
      refreshStatus();
    } finally {
      setBusy(null);
    }
  };

  if (!status) return <p className="zorai-code-search__summary">{error ?? "Checking the code index…"}</p>;
  if (!status.embeddings_configured) {
    return <p className="zorai-code-search__summary">Semantic search needs an embedding model. Turn on embeddings in Settings → Memory → Semantic embeddings first.</p>;
  }
  if (!status.enabled) {
    return (
      <div className="zorai-semantic-optin">
        <p>Semantic search finds code by meaning (e.g. "where do we retry failed uploads"). It embeds this workspace with <strong>{status.embedding_model}</strong>, which your provider bills per token.</p>
        <p>This workspace: {status.estimate.files} files · {status.estimate.chunks} chunks · about {status.estimate.approx_tokens.toLocaleString()} tokens for the first pass. After that only changed functions are re-embedded.</p>
        <button type="button" disabled={Boolean(busy)} onClick={() => void toggle(true)}>{busy ?? "Enable for this workspace"}</button>
        {error ? <p className="is-error">{error}</p> : null}
      </div>
    );
  }
  return (
    <div className="zorai-semantic-results">
      <div className="zorai-code-search__summary">
        {busy ?? (query.trim() ? `${hits.length} semantic match${hits.length === 1 ? "" : "es"}` : "Describe what the code does.")}
        <button type="button" className="zorai-semantic-off" onClick={() => void toggle(false)}>Turn off</button>
      </div>
      {error ? <p className="zorai-code-search__summary is-error">{error}</p> : null}
      {hits.map((hit) => (
        <button key={`${hit.path}:${hit.line}:${hit.title}`} type="button" className="zorai-semantic-hit" title={hit.snippet} onClick={() => onOpen(hit.path, hit.line)}>
          <span className="zorai-semantic-hit__title">{hit.title}</span>
          <span className="zorai-semantic-hit__score">{Math.round(hit.score * 100)}%</span>
          <code>{hit.snippet.split("\n").slice(1, 4).join("\n")}</code>
        </button>
      ))}
    </div>
  );
}
