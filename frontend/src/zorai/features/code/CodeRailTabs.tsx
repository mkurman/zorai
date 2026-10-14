import { useState } from "react";

export type CodeRailTab = "explorer" | "scm" | "testing" | "agent";

const STORAGE_KEY = "zorai.code.railTab";

const TABS: ReadonlyArray<{ id: CodeRailTab; label: string }> = [
  { id: "explorer", label: "Explorer" },
  { id: "scm", label: "Source Control" },
  { id: "testing", label: "Testing" },
  { id: "agent", label: "Agent" },
];

export function readStoredCodeRailTab(storage: Pick<Storage, "getItem"> | undefined): CodeRailTab {
  const stored = storage?.getItem(STORAGE_KEY);
  return TABS.some((tab) => tab.id === stored) ? stored as CodeRailTab : "explorer";
}

export function useCodeRailTab(): [CodeRailTab, (tab: CodeRailTab) => void] {
  const storage = typeof window === "undefined" ? undefined : window.localStorage;
  const [tab, setTab] = useState<CodeRailTab>(() => readStoredCodeRailTab(storage));
  return [tab, (next) => {
    setTab(next);
    storage?.setItem(STORAGE_KEY, next);
  }];
}

export function CodeRailTabs({ value, onChange, badges }: {
  value: CodeRailTab;
  onChange: (tab: CodeRailTab) => void;
  badges: Partial<Record<CodeRailTab, { count: number; tone?: "danger" }>>;
}) {
  return (
    <div className="zorai-code-rail-tabs" role="tablist" aria-label="Code sidebar">
      {TABS.map((tab) => {
        const badge = badges[tab.id];
        return (
          <button
            key={tab.id}
            type="button"
            role="tab"
            aria-selected={value === tab.id}
            className={value === tab.id ? "is-active" : undefined}
            onClick={() => onChange(tab.id)}
          >
            <span>{tab.label}</span>
            {badge && badge.count > 0 ? (
              <span className={`zorai-code-rail-tabs__badge${badge.tone === "danger" ? " is-danger" : ""}`}>{badge.count > 999 ? "999+" : badge.count}</span>
            ) : null}
          </button>
        );
      })}
    </div>
  );
}
