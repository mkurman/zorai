import { CodeContextMenu } from "@/zorai/features/code/CodeToolbarMenu";
import { AppConfirmDialog } from "../AppConfirmDialog";
import { canvasPanelMenuItems } from "./canvasPanelMenu";
import type { CanvasContextMenuState, CanvasPanelRecord } from "./types";

type CanvasMenusProps = {
  contextMenu: CanvasContextMenuState | null;
  contextPaneIds: string[];
  confirmClosePaneIds: string[];
  panelByPane: Map<string, CanvasPanelRecord>;
  paneNames: Record<string, string>;
  workspaceMoveTargets: Array<{ id: string; name: string }>;
  onZoomIn: () => void;
  onRenamePanel: () => void;
  onDuplicatePanels: () => void;
  onConvertToBsp: () => void;
  onRunSnippet: () => void;
  onSetIcon: (iconId: string) => void;
  onMoveToWorkspace: (workspaceId: string) => void;
  onRequestClosePanels: () => void;
  onCloseContextMenu: () => void;
  onConfirmClosePanels: () => void;
  onCancelClosePanels: () => void;
};

export function CanvasMenus({
  contextMenu,
  contextPaneIds,
  confirmClosePaneIds,
  panelByPane,
  paneNames,
  workspaceMoveTargets,
  onZoomIn,
  onRenamePanel,
  onDuplicatePanels,
  onConvertToBsp,
  onRunSnippet,
  onSetIcon,
  onMoveToWorkspace,
  onRequestClosePanels,
  onCloseContextMenu,
  onConfirmClosePanels,
  onCancelClosePanels,
}: CanvasMenusProps) {
  return (
    <>
      {contextMenu ? (
        <CodeContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          label="Panel actions"
          onClose={onCloseContextMenu}
          items={canvasPanelMenuItems({
            count: contextPaneIds.length,
            hasNonBrowser: contextPaneIds.some((id) => panelByPane.get(id)?.panelType !== "browser"),
            workspaceTargets: workspaceMoveTargets,
            onZoomIn,
            onRename: onRenamePanel,
            onDuplicate: onDuplicatePanels,
            onConvertToBsp,
            onRunSnippet,
            onSetIcon,
            onMoveToWorkspace,
            onClose: onRequestClosePanels,
          })}
        />
      ) : null}

      <AppConfirmDialog
        open={confirmClosePaneIds.length > 0}
        title={confirmClosePaneIds.length > 1
          ? `Close ${confirmClosePaneIds.length} panels?`
          : confirmClosePaneIds.length === 1
            ? `Close '${paneNames[confirmClosePaneIds[0]] ?? "panel"}'?`
            : ""}
        message={confirmClosePaneIds.length > 1
          ? "All selected terminal panels will be closed."
          : "This terminal panel will be closed."}
        confirmLabel={confirmClosePaneIds.length > 1 ? `Close ${confirmClosePaneIds.length} Panels` : "Close Panel"}
        tone="danger"
        onCancel={onCancelClosePanels}
        onConfirm={onConfirmClosePanels}
      />
    </>
  );
}
