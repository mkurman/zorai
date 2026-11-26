import type { BrowserWebview } from "./threadBrowserRegistry";

export type PickedElement = {
  url: string;
  tag: string;
  id: string;
  classes: string[];
  text: string;
  html: string;
  component: string | null;
  source: { fileName: string; lineNumber: number } | null;
  styles: Record<string, string>;
  rect?: { x: number; y: number; width: number; height: number };
  screenshot?: string | null;
};
export type CodeLocation = { path: string; line: number; reason: string };

const DEFAULT_STYLE_VALUES = new Set(["none", "normal", "auto", "static", "0px", "rgba(0, 0, 0, 0)", "0px none rgb(0, 0, 0)"]);
const STYLE_KEYS = ["display", "position", "width", "height", "margin", "padding", "color", "background-color", "font-size", "font-weight", "border", "border-radius", "gap", "flex-direction", "align-items", "justify-content"];

export const PICK_ELEMENT_SCRIPT = `new Promise((resolve) => {
  const box = document.createElement("div");
  box.style.cssText = "position:fixed;pointer-events:none;z-index:2147483647;border:2px solid #f5a524;background:rgba(245,165,36,.12);border-radius:3px;transition:all .05s";
  document.documentElement.appendChild(box);
  let current = null;
  const move = (event) => {
    current = event.target;
    const rect = current.getBoundingClientRect();
    Object.assign(box.style, { left: rect.left + "px", top: rect.top + "px", width: rect.width + "px", height: rect.height + "px" });
  };
  const component = (element) => {
    const key = Object.keys(element).find((name) => name.startsWith("__reactFiber$") || name.startsWith("__reactInternalInstance$"));
    let fiber = key ? element[key] : null;
    let source = null;
    while (fiber) {
      if (!source && fiber._debugSource) source = { fileName: fiber._debugSource.fileName, lineNumber: fiber._debugSource.lineNumber };
      const type = fiber.type;
      const name = typeof type === "function" ? (type.displayName || type.name) : type && typeof type === "object" ? (type.displayName || (type.render && (type.render.displayName || type.render.name))) : null;
      if (name && /^[A-Z]/.test(name)) return { name, source };
      fiber = fiber.return;
    }
    return { name: null, source };
  };
  const finish = (element) => {
    window.removeEventListener("mousemove", move, true);
    window.removeEventListener("click", click, true);
    window.removeEventListener("keydown", key, true);
    box.remove();
    if (!element) return resolve(null);
    const style = getComputedStyle(element);
    const info = component(element);
    resolve({
      url: location.href,
      tag: element.tagName.toLowerCase(),
      id: element.id || "",
      classes: Array.from(element.classList),
      text: (element.innerText || "").trim().slice(0, 120),
      html: element.outerHTML.slice(0, 600),
      component: info.name,
      source: info.source,
      rect: (() => { const box = element.getBoundingClientRect(); const pad = 8; return { x: Math.max(0, Math.round(box.left - pad)), y: Math.max(0, Math.round(box.top - pad)), width: Math.max(1, Math.round(box.width + pad * 2)), height: Math.max(1, Math.round(box.height + pad * 2)) }; })(),
      styles: Object.fromEntries(${JSON.stringify(STYLE_KEYS)}.map((name) => [name, style.getPropertyValue(name)])),
    });
  };
  const click = (event) => { event.preventDefault(); event.stopPropagation(); finish(current || event.target); };
  const key = (event) => { if (event.key === "Escape") { event.preventDefault(); finish(null); } };
  window.addEventListener("mousemove", move, true);
  window.addEventListener("click", click, true);
  window.addEventListener("keydown", key, true);
})`;

let latestScreenshot: string | null = null;

export function latestPickedScreenshot(): string | null {
  return latestScreenshot;
}

export async function pickElement(webview: BrowserWebview): Promise<PickedElement | null> {
  const picked = (await webview.executeJavaScript(PICK_ELEMENT_SCRIPT, true)) as PickedElement | null;
  if (!picked) return null;
  await new Promise((resolve) => window.setTimeout(resolve, 60));
  const image = await webview.capturePage(picked.rect).catch(() => null);
  const screenshot = image && !image.isEmpty?.() ? image.toDataURL() : null;
  if (screenshot) latestScreenshot = screenshot;
  return { ...picked, screenshot };
}

