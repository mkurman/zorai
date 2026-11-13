import { useEffect, useId, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import { useAgentStore } from "@/lib/agentStore";
import {
  SECURITY_SHIELD_MENU_WIDTH,
  securityShieldFill,
  securityShieldMenuPosition,
  securityShieldMuted,
} from "./managedSecurityModel";
import {
  applyManagedSecurityLevel,
  managedSecurityLevels,
  type ThreadManagedSecurityLevel,
} from "./threadRuntimeActions";
import { usePresence } from "@/hooks/usePresence";

export function ManagedSecurityShield() {
  const level = useAgentStore((state) => state.agentSettings.managed_security_level);
  const [open, setOpen] = useState(false);
  const presence = usePresence(open);
  const [menuStyle, setMenuStyle] = useState<CSSProperties>({});
  const rootRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const clipId = useId().replace(/:/g, "");
  const fill = securityShieldFill(level);
  const muted = securityShieldMuted(level);

  useLayoutEffect(() => {
    if (!open || !buttonRef.current) return;
    const update = () => {
      if (!buttonRef.current) return;
      const rect = buttonRef.current.getBoundingClientRect();
      const position = securityShieldMenuPosition(rect, {
        width: window.innerWidth,
        height: window.innerHeight,
      });
      setMenuStyle({
        position: "fixed",
        left: position.left,
        right: "auto",
        bottom: position.bottom,
        zIndex: 90,
        minWidth: SECURITY_SHIELD_MENU_WIDTH,
      });
    };
    update();
    window.addEventListener("resize", update);
    window.addEventListener("scroll", update, true);
    return () => {
      window.removeEventListener("resize", update);
      window.removeEventListener("scroll", update, true);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (rootRef.current?.contains(target) || menuRef.current?.contains(target)) return;
      setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  const menu =
    presence.mounted && typeof document !== "undefined"
      ? createPortal(
          <div data-closing={presence.closing || undefined}
            ref={menuRef}
            className="zorai-security-shield__menu"
            role="menu"
            aria-label="Managed security mode"
            style={menuStyle}
          >
            {managedSecurityLevels().map((option) => (
              <button
                type="button"
                role="menuitemradio"
                aria-checked={option === level}
                key={option}
                className={option === level ? "is-active" : ""}
                onClick={() => {
                  setOpen(false);
                  void applyManagedSecurityLevel(option as ThreadManagedSecurityLevel);
                }}
              >
                {option}
              </button>
            ))}
          </div>,
          document.body,
        )
      : null;

  return (
    <div ref={rootRef} className="zorai-security-shield">
      <button
        ref={buttonRef}
        type="button"
        className={["zorai-composer-icon-button", muted ? "zorai-security-shield--muted" : ""]
          .filter(Boolean)
          .join(" ")}
        title={`Managed security mode: ${level}`}
        aria-label="Managed security mode"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((current) => !current)}
      >
        <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
          <defs>
            <clipPath id={clipId}>
              <rect x="0" y={24 * (1 - fill)} width="24" height={24 * fill} />
            </clipPath>
          </defs>
          <path
            d="M12 3 5 6.2v5.3c0 4.6 3 8.4 7 9.5 4-1.1 7-4.9 7-9.5V6.2L12 3z"
            fill="currentColor"
            opacity="0.16"
          />
          <path
            d="M12 3 5 6.2v5.3c0 4.6 3 8.4 7 9.5 4-1.1 7-4.9 7-9.5V6.2L12 3z"
            fill="currentColor"
            clipPath={`url(#${clipId})`}
          />
          <path
            d="M12 3 5 6.2v5.3c0 4.6 3 8.4 7 9.5 4-1.1 7-4.9 7-9.5V6.2L12 3z"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.7"
            strokeLinejoin="round"
          />
        </svg>
      </button>
      {menu}
    </div>
  );
}
