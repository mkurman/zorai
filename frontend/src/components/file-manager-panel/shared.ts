import type { CSSProperties } from "react";

export type PaneKey = "left" | "right";

export type FsEntry = {
    name: string;
    path: string;
    isDirectory: boolean;
    sizeBytes: number | null;
    modifiedAt: number | null;
};

export type PaneState = {
    path: string;
    entries: FsEntry[];
    selectedPath: string | null;
    loading: boolean;
    error: string | null;
};

export type PaneRow = {
    path: string;
    type: "parent" | "entry";
    entry?: FsEntry;
};

export type FsBridge = {
    listFsDir?: (targetDir: string) => Promise<FsEntry[]>;
    copyFsPath?: (sourcePath: string, destinationPath: string) => Promise<boolean>;
    moveFsPath?: (sourcePath: string, destinationPath: string) => Promise<boolean>;
    deleteFsPath?: (targetPath: string) => Promise<boolean>;
    createFsDirectory?: (targetDirPath: string) => Promise<boolean>;
    openFsPath?: (targetPath: string) => Promise<string>;
    revealFsPath?: (targetPath: string) => Promise<boolean>;
    sendTerminalInput?: (paneId: string, dataB64: string) => Promise<boolean>;
};

export const DEFAULT_LEFT_PATH = "~/";
export const DEFAULT_RIGHT_PATH = ".";

export function getBridge(): FsBridge | null {
    if (typeof window === "undefined") return null;
    return ((window as unknown as { zorai?: FsBridge }).zorai ?? null);
}

export function encodeToBase64(text: string): string {
    const bytes = new TextEncoder().encode(text);
    let binary = "";
    for (const byte of bytes) {
        binary += String.fromCharCode(byte);
    }
    return btoa(binary);
}

export function getPathSeparator(targetPath: string): string {
    return targetPath.includes("\\") ? "\\" : "/";
}

export function joinPath(parent: string, name: string): string {
    const sep = getPathSeparator(parent);
    if (parent.endsWith(sep)) {
        return `${parent}${name}`;
    }
    return `${parent}${sep}${name}`;
}

export function getParentPath(targetPath: string): string | null {
    if (!targetPath) return null;

    if (/^[A-Za-z]:\\?$/.test(targetPath)) {
        return null;
    }

    if (targetPath === "/") {
        return null;
    }

    const sep = getPathSeparator(targetPath);
    const normalized = targetPath.endsWith(sep) && targetPath.length > 1
        ? targetPath.slice(0, -1)
        : targetPath;

    const index = normalized.lastIndexOf(sep);
    if (index < 0) return null;
    if (index === 0 && sep === "/") return "/";

    return normalized.slice(0, index);
}

export function formatBytes(value: number | null): string {
    if (value === null || value < 0) return "-";
    if (value < 1024) return `${value} B`;
    if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
    if (value < 1024 * 1024 * 1024) return `${(value / (1024 * 1024)).toFixed(1)} MB`;
    return `${(value / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

export function buildPaneRows(pane: PaneState): PaneRow[] {
    const rows: PaneRow[] = [];
    const parentPath = getParentPath(pane.path);
    if (parentPath) {
        rows.push({ path: parentPath, type: "parent" });
    }
    for (const entry of pane.entries) {
        rows.push({ path: entry.path, type: "entry", entry });
    }
    return rows;
}

export const inputStyle: CSSProperties = {
    height: 28,
    boxSizing: "border-box",
    background: "transparent",
    border: "1px solid var(--zorai-border)",
    color: "var(--zorai-text)",
    borderRadius: 6,
    padding: "4px 8px",
    fontFamily: "var(--font-ui)",
    fontSize: "var(--text-sm)",
    outline: "none",
    width: "100%",
};