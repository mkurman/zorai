import { useEffect, useState } from "react";
import { formatKeyboardEvent, useKeybindStore } from "../../lib/keybindStore";
import { Section, addBtnStyle, inputStyle, kbdStyle, rebindBtnStyle } from "./shared";

export function KeyboardTab() {
    const bindings = useKeybindStore((s) => s.bindings);
    const setBinding = useKeybindStore((s) => s.setBinding);
    const resetBindings = useKeybindStore((s) => s.resetBindings);
    const [query, setQuery] = useState("");
    const [recordingAction, setRecordingAction] = useState<string | null>(null);
    const [warning, setWarning] = useState<string | null>(null);

    useEffect(() => {
        if (!recordingAction) return;

        const onKeyDown = (event: KeyboardEvent) => {
            event.preventDefault();
            event.stopPropagation();

            if (event.key === "Escape") {
                setRecordingAction(null);
                return;
            }

            const combo = formatKeyboardEvent(event);
            if (!combo || combo === "Ctrl" || combo === "Alt" || combo === "Shift") {
                return;
            }

            const conflicting = bindings.find((binding) => binding.combo === combo && binding.action !== recordingAction);
            if (conflicting) {
                setWarning(`${combo} is already assigned to ${conflicting.description}`);
                return;
            }

            setBinding(recordingAction as never, combo);
            setWarning(null);
            setRecordingAction(null);
        };

        window.addEventListener("keydown", onKeyDown, true);
        return () => window.removeEventListener("keydown", onKeyDown, true);
    }, [bindings, recordingAction, setBinding]);

    const filteredShortcuts = query.trim()
        ? bindings.filter((binding) => `${binding.combo} ${binding.description}`.toLowerCase().includes(query.toLowerCase()))
        : bindings;

    return (
        <Section title="Keyboard Shortcuts">
            <div style={{ marginBottom: 10 }}>
                <input
                    type="text"
                    value={query}
                    onChange={(event) => setQuery(event.target.value)}
                    placeholder="Search shortcuts..."
                    style={{ ...inputStyle, width: "100%" }}
                />
                <div style={{ marginTop: 8, display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                    <span style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
                        {recordingAction ? "Press a new shortcut or Esc to cancel" : "Click Rebind to capture a shortcut"}
                    </span>
                    <button onClick={resetBindings} style={addBtnStyle}>Reset bindings</button>
                </div>
                {warning ? (
                    <div style={{ marginTop: 8, fontSize: "var(--text-sm)", color: "var(--warning)" }}>
                        {warning}
                    </div>
                ) : null}
            </div>
            {filteredShortcuts.map((binding) => (
                <div
                    key={binding.action}
                    style={{
                        display: "flex", justifyContent: "space-between", alignItems: "center", padding: "5px 0",
                        fontSize: "var(--text-sm)", borderBottom: "1px solid color-mix(in srgb, var(--text-primary) 3%, transparent)",
                    }}
                >
                    <span style={{ color: "var(--text-secondary)" }}>{binding.description}</span>
                    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                        <kbd style={kbdStyle}>{binding.combo}</kbd>
                        <button
                            type="button"
                            onClick={() => setRecordingAction(binding.action)}
                            style={rebindBtnStyle}
                        >
                            {recordingAction === binding.action ? "Recording..." : "Rebind"}
                        </button>
                        <button
                            type="button"
                            onClick={() => setBinding(binding.action, "")}
                            style={rebindBtnStyle}
                        >
                            Clear
                        </button>
                    </div>
                </div>
            ))}
        </Section>
    );
}