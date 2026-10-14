import type { CodeIconId } from "./codeCommands";

export function CodeIcon({ icon }: { icon: CodeIconId }) {
  const common = { viewBox: "0 0 24 24", "aria-hidden": true, focusable: false } as const;
  switch (icon) {
    case "save": return <svg {...common}><path d="M5 3h12l2 2v16H5zM8 3v6h8V3M8 21v-7h8v7" /></svg>;
    case "reload": return <svg {...common}><path d="M20 7v5h-5M4 17v-5h5M6.1 8a7 7 0 0 1 11.3-2L20 8M4 16l2.6 2a7 7 0 0 0 11.3-2" /></svg>;
    case "search": return <svg {...common}><circle cx="10.5" cy="10.5" r="6.5" /><path d="m15.5 15.5 5 5" /></svg>;
    case "replace": return <svg {...common}><path d="M4 7h14M14 3l4 4-4 4M20 17H6M10 13l-4 4 4 4" /></svg>;
    case "settings": return <svg {...common}><circle cx="12" cy="12" r="3" /><path d="M19 12a7 7 0 0 0-.1-1l2-1.5-2-3.4-2.4 1a7 7 0 0 0-1.7-1L14.5 3h-5L9 6a7 7 0 0 0-1.7 1L5 6.1 3 9.5 5 11a7 7 0 0 0 0 2l-2 1.5 2 3.4 2.4-1a7 7 0 0 0 1.7 1l.4 3h5l.5-3a7 7 0 0 0 1.7-1l2.3 1 2-3.4-2-1.5a7 7 0 0 0 0-1" /></svg>;
    case "close": return <svg {...common}><path d="M6 6l12 12M18 6 6 18" /></svg>;
    case "external": return <svg {...common}><path d="M14 4h6v6M20 4l-9 9M18 13v7H4V6h7" /></svg>;
    case "reveal": return <svg {...common}><path d="M3 6h7l2 2h9v11H3z" /><path d="M8 13h8M12 10l4 3-4 3" /></svg>;
    case "minimap": return <svg {...common}><rect x="4" y="3" width="16" height="18" rx="1" /><path d="M14 6h3M14 9h3M14 12h3M7 7h4M7 11h4M7 15h7" /></svg>;
    case "wrap": return <svg {...common}><path d="M4 7h12a4 4 0 0 1 0 8H9M12 12l-3 3 3 3M4 11h6" /></svg>;
    case "palette": return <svg {...common}><path d="M4 6h16M4 12h10M4 18h7" /><circle cx="18" cy="12" r="2" /></svg>;
    case "pin": return <svg {...common}><path d="m8 3 8 8-2 2 3 4-1 1-4-3-2 2-8-8zM8 16l-5 5" /></svg>;
    case "terminal": return <svg {...common}><path d="m4 6 5 5-5 5M11 18h9" /></svg>;
    case "text": return <svg {...common}><path d="M5 5h14M12 5v14M8 19h8" /></svg>;
    case "diff": return <svg {...common}><path d="M6 3v12M3 6h6M15 21V9M12 18h6M6 15a3 3 0 1 0 0 6 3 3 0 0 0 0-6M18 3a3 3 0 1 0 0 6 3 3 0 0 0 0-6" /></svg>;
    case "more": return <svg {...common}><circle cx="5" cy="12" r="1.4" /><circle cx="12" cy="12" r="1.4" /><circle cx="19" cy="12" r="1.4" /></svg>;
    case "edit": return <svg {...common}><path d="m4 20 4-1 11-11-3-3L5 16zM14 7l3 3" /></svg>;
    default: return <svg {...common}><path d="M5 3h10l4 4v14H5zM14 3v5h5" /></svg>;
  }
}
