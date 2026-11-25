import { useRef, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { RarogFixPrompt } from "@/zorai/features/code/RarogFixPrompt";
import { cleanGeneratedCommand, terminalCommandContext } from "./terminalInlineCommand";

export function TerminalInlineCommandPrompt({ cwd, shell, getRecentOutput, onInsert, onClose }: {
  cwd?: string | null;
  shell?: string | null;
  getRecentOutput: () => string;
  onInsert: (command: string, run: boolean) => void;
  onClose: () => void;
}) {
  const [command, setCommand] = useState("");
  const [busy, setBusy] = useState(false);
  const [thinking, setThinking] = useState("");
  const [error, setError] = useState<string | null>(null);
  const requestRef = useRef(0);
  const shellName = shell || "shell";

  const generate = async (instruction: string, reasoning: boolean) => {
    const bridge = getBridge();
    if (!instruction || busy) return;
    if (!bridge?.agentGenerateCodeFix) {
      setError("Terminal commands need the Zorai desktop app with an updated daemon.");
      return;
    }
    const request = ++requestRef.current;
    const requestId = `terminal-${Date.now()}-${request}`;
    let thought = "";
    const unsubscribe = bridge.onAgentEvent?.((event: { type?: string; data?: { request_id?: string; text?: string; reasoning?: boolean } }) => {
      if (event?.type !== "code-fix-delta" || event.data?.request_id !== requestId || !event.data?.reasoning) return;
      thought += event.data?.text ?? "";
      if (request === requestRef.current) setThinking(thought);
    });
    setBusy(true);
    setError(null);
    setThinking("");
    try {
      const result = await bridge.agentGenerateCodeFix({
        requestId,
        path: "terminal",
        language: shellName,
        before: terminalCommandContext({ cwd, shell, platform: navigator.platform, recentOutput: getRecentOutput() }),
        target: command,
        after: "",
        diagnostics: [],
        instruction: command
          ? `Revise this ${shellName} command: ${instruction}. Reply with only the command, no explanation.`
          : `Write a single ${shellName} command (one line, or a short pipeline) for: ${instruction}. Reply with only the command, no explanation.`,
        reasoning,
      });
      if (request !== requestRef.current) return;
      const next = cleanGeneratedCommand(result.replacement ?? "");
      if (result.error || !next) setError(`Leszy could not write a command: ${result.error || "empty reply"}`);
      else setCommand(next);
    } catch (reason) {
      if (request === requestRef.current) setError(`Leszy could not write a command: ${reason instanceof Error ? reason.message : String(reason)}`);
    } finally {
      unsubscribe?.();
      if (request === requestRef.current) {
        setBusy(false);
        setThinking("");
      }
    }
  };

  return (
    <div
      className="zorai-terminal-command"
      onMouseDown={(event) => event.stopPropagation()}
      onClick={(event) => event.stopPropagation()}
      onKeyDown={(event) => event.stopPropagation()}
      onKeyUp={(event) => event.stopPropagation()}
    >
      <RarogFixPrompt
        startLine={0}
        endLine={0}
        diagnostics={0}
        scope={shellName}
        title="Terminal command with Leszy"
        placeholder="Describe the command you need…"
        busy={busy}
        error={error}
        thinking={thinking}
        reviewing={Boolean(command)}
        acceptLabel="Insert"
        preview={command ? <code>{command}</code> : null}
        onSubmit={(instruction, reasoning) => void generate(instruction, reasoning)}
        onAccept={() => onInsert(command, false)}
        onRun={() => onInsert(command, true)}
        onClose={onClose}
      />
    </div>
  );
}
