import { useEffect, useMemo, useState } from "react";
import { RefreshButton } from "@/zorai/shell/RefreshButton";
import { useAgentStore } from "../lib/agentStore";
import { normalizeOperatorProfileInputKind } from "../lib/agentStore/operatorProfile";

const SELECT_OPTIONS_BY_FIELD: Record<string, string[]> = {
  notification_preference: ["minimal", "balanced", "proactive"],
};

function getQuestionSelectOptions(fieldKey: string): string[] {
  return SELECT_OPTIONS_BY_FIELD[fieldKey] ?? [];
}

export function OperatorProfileOnboardingPanel() {
  const operatorProfile = useAgentStore((s) => s.operatorProfile);
  const fetchNextQuestion = useAgentStore((s) => s.fetchNextOperatorProfileQuestion);
  const submitAnswer = useAgentStore((s) => s.submitOperatorProfileAnswer);
  const skipQuestion = useAgentStore((s) => s.skipOperatorProfileQuestion);
  const deferQuestion = useAgentStore((s) => s.deferOperatorProfileQuestion);
  const setPanelOpen = useAgentStore((s) => s.setOperatorProfilePanelOpen);

  const question = operatorProfile.question;
  const inputKind = normalizeOperatorProfileInputKind(question?.input_kind);
  const selectOptions = useMemo(
    () => getQuestionSelectOptions(question?.field_key ?? ""),
    [question?.field_key],
  );

  const [textValue, setTextValue] = useState("");
  const [boolValue, setBoolValue] = useState<boolean | null>(null);
  const [selectValue, setSelectValue] = useState("");

  useEffect(() => {
    setTextValue("");
    setBoolValue(null);
    setSelectValue(selectOptions[0] ?? "");
  }, [question?.question_id, selectOptions]);

  if (!operatorProfile.panelOpen) {
    return null;
  }

  const answered = operatorProfile.progress?.answered ?? 0;
  const remaining = operatorProfile.progress?.remaining ?? (question ? 1 : 0);
  const total = Math.max(1, answered + remaining);
  const completionRatio = operatorProfile.progress?.completion_ratio ?? answered / total;
  const completionPct = Math.max(0, Math.min(100, Math.round(completionRatio * 100)));

  const canSubmit = Boolean(question) && (
    (inputKind === "text" && textValue.trim().length > 0)
    || (inputKind === "bool" && typeof boolValue === "boolean")
    || (inputKind === "select" && selectValue.trim().length > 0)
    || (inputKind !== "text" && inputKind !== "bool" && inputKind !== "select")
  );

  const submitValue = async () => {
    if (!question || !canSubmit) {
      return;
    }
    if (inputKind === "bool") {
      await submitAnswer(boolValue);
      return;
    }
    if (inputKind === "select") {
      await submitAnswer(selectValue);
      return;
    }
    await submitAnswer(textValue.trim());
  };

  return (
    <div className="zorai-onboarding-backdrop zorai-onboarding-backdrop--profile">
      <div className="zorai-onboarding-dialog zorai-onboarding-dialog--profile">
        <div className="zorai-onboarding-header-row">
          <div className="zorai-onboarding-stack">
            <span className="zorai-onboarding-kicker">About You</span>
            <h2>Operator Profile Onboarding</h2>
            <span className="zorai-onboarding-copy--sm zorai-onboarding-copy">
              {operatorProfile.sessionKind ?? "first_run_onboarding"}
            </span>
          </div>
          <button
            type="button"
            className="zorai-onboarding-button"
            onClick={() => setPanelOpen(false)}
          >
            Hide
          </button>
        </div>

        <div className="zorai-onboarding-progress">
          <div className="zorai-onboarding-progress__meta">
            <span>Progress</span>
            <span>{answered} answered • {remaining} remaining</span>
          </div>
          <div className="zorai-onboarding-progress__track">
            <div
              className="zorai-onboarding-progress__bar"
              style={{ width: `${completionPct}%` }}
            />
          </div>
        </div>

        {!question ? (
          <div className="zorai-onboarding-copy">
            {operatorProfile.loading ? "Loading your next question..." : "No pending profile question."}
          </div>
        ) : (
          <div className="zorai-onboarding-question">
            <div className="zorai-onboarding-question__prompt">{question.prompt}</div>
            <div className="zorai-onboarding-copy--sm zorai-onboarding-copy">
              Field: <code>{question.field_key}</code> • {question.optional ? "optional" : "recommended"}
            </div>
            {operatorProfile.loading ? (
              <div className="zorai-onboarding-copy--sm zorai-onboarding-copy">
                Saving answer...
              </div>
            ) : null}

            {inputKind === "bool" ? (
              <div className="zorai-onboarding-bool-row">
                <button
                  type="button"
                  onClick={() => setBoolValue(true)}
                  className={["zorai-onboarding-button", boolValue === true ? "zorai-onboarding-button--selected" : ""].filter(Boolean).join(" ")}
                >
                  Yes
                </button>
                <button
                  type="button"
                  onClick={() => setBoolValue(false)}
                  className={["zorai-onboarding-button", boolValue === false ? "zorai-onboarding-button--selected" : ""].filter(Boolean).join(" ")}
                >
                  No
                </button>
              </div>
            ) : null}

            {inputKind === "select" ? (
              <select
                value={selectValue}
                onChange={(event) => setSelectValue(event.target.value)}
                className="zorai-onboarding-input"
              >
                {selectOptions.length > 0 ? (
                  selectOptions.map((option) => (
                    <option key={option} value={option}>{option}</option>
                  ))
                ) : (
                  <option value="">Select an option</option>
                )}
              </select>
            ) : null}

            {inputKind !== "bool" && inputKind !== "select" ? (
              <textarea
                value={textValue}
                onChange={(event) => setTextValue(event.target.value)}
                placeholder="Type your answer…"
                rows={4}
                className="zorai-onboarding-textarea"
              />
            ) : null}
          </div>
        )}

        {operatorProfile.error ? (
          <div className="zorai-onboarding-error">
            {operatorProfile.error}
          </div>
        ) : null}

        <div className="zorai-onboarding-actions zorai-onboarding-actions--split">
          <div className="zorai-onboarding-actions__group">
            <button
              type="button"
              onClick={() => void skipQuestion("skipped_from_onboarding_panel")}
              className="zorai-onboarding-button"
              disabled={!question || operatorProfile.loading}
            >
              Skip
            </button>
            <button
              type="button"
              onClick={() => void deferQuestion(Date.now() + 24 * 60 * 60 * 1000)}
              className="zorai-onboarding-button"
              disabled={!question || operatorProfile.loading}
            >
              Defer 24h
            </button>
          </div>

          <div className="zorai-onboarding-actions__group">
            <RefreshButton
              onClick={() => void fetchNextQuestion()}
              disabled={operatorProfile.loading || !operatorProfile.sessionId}
              busy={operatorProfile.loading}
            />
            <button
              type="button"
              onClick={() => void submitValue()}
              className="zorai-onboarding-button zorai-onboarding-button--primary"
              disabled={!canSubmit || operatorProfile.loading}
            >
              {operatorProfile.loading ? "Submitting" : "Submit"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