export function screenshotFile(picked: PickedElement): File | null {
  const match = /^data:(image\/\w+);base64,(.+)$/.exec(picked.screenshot ?? "");
  if (!match) return null;
  const binary = atob(match[2]);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  const name = `${picked.tag}${picked.classes[0] ? `-${picked.classes[0]}` : ""}.png`.replace(/[^\w.-]+/g, "-");
  return new File([bytes], name, { type: match[1] });
}

export function distinctiveClasses(classes: string[]): string[] {
  return classes.filter((name) => name.length > 2 && !/^(is|has)-|^(active|open|hidden|selected|disabled)$/.test(name) && !/^[a-z]{1,3}-\d/.test(name));
}

export function sourcePathFromDebug(fileName: string, root: string): string | null {
  const normalizedRoot = root.replace(/\\/g, "/").replace(/\/+$/, "");
  const normalized = fileName.replace(/\\/g, "/").replace(/^\/@fs/, "").replace(/\?.*$/, "");
  if (normalized.startsWith(`${normalizedRoot}/`)) return normalized.slice(normalizedRoot.length + 1);
  return /^[^/].*\.(t|j)sx?$/.test(normalized) ? normalized : null;
}

type Search = (query: string, options: { regex: boolean; include: string; maxResults: number }) => Promise<Array<{ path: string; line: number }>>;

export async function locatePickedElement(root: string, picked: PickedElement, search: Search): Promise<{ component: CodeLocation | null; style: CodeLocation | null }> {
  let component: CodeLocation | null = null;
  if (picked.source) {
    const path = sourcePathFromDebug(picked.source.fileName, root);
    if (path) component = { path, line: picked.source.lineNumber, reason: "React source" };
  }
  const escape = (value: string) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  if (!component && picked.component) {
    const [hit] = await search(`(function|const|class)\\s+${escape(picked.component)}\\b`, { regex: true, include: "*.tsx,*.jsx,*.ts,*.js,*.vue,*.svelte", maxResults: 1 }).catch(() => []);
    if (hit) component = { path: hit.path, line: hit.line, reason: `component ${picked.component}` };
  }
  const classes = distinctiveClasses(picked.classes);
  for (const name of classes) {
    if (component) break;
    const [hit] = await search(`["'\`\\s]${escape(name)}["'\`\\s]`, { regex: true, include: "*.tsx,*.jsx,*.vue,*.svelte,*.html", maxResults: 1 }).catch(() => []);
    if (hit) component = { path: hit.path, line: hit.line, reason: `class ${name}` };
  }
  let style: CodeLocation | null = null;
  for (const name of classes) {
    const [hit] = await search(`\\.${escape(name)}(?![\\w-])`, { regex: true, include: "*.css,*.scss,*.less", maxResults: 1 }).catch(() => []);
    if (hit) {
      style = { path: hit.path, line: hit.line, reason: `.${name}` };
      break;
    }
  }
  return { component, style };
}

export function describePickedElement(picked: PickedElement, located: { component: CodeLocation | null; style: CodeLocation | null }): string {
  const selector = `${picked.tag}${picked.id ? `#${picked.id}` : ""}${picked.classes.map((name) => `.${name}`).join("")}`;
  const styles = Object.entries(picked.styles).filter(([, value]) => value && !DEFAULT_STYLE_VALUES.has(value)).map(([name, value]) => `${name}: ${value}`).join("; ");
  return [
    `Picked element on ${picked.url}: \`${selector}\`${picked.component ? ` (React <${picked.component}>)` : ""}`,
    located.component ? `Component: ${located.component.path}:${located.component.line}` : "",
    located.style ? `Styles: ${located.style.path}:${located.style.line}` : "",
    picked.text ? `Text: "${picked.text}"` : "",
    `Computed: ${styles}`,
    `HTML: ${picked.html}`,
    "",
    "Change: ",
  ].filter((line, index, all) => line !== "" || index === all.length - 2).join("\n");
}
