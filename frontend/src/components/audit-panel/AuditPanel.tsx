import { useEffect } from "react";
import { useAuditStore } from "../../lib/auditStore";
import { AuditHeader } from "./AuditHeader";
import { AuditList } from "./AuditList";
import { EscalationBanner } from "./EscalationBanner";

/**
 * Main slide-over audit panel, 440px wide from right.
 * Same layout pattern as NotificationPanel.
 * Opens via Ctrl+Shift+A or programmatic toggle.
 */
export function AuditPanel() {
  const isOpen = useAuditStore((s) => s.isOpen);
  const toggle = useAuditStore((s) => s.togglePanel);
  const currentEscalation = useAuditStore((s) => s.currentEscalation);
  const setEscalation = useAuditStore((s) => s.setEscalation);

  // Escape key closes panel
  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        toggle();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, toggle]);

  if (!isOpen) return null;

  const handleCancelEscalation = () => {
    // Send cancel to daemon via IPC bridge
    const zorai = (window as unknown as Record<string, unknown>).zorai ??
      (window as unknown as Record<string, unknown>).zorai;
    if (zorai && typeof (zorai as Record<string, unknown>).cancelEscalation === "function") {
      void (zorai as Record<string, (...args: unknown[]) => Promise<void>>).cancelEscalation(
        currentEscalation?.threadId,
      );
    }
    setEscalation(null);
  };

  return (
    <div
      onClick={toggle}
      style={{
        position: "fixed",
        inset: 0,
        background: "color-mix(in srgb, var(--shade) 56%, transparent)",
        zIndex: 900,
        display: "flex",
        justifyContent: "flex-end",
        backdropFilter: "none",
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 440,
          maxWidth: "90vw",
          height: "100%",
          background: "var(--zorai-bg)",
          borderLeft: "1px solid var(--glass-border)",
          display: "flex",
          flexDirection: "column",
          boxShadow: "none",
        }}
      >
        {currentEscalation && (
          <EscalationBanner
            escalation={currentEscalation}
            onCancel={handleCancelEscalation}
          />
        )}

        <AuditHeader onClose={toggle} />
        <AuditList />
      </div>
    </div>
  );
}
