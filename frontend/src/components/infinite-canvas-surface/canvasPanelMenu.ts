import type { CodeToolbarMenuItem } from "@/zorai/features/code/CodeToolbarMenu";
import { iconChoices, PANE_ICON_IDS } from "../../lib/iconRegistry";

export function canvasPanelMenuItems({ count, hasNonBrowser, workspaceTargets, onZoomIn, onRename, onDuplicate, onConvertToBsp, onRunSnippet, onSetIcon, onMoveToWorkspace, onClose }: {
  count: number;
  hasNonBrowser: boolean;
  workspaceTargets: Array<{ id: string; name: string }>;
  onZoomIn: () => void;
  onRename: () => void;
  onDuplicate: () => void;
  onConvertToBsp: () => void;
  onRunSnippet: () => void;
  onSetIcon: (iconId: string) => void;
  onMoveToWorkspace: (workspaceId: string) => void;
  onClose: () => void;
}): CodeToolbarMenuItem[] {
  const many = count > 1;
  const items: CodeToolbarMenuItem[] = [
    { key: "zoom", label: "Zoom In", onSelect: onZoomIn },
    { key: "rename", label: "Rename Panel", disabled: many, onSelect: onRename },
    { key: "duplicate", label: many ? `Duplicate ${count} Panels` : "Duplicate Panel", onSelect: onDuplicate },
  ];
  if (hasNonBrowser) {
    items.push(
      { key: "bsp", label: many ? `Convert ${count} to BSP` : "Convert to BSP", onSelect: onConvertToBsp },
      { key: "snippet", label: "Run Snippet", onSelect: onRunSnippet },
    );
  }
  items.push({
    key: "icon",
    label: many ? `Change Icon (${count} Panels)` : "Change Icon",
    separatorBefore: true,
    children: iconChoices(PANE_ICON_IDS).map((icon) => ({ key: `icon-${icon.id}`, label: `${icon.glyph}  ${icon.label}`, onSelect: () => onSetIcon(icon.id) })),
  });
  if (workspaceTargets.length > 0) {
    items.push({
      key: "move",
      label: "Move to Workspace",
      children: workspaceTargets.map((workspace) => ({ key: `move-${workspace.id}`, label: workspace.name, onSelect: () => onMoveToWorkspace(workspace.id) })),
    });
  }
  items.push({ key: "close", label: many ? `Close ${count} Panels` : "Close Panel", danger: true, separatorBefore: true, onSelect: onClose });
  return items;
}
