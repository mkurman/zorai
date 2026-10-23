import type { ReactNode } from "react";
import type { ZoraiSettingsIconId } from "./settingsTabs";

const ICON_PATHS: Record<ZoraiSettingsIconId, ReactNode> = {
  key: (
    <>
      <circle cx="8" cy="15" r="4" />
      <path d="m11 12 9-9" />
      <path d="m17 6 3 3" />
      <path d="m14.5 8.5 2 2" />
    </>
  ),
  spark: (
    <>
      <path d="M12 3v4" />
      <path d="M12 17v4" />
      <path d="M3 12h4" />
      <path d="M17 12h4" />
      <path d="m9 9 3-2 3 2 2 3-2 3-3 2-3-2-2-3Z" />
    </>
  ),
  compass: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="m15.5 8.5-2 5-5 2 2-5Z" />
    </>
  ),
  users: (
    <>
      <circle cx="9" cy="8" r="3.5" />
      <path d="M3 20c0-3.3 2.7-6 6-6s6 2.7 6 6" />
      <path d="M16 4.5a3.5 3.5 0 0 1 0 7" />
      <path d="M18 14.5c1.8.8 3 2.9 3 5.5" />
    </>
  ),
  wrench: (
    <path d="M14.5 6.5a4 4 0 0 0 5 5L12 19a2.1 2.1 0 0 1-3-3l7.5-7.5a4 4 0 0 1-2-2Z" />
  ),
  puzzle: (
    <path d="M10 4h4v2.5a1.5 1.5 0 0 0 3 0V4h3v6h-2.5a1.5 1.5 0 0 0 0 3H20v7h-6v-2.5a1.5 1.5 0 0 0-3 0V20H4v-7h2.5a1.5 1.5 0 0 0 0-3H4V4h6Z" />
  ),
  globe: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M3 12h18" />
      <path d="M12 3c2.5 2.5 3.8 5.5 3.8 9s-1.3 6.5-3.8 9c-2.5-2.5-3.8-5.5-3.8-9S9.5 5.5 12 3Z" />
    </>
  ),
  wave: (
    <>
      <path d="M4 10v4" />
      <path d="M8 6v12" />
      <path d="M12 9v6" />
      <path d="M16 4v16" />
      <path d="M20 10v4" />
    </>
  ),
  brain: (
    <>
      <path d="M9 4a3 3 0 0 0-3 3 3 3 0 0 0-2 5 3 3 0 0 0 2 5 3 3 0 0 0 6 1V5a2 2 0 0 0-3-1Z" />
      <path d="M15 4a3 3 0 0 1 3 3 3 3 0 0 1 2 5 3 3 0 0 1-2 5 3 3 0 0 1-6 1" />
      <path d="M12 9h2.5" />
      <path d="M12 14h2.5" />
    </>
  ),
  pulse: <path d="M3 12h4l2-5 4 10 2-5h6" />,
  radar: (
    <>
      <circle cx="12" cy="12" r="9" />
      <circle cx="12" cy="12" r="5" />
      <path d="m12 12 6-6" />
    </>
  ),
  flow: (
    <>
      <rect x="3" y="4" width="6" height="5" rx="1" />
      <rect x="15" y="15" width="6" height="5" rx="1" />
      <path d="M6 9v4a2 2 0 0 0 2 2h7" />
      <path d="m13 13 2 2-2 2" />
    </>
  ),
  message: (
    <>
      <path d="M4 5h16v11H9l-5 4V5Z" />
      <path d="M8 9h8" />
      <path d="M8 12h5" />
    </>
  ),
  chat: (
    <>
      <path d="M4 5h11v8H8l-4 3V5Z" />
      <path d="M15 9h5v8l-3-2h-6v-2" />
    </>
  ),
  layers: (
    <>
      <path d="m12 3 9 5-9 5-9-5 9-5Z" />
      <path d="m3 13 9 5 9-5" />
    </>
  ),
  gauge: (
    <>
      <path d="M4 17a8 8 0 1 1 16 0" />
      <path d="m12 17 4-6" />
      <path d="M4 17h16" />
    </>
  ),
  shield: (
    <>
      <path d="M12 3 5 6v5c0 4.5 3 8.5 7 10 4-1.5 7-5.5 7-10V6l-7-3Z" />
      <path d="m9 12 2 2 4-4" />
    </>
  ),
  activity: (
    <>
      <path d="M4 19V5" />
      <path d="M4 19h16" />
      <path d="m8 15 3-4 3 2 5-6" />
    </>
  ),
  disk: (
    <>
      <ellipse cx="12" cy="6" rx="8" ry="3" />
      <path d="M4 6v12c0 1.7 3.6 3 8 3s8-1.3 8-3V6" />
      <path d="M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3" />
    </>
  ),
  plug: (
    <>
      <path d="M9 3v5" />
      <path d="M15 3v5" />
      <path d="M6 8h12v3a6 6 0 0 1-12 0V8Z" />
      <path d="M12 17v4" />
    </>
  ),
  palette: (
    <>
      <path d="M12 3a9 9 0 0 0 0 18c1.4 0 2-1 2-2s-1-1.5-1-2.5 1-1.5 2-1.5h2a4 4 0 0 0 4-4c0-4.4-4-8-9-8Z" />
      <circle cx="7.5" cy="11" r="1" />
      <circle cx="10" cy="7" r="1" />
      <circle cx="15" cy="7.5" r="1" />
    </>
  ),
  import: (
    <>
      <path d="M12 3v12" />
      <path d="m7 10 5 5 5-5" />
      <path d="M4 17v3h16v-3" />
    </>
  ),
  info: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 11v6" />
      <path d="M12 7.5v.5" />
    </>
  ),
};

export function SettingsIcon({ icon }: { icon: ZoraiSettingsIconId }) {
  return (
    <svg className="zorai-settings-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      {ICON_PATHS[icon]}
    </svg>
  );
}
