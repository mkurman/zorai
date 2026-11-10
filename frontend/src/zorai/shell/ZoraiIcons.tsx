import type { ZoraiNavIconId } from "./navigation";

type IconProps = {
  icon: ZoraiNavIconId;
};

export function ZoraiNavIcon({ icon }: IconProps) {
  return (
    <svg
      className="zorai-nav-icon"
      viewBox="0 0 24 24"
      aria-hidden="true"
      focusable="false"
    >
      {renderIconPath(icon)}
    </svg>
  );
}

export const ZORAI_LOGO_PETAL = "M 50 45.82 A 37.67 37.67 0 0 1 50 11.32 A 37.67 37.67 0 0 1 50 45.82";
export const ZORAI_LOGO_WEDGE = "M 69.34 50 L 77.69 44.27 A 28.28 28.28 0 0 1 77.69 55.73 Z";
const ZORAI_LOGO_TURNS = [0, 60, 120, 180, 240, 300];

type ZoraiBrandMarkProps = {
  className?: string;
};

export function ZoraiBrandMark({ className = "" }: ZoraiBrandMarkProps) {
  return (
    <div className={`zorai-brand-mark ${className}`} aria-hidden="true">
      <svg className="zorai-brand-mark__svg" viewBox="0 0 100 100" focusable="false">
        <circle className="zorai-brand-mark__line" cx="50" cy="50" r="42.86" pathLength="1" />
        {ZORAI_LOGO_TURNS.map((turn) => (
          <path key={`petal-${turn}`} className="zorai-brand-mark__line" d={ZORAI_LOGO_PETAL} transform={`rotate(${turn} 50 50)`} pathLength="1" />
        ))}
        {ZORAI_LOGO_TURNS.map((turn) => (
          <path key={`wedge-${turn}`} className="zorai-brand-mark__line" d={ZORAI_LOGO_WEDGE} transform={`rotate(${turn} 50 50)`} pathLength="1" />
        ))}
      </svg>
    </div>
  );
}

export function ZoraiHamburgerIcon() {
  return (
    <svg
      className="zorai-nav-icon"
      viewBox="0 0 24 24"
      aria-hidden="true"
      focusable="false"
    >
      <path d="M5 7h14" />
      <path d="M5 12h14" />
      <path d="M5 17h14" />
    </svg>
  );
}

function renderIconPath(icon: ZoraiNavIconId) {
  if (icon === "code") {
    return (
      <>
        <path d="m8 7-5 5 5 5" />
        <path d="m16 7 5 5-5 5" />
        <path d="m13.5 4-3 16" />
      </>
    );
  }

  if (icon === "threads") {
    return (
      <>
        <path d="M5 6.5h14v7H9l-4 4v-11Z" />
        <path d="M8.5 9h7" />
        <path d="M8.5 11.5h4.5" />
      </>
    );
  }

  if (icon === "goals") {
    return (
      <>
        <circle cx="12" cy="12" r="7" />
        <circle cx="12" cy="12" r="3" />
        <path d="M12 5v3" />
        <path d="M12 16v3" />
        <path d="M5 12h3" />
        <path d="M16 12h3" />
      </>
    );
  }

  if (icon === "workspaces") {
    return (
      <>
        <rect x="4" y="5" width="7" height="6" rx="1" />
        <rect x="13" y="5" width="7" height="6" rx="1" />
        <rect x="4" y="13" width="7" height="6" rx="1" />
        <rect x="13" y="13" width="7" height="6" rx="1" />
      </>
    );
  }

  if (icon === "tools") {
    return (
      <>
        <path d="M14.5 5.5 18 2l4 4-3.5 3.5" />
        <path d="M13 7 5 15l-1 5 5-1 8-8" />
        <path d="M6.5 16.5 7.5 17.5" />
      </>
    );
  }

  if (icon === "database") {
    return (
      <>
        <ellipse cx="12" cy="6" rx="7" ry="3" />
        <path d="M5 6v6c0 1.7 3.1 3 7 3s7-1.3 7-3V6" />
        <path d="M5 12v6c0 1.7 3.1 3 7 3s7-1.3 7-3v-6" />
      </>
    );
  }

  if (icon === "skills") {
    return (
      <>
        <path d="m4 20 11-11" />
        <path d="m13.5 7.5 3 3" />
        <path d="M18 3v4M16 5h4" />
        <path d="M9 3v2M8 4h2" />
        <path d="M19 12v2M18 13h2" />
      </>
    );
  }

  if (icon === "activity") {
    return (
      <>
        <path d="M4 13h4l2-6 4 10 2-4h4" />
        <path d="M4 19h16" />
      </>
    );
  }

  if (icon === "nightMode") {
    return <path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z" />;
  }

  if (icon === "lightMode") {
    return (
      <>
        <circle cx="12" cy="12" r="3" />
        <path d="M12 3v3" />
        <path d="M12 18v3" />
        <path d="M3 12h3" />
        <path d="M18 12h3" />
        <path d="m5.6 5.6 2.1 2.1" />
        <path d="m16.3 16.3 2.1 2.1" />
        <path d="m18.4 5.6-2.1 2.1" />
        <path d="m7.7 16.3-2.1 2.1" />
      </>
    );
  }

  return (
    <>
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 13a7.7 7.7 0 0 0 .1-2l2.1-1.6-2-3.4-2.5 1a7.8 7.8 0 0 0-1.7-1l-.4-2.7h-4l-.4 2.7a7.8 7.8 0 0 0-1.7 1l-2.5-1-2 3.4L4.5 11a7.7 7.7 0 0 0 .1 2l-2.1 1.6 2 3.4 2.5-1a7.8 7.8 0 0 0 1.7 1l.4 2.7h4l.4-2.7a7.8 7.8 0 0 0 1.7-1l2.5 1 2-3.4Z" />
    </>
  );
}
