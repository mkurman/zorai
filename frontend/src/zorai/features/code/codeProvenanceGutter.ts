import type { editor as MonacoEditorApi, IDisposable } from "monaco-editor";
import { computeLineHunks } from "./codeInlineEdit";

type MonacoApi = typeof import("monaco-editor");

export type ProvenanceOperation = { operationId: string; label: string; detail: string; at: number; prompt?: string; reasoning?: string; verification?: string };

type ContextMessage = { role: string; content: string; reasoning?: string; createdAt: number };
type ContextEvent = { command?: string | null; message?: string | null; timestamp: number };

function clip(text: string, max: number): string {
  const flat = text.replace(/\s+/g, " ").trim();
  return flat.length > max ? `${flat.slice(0, max - 1)}…` : flat;
}

export function provenanceContext(messages: ContextMessage[], events: ContextEvent[], at: number): Pick<ProvenanceOperation, "prompt" | "reasoning" | "verification"> {
  const prompt = [...messages].reverse().find((message) => message.role === "user" && message.createdAt <= at && message.content.trim());
  const reply = messages.find((message) => message.role === "assistant" && message.createdAt <= at + 120_000 && message.createdAt >= (prompt?.createdAt ?? 0) && (message.reasoning || message.content).trim());
  const verification = events.filter((event) => event.timestamp >= at && /^verify-(passed|failed)$/.test(event.command ?? "")).sort((left, right) => left.timestamp - right.timestamp)[0];
  return {
    prompt: prompt ? clip(prompt.content, 280) : undefined,
    reasoning: reply ? clip(reply.reasoning || reply.content, 360) : undefined,
    verification: verification?.message ?? undefined,
  };
}
export type ProvenanceOptions = {
  enabled: boolean;
  load: () => Promise<{ operations: ProvenanceOperation[]; befores: string[] } | null>;
  onRevert: (operationId: string) => void;
  onAskWhy: (operationId: string, startLine: number, endLine: number) => void;
};

const COLORS = 4;
const MAX_LINES = 20000;

export function attributeLines(versions: string[], owners: string[]): Array<string | null> {
  if (versions.length === 0) return [];
  let attribution: Array<string | null> = versions[0].split("\n").map(() => null);
  for (let index = 1; index < versions.length; index += 1) {
    const previous = attribution;
    const next: Array<string | null> = [];
    let cursor = 0;
    for (const hunk of computeLineHunks(versions[index - 1], versions[index])) {
      next.push(...previous.slice(cursor, hunk.baseStart));
      next.push(...hunk.added.map(() => owners[index - 1] || null));
      cursor = hunk.baseStart + hunk.removed.length;
    }
    next.push(...previous.slice(cursor));
    attribution = next;
  }
  return attribution;
}

export function provenanceBlocks(attribution: Array<string | null>): Array<{ owner: string; startLine: number; endLine: number }> {
  const blocks: Array<{ owner: string; startLine: number; endLine: number }> = [];
  attribution.forEach((owner, index) => {
    if (!owner) return;
    const last = blocks[blocks.length - 1];
    if (last && last.owner === owner && last.endLine === index) last.endLine = index + 1;
    else blocks.push({ owner, startLine: index + 1, endLine: index + 1 });
  });
  return blocks;
}

export function attachProvenanceGutter(editor: MonacoEditorApi.IStandaloneCodeEditor, monaco: MonacoApi, options: () => ProvenanceOptions | null): IDisposable & { refresh: () => void } {
  const decorations = editor.createDecorationsCollection([]);
  let data: { operations: ProvenanceOperation[]; befores: string[] } | null = null;
  let generation = 0;
  let timer = 0;
  const revertCommand = `zorai.provenance.revert.${editor.getId()}`;
  const askCommand = `zorai.provenance.ask.${editor.getId()}`;
  const commands = [
    monaco.editor.registerCommand(revertCommand, (_accessor, operationId: string) => options()?.onRevert(operationId)),
    monaco.editor.registerCommand(askCommand, (_accessor, operationId: string, startLine: number, endLine: number) => options()?.onAskWhy(operationId, startLine, endLine)),
  ];

  const render = () => {
    const model = editor.getModel();
    if (!model || !data || data.operations.length === 0 || !options()?.enabled || model.getLineCount() > MAX_LINES) {
      decorations.clear();
      return;
    }
    const versions = [...data.befores, model.getValue()];
    const owners = data.operations.map((operation) => operation.operationId);
    const byId = new Map(data.operations.map((operation, index) => [operation.operationId, { operation, index }]));
    decorations.set(provenanceBlocks(attributeLines(versions, owners)).map((block) => {
      const entry = byId.get(block.owner)!;
      const args = encodeURIComponent(JSON.stringify([block.owner, block.startLine, block.endLine]));
      return {
        range: new monaco.Range(block.startLine, 1, block.endLine, 1),
        options: {
          isWholeLine: true,
          linesDecorationsClassName: `zorai-provenance zorai-provenance--${entry.index % COLORS}`,
          hoverMessage: {
            isTrusted: true,
            value: [
              `**Agent change** · ${entry.operation.label}`,
              entry.operation.detail,
              entry.operation.prompt ? `**Asked:** ${entry.operation.prompt}` : "",
              entry.operation.reasoning ? `**Why:** ${entry.operation.reasoning}` : "",
              entry.operation.verification ? `**Verification:** ${entry.operation.verification}` : "",
              `${new Date(entry.operation.at).toLocaleString()} · lines ${block.startLine}–${block.endLine}`,
              `[Revert this change](command:${revertCommand}?${encodeURIComponent(JSON.stringify([block.owner]))}) · [Ask agent why](command:${askCommand}?${args})`,
            ].filter(Boolean).join("\n\n"),
          },
        },
      };
    }));
  };
  const refresh = () => {
    const current = options();
    const requested = ++generation;
    if (!current?.enabled) {
      data = null;
      render();
      return;
    }
    void current.load().catch(() => null).then((next) => {
      if (requested !== generation) return;
      data = next;
      render();
    });
  };
  const listeners = [
    editor.onDidChangeModel(refresh),
    editor.onDidChangeModelContent(() => {
      window.clearTimeout(timer);
      timer = window.setTimeout(render, 500);
    }),
  ];
  refresh();
  return {
    refresh,
    dispose() {
      generation += 1;
      window.clearTimeout(timer);
      listeners.forEach((listener) => listener.dispose());
      commands.forEach((command) => command.dispose());
      decorations.clear();
    },
  };
}
