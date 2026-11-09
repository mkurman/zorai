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

export function ZoraiBrandMark() {
  return (
    <div className="zorai-brand-mark" aria-hidden="true">
      <span></span>
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
        <path d="M4 5.5A1.5 1.5 0 0 1 5.5 4H11v16H5.5A1.5 1.5 0 0 1 4 18.5z" />
        <path d="M20 5.5A1.5 1.5 0 0 0 18.5 4H13v16h5.5a1.5 1.5 0 0 0 1.5-1.5z" />
        <path d="M7 8h1.5M7 11h1.5M15.5 8H17M15.5 11H17" />
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
