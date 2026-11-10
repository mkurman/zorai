import { useState } from "react";

export type CodeRailTab = "explorer" | "scm" | "testing" | "agent";

const STORAGE_KEY = "zorai.code.railTab";

const TABS: ReadonlyArray<{ id: CodeRailTab; label: string; icon: string }> = [
  { id: "explorer", label: "Explorer", icon: "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5M9 13h6M9 17h4" },
  { id: "scm", label: "Source Control", icon: "M6 3v12M6 15a3 3 0 1 0 0 6 3 3 0 0 0 0-6M18 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6M18 9a9 9 0 0 1-9 9" },
  { id: "testing", label: "Testing", icon: "M9 3h6M10 3v6L4.5 18.5A1.7 1.7 0 0 0 6 21h12a1.7 1.7 0 0 0 1.5-2.5L14 9V3M7 15h10" },
  { id: "agent", label: "Agent", icon: "M12 3v3M7 6h10a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2zM9.5 11.5v1M14.5 11.5v1M9.5 15.5h5M3 11v3M21 11v3" },
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
            aria-label={tab.label}
            title={tab.label}
            onClick={() => onChange(tab.id)}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d={tab.icon} /></svg>
            {badge && badge.count > 0 ? (
              <span className={`zorai-code-rail-tabs__badge${badge.tone === "danger" ? " is-danger" : ""}`}>{badge.count > 999 ? "999+" : badge.count}</span>
            ) : null}
          </button>
        );
      })}
    </div>
  );
}
