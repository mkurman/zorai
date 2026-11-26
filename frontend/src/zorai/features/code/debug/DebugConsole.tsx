import { useEffect, useRef, useState } from "react";
import { useDebugStore } from "./debugStore";

export function DebugConsole() {
  const lines = useDebugStore((state) => state.console);
  const status = useDebugStore((state) => state.status);
  const evaluate = useDebugStore((state) => state.evaluate);
  const [input, setInput] = useState("");
  const [history, setHistory] = useState<string[]>([]);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const outputRef = useRef<HTMLPreElement | null>(null);

  useEffect(() => {
    const output = outputRef.current;
    if (output) output.scrollTop = output.scrollHeight;
  }, [lines]);

  return (
    <div className="zorai-debug-console">
      <pre ref={outputRef} className="zorai-debug-console__output" aria-live="polite">
        {lines.length === 0 ? <span className="zorai-debug-console__line is-console">Start a debug session to see program output here.</span> : lines.map((line, index) => (
          <span key={index} className={`zorai-debug-console__line is-${line.category}`}>{line.category === "input" ? `› ${line.text}` : line.text}</span>
        ))}
      </pre>
      <input
        className="zorai-debug-console__input"
        value={input}
        disabled={status !== "stopped" && status !== "running"}
        placeholder={status === "stopped" ? "Evaluate in the selected frame" : "Pause the program to evaluate expressions"}
        aria-label="Debug console input"
        onChange={(event) => setInput(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && input.trim()) {
            void evaluate(input.trim(), "repl");
            setHistory((current) => [input.trim(), ...current.filter((entry) => entry !== input.trim())].slice(0, 50));
            setHistoryIndex(-1);
            setInput("");
          } else if (event.key === "ArrowUp" && history.length > 0) {
            const next = Math.min(history.length - 1, historyIndex + 1);
            setHistoryIndex(next);
            setInput(history[next]);
            event.preventDefault();
          } else if (event.key === "ArrowDown" && historyIndex >= 0) {
            const next = historyIndex - 1;
            setHistoryIndex(next);
            setInput(next >= 0 ? history[next] : "");
            event.preventDefault();
          }
        }}
      />
    </div>
  );
}
