import { useEffect, useRef, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { BYTES_PER_ROW, hexRow } from "./codeHexModel";

const CHUNK_BYTES = 64 * 1024;
const ROW_HEIGHT = 20;

function decode(base64: string): Uint8Array {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  return bytes;
}

export function CodeHexViewer({ root, path, onClose }: { root: string; path: string; onClose: () => void }) {
  const [total, setTotal] = useState<number | null>(null);
  const [chunks, setChunks] = useState<Record<number, Uint8Array>>({});
  const [scrollTop, setScrollTop] = useState(0);
  const [height, setHeight] = useState(400);
  const [error, setError] = useState<string | null>(null);
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const loading = useRef(new Set<number>());

  const load = (chunk: number) => {
    if (chunks[chunk] || loading.current.has(chunk)) return;
    loading.current.add(chunk);
    void getBridge()?.workspaceReadBytes?.(root, path, chunk * CHUNK_BYTES, CHUNK_BYTES).then((result) => {
      setTotal(result.totalBytes);
      setChunks((current) => ({ ...current, [chunk]: decode(result.base64) }));
    }).catch((reason) => setError(reason instanceof Error ? reason.message : String(reason))).finally(() => loading.current.delete(chunk));
  };

  useEffect(() => {
    setChunks({});
    setTotal(null);
    setError(null);
    loading.current.clear();
  }, [root, path]);

  useEffect(() => {
    const body = bodyRef.current;
    if (!body) return;
    const observer = new ResizeObserver(() => setHeight(body.clientHeight));
    observer.observe(body);
    setHeight(body.clientHeight);
    return () => observer.disconnect();
  }, []);

  const rows = total === null ? 0 : Math.ceil(total / BYTES_PER_ROW);
  const firstRow = Math.floor(scrollTop / ROW_HEIGHT);
  const visibleRows = Math.ceil(height / ROW_HEIGHT) + 2;
  const lastRow = Math.min(rows, firstRow + visibleRows);
  useEffect(() => {
    if (total === null) { load(0); return; }
    for (let chunk = Math.floor((firstRow * BYTES_PER_ROW) / CHUNK_BYTES); chunk <= Math.floor((lastRow * BYTES_PER_ROW) / CHUNK_BYTES); chunk += 1) load(chunk);
  });

  const rowAt = (row: number) => {
    const offset = row * BYTES_PER_ROW;
    const chunk = chunks[Math.floor(offset / CHUNK_BYTES)];
    if (!chunk) return null;
    const start = offset % CHUNK_BYTES;
    return hexRow(chunk.subarray(start, start + BYTES_PER_ROW), offset);
  };

  return (
    <div className="zorai-hex-viewer">
      <div className="zorai-hex-viewer__bar">
        <strong>{path.split("/").pop()}</strong>
        <span>{total === null ? "Loading…" : `${total.toLocaleString()} bytes · read-only hex view`}</span>
        <button type="button" onClick={onClose}>Close hex view</button>
      </div>
      {error ? <p className="zorai-hex-viewer__error">{error}</p> : null}
      <div ref={bodyRef} className="zorai-hex-viewer__body" onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}>
        <div style={{ height: rows * ROW_HEIGHT, position: "relative" }}>
          {Array.from({ length: Math.max(0, lastRow - firstRow) }, (_, index) => {
            const row = firstRow + index;
            const data = rowAt(row);
            return (
              <div key={row} className="zorai-hex-viewer__row" style={{ top: row * ROW_HEIGHT }}>
                <span className="zorai-hex-viewer__address">{data?.address ?? (row * BYTES_PER_ROW).toString(16).padStart(8, "0")}</span>
                <span className="zorai-hex-viewer__hex">{data?.hex ?? ""}</span>
                <span className="zorai-hex-viewer__ascii">{data?.ascii ?? ""}</span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
