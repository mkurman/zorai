import { useEffect, useState, type KeyboardEvent } from "react";
import { getBridge } from "@/lib/bridge";
import { pushToast } from "@/lib/toastStore";
import { useOperatorQuestionStore } from "@/lib/operatorQuestionStore";

/**
 * Non-blocking question dock. Renders as a bottom-anchored layer above the
 * input so the operator can read the transcript behind it while answering.
 * There is intentionally no backdrop and no focus trap.
 */
export function OperatorQuestionDock({
  inline = false,
  threadId = null,
}: {
  inline?: boolean;
  threadId?: string | null;
}) {
  const question = useOperatorQuestionStore((state) => state.question);
  const inlineHosts = useOperatorQuestionStore((state) => state.inlineHosts);
  const registerInlineHost = useOperatorQuestionStore((state) => state.registerInlineHost);
  const [busyAnswer, setBusyAnswer] = useState<string | null>(null);
  const [otherOpen, setOtherOpen] = useState(false);
  const [otherText, setOtherText] = useState("");

  useEffect(() => (inline ? registerInlineHost() : undefined), [inline, registerInlineHost]);

  const questionId = question?.question_id ?? null;
  useEffect(() => {
    setOtherOpen(false);
    setOtherText("");
  }, [questionId]);

  if (
    !question
    || (!inline && inlineHosts > 0)
    || (inline && question.threadId !== null && question.threadId !== threadId)
  ) {
    return null;
  }

  const submit = async (answer: string) => {
    const trimmed = answer.trim();
    if (busyAnswer || !trimmed) return;
    setBusyAnswer(trimmed);
    const bridge = getBridge();
    try {
      const result = await bridge?.agentAnswerQuestion?.(question.question_id, trimmed);
      if (result && result.ok === false) {
        pushToast(result.error || "Failed to send the answer.");
      }
    } finally {
      setBusyAnswer(null);
    }
  };

  const onOtherKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void submit(otherText);
    } else if (event.key === "Escape") {
      event.preventDefault();
      setOtherOpen(false);
    }
  };

  return (
    <div
      className={"zorai-question-dock" + (inline ? " zorai-question-dock--inline" : "")}
      role="dialog"
      aria-label="Operator question"
    >
      <div className="zorai-question-dock__eyebrow">Question · answer to continue</div>
      <div className="zorai-question-dock__content">{question.content}</div>
      <div className="zorai-question-dock__options">
        {question.options.map((option) => {
          const active = busyAnswer === option;
          return (
            <button
              key={option}
              type="button"
              onClick={() => void submit(option)}
              disabled={Boolean(busyAnswer)}
              className={
                "zorai-question-dock__option"
                + (active ? " zorai-question-dock__option--active" : "")
              }
            >
              {option}
            </button>
          );
        })}
        <button
          type="button"
          onClick={() => setOtherOpen((open) => !open)}
          disabled={Boolean(busyAnswer)}
          aria-expanded={otherOpen}
          className={
            "zorai-question-dock__option"
            + (otherOpen ? " zorai-question-dock__option--active" : "")
          }
        >
          Other…
        </button>
      </div>
      {otherOpen ? (
        <div className="zorai-question-dock__other">
          <textarea
            autoFocus
            rows={2}
            value={otherText}
            onChange={(event) => setOtherText(event.target.value)}
            onKeyDown={onOtherKeyDown}
            placeholder="Type your own answer… (Enter sends, Esc cancels)"
            aria-label="Your own answer"
            disabled={Boolean(busyAnswer)}
          />
          <button
            type="button"
            className="zorai-question-dock__option zorai-question-dock__option--active"
            onClick={() => void submit(otherText)}
            disabled={Boolean(busyAnswer) || !otherText.trim()}
          >
            Send
          </button>
        </div>
      ) : null}
    </div>
  );
}
