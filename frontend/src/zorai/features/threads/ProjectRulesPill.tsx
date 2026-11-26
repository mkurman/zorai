import { useEffect, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { CodeContextMenu } from "@/zorai/features/code/CodeToolbarMenu";

type ProjectRule = { name: string; description: string; globs: string[]; applied: boolean };

export function ProjectRulesPill({ root, targets, onOpen }: { root: string; targets: string[]; onOpen: (path: string) => void }) {
  const [rules, setRules] = useState<ProjectRule[]>([]);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const targetsKey = targets.join("\n");

  useEffect(() => {
    const bridge = getBridge();
    if (!bridge?.agentListProjectRules) return;
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void bridge.agentListProjectRules!(root, targetsKey ? targetsKey.split("\n") : []).then((next) => { if (!cancelled) setRules(next); }).catch(() => undefined);
    }, 300);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [root, targetsKey]);

  const applied = rules.filter((rule) => rule.applied);
  if (rules.length === 0) return null;

  return (
    <>
      <button
        type="button"
        className={applied.length > 0 ? "zorai-composer-context-chip zorai-rules-pill is-active" : "zorai-composer-context-chip zorai-rules-pill"}
        title={`${applied.length > 0 ? `Project rules in effect: ${applied.map((rule) => rule.name).join(", ")}` : "No project rule matches the files in play"}. Click to open a rule file.`}
        onClick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          setMenu({ x: rect.left, y: rect.top });
        }}
      >
        {applied.length} / {rules.length} rules
      </button>
      {menu ? (
        <CodeContextMenu
          x={menu.x}
          y={menu.y}
          label="Project rules"
          onClose={() => setMenu(null)}
          items={rules.map((rule) => ({
            key: rule.name,
            label: `${rule.name}${rule.globs.length > 0 ? ` — ${rule.globs.join(", ")}` : rule.description ? ` — ${rule.description}` : ""}`,
            checked: rule.applied,
            onSelect: () => onOpen(rule.name),
          }))}
        />
      ) : null}
    </>
  );
}
