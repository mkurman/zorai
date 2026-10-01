import { getBridge } from "@/lib/bridge";
import { useThreadFilePreview } from "@/zorai/features/threads/ThreadFilePreviewContext";
import type { ToolArtifactReference } from "./toolArtifacts";
import { toolArtifactPreviewEntry } from "./toolArtifactPresentation";
import { useMemo } from "react";

export function ToolArtifactChips({
  artifacts,
  createdAt,
  compact = false,
}: {
  artifacts: ToolArtifactReference[];
  createdAt: number;
  compact?: boolean;
}) {
  const { openThreadFilePreview } = useThreadFilePreview();
  const visible = compact ? artifacts.slice(0, 2) : artifacts;
  const overflow = compact ? Math.max(0, artifacts.length - visible.length) : 0;
  const bridge = getBridge();
  const paths = useMemo(() => artifacts.map((artifact) => artifact.path.split("/").pop() ?? artifact.path), [artifacts]);

  if (artifacts.length === 0) return null;

  return (
    <div className={compact ? "zorai-tool-artifacts zorai-tool-artifacts--compact" : "zorai-tool-artifacts"}>
      {visible.map((artifact, index) => (
        <div key={`${artifact.provenance}:${artifact.path}`} className="zorai-tool-artifact">
          <button
            type="button"
            className="zorai-tool-artifact__path"
            title={`Preview ${artifact.path}`}
            onClick={(event) => {
              event.stopPropagation();
              openThreadFilePreview(toolArtifactPreviewEntry(artifact, createdAt));
            }}
          >
            {paths[index] ?? artifact.path}
          </button>
          {!compact ? (
            <>
              <span className="zorai-status-pill">{artifact.provenance}</span>
              <button
                type="button"
                className="zorai-ghost-button"
                onClick={(event) => {
                  event.stopPropagation();
                  openThreadFilePreview(toolArtifactPreviewEntry(artifact, createdAt));
                }}
              >
                Open
              </button>
              {bridge?.revealFsPath ? (
                <button
                  type="button"
                  className="zorai-ghost-button"
                  onClick={(event) => {
                    event.stopPropagation();
                    void bridge.revealFsPath?.(artifact.path);
                  }}
                >
                  Reveal
                </button>
              ) : null}
            </>
          ) : null}
        </div>
      ))}
      {overflow > 0 ? <span className="zorai-status-pill">+{overflow}</span> : null}
    </div>
  );
}
