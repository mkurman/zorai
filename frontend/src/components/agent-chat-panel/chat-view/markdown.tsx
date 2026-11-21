import { memo, useContext, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import type { Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import renderMathInElement from "katex/contrib/auto-render";
import "katex/dist/katex.min.css";
import { FencedDataBlock, fencedBlockKind } from "@/components/data-viewers/FencedDataBlock";
import { MarkdownTable, hastText } from "@/components/data-viewers/MarkdownTable";
import { ThreadFilePreviewContext } from "@/zorai/features/threads/ThreadFilePreviewContext";
import { ImageLightbox } from "@/zorai/features/threads/ImageLightbox";
import { getBridge } from "@/lib/bridge";
import { useAgentStore } from "@/lib/agentStore";
import { useWorkspaceContextStore } from "@/lib/workspaceContextStore";
import { pushToast } from "@/lib/toastStore";
import { Presence } from "@/components/Presence";
import { CodeFileIcon } from "@/zorai/features/code/CodeFileIcon";
import { looksLikeFileReference } from "@/zorai/features/code/codeFileTypes";
import { fileLinkCandidatesFromTools } from "./fileLinkCandidates";
import { MathSegmentsContext } from "./mathText";

export const KATEX_DELIMITERS = [
  { left: "$$", right: "$$", display: true },
  { left: "\\[", right: "\\]", display: true },
  { left: "\\begin{equation}", right: "\\end{equation}", display: true },
  { left: "\\begin{equation*}", right: "\\end{equation*}", display: true },
  { left: "\\begin{align}", right: "\\end{align}", display: true },
  { left: "\\begin{align*}", right: "\\end{align*}", display: true },
  { left: "\\begin{gather}", right: "\\end{gather}", display: true },
  { left: "\\begin{gather*}", right: "\\end{gather*}", display: true },
  { left: "\\(", right: "\\)", display: false },
] as const;

export function inlineDollarMathEnd(source: string, start: number): number {
  const next = source[start + 1];
  if (!next || /\s|\$/.test(next)) return -1;
  const lineEnd = source.indexOf("\n", start);
  const limit = lineEnd < 0 ? source.length : lineEnd;
  for (let index = start + 2; index < limit; index += 1) {
    if (source[index] !== "$" || source[index - 1] === "\\") continue;
    if (/\s/.test(source[index - 1]) || /\d/.test(source[index + 1] ?? "")) continue;
    return index;
  }
  return -1;
}

export function isRelativeFileHref(href: string | undefined): boolean {
  if (!href) return false;
  return !/^[A-Za-z][A-Za-z0-9+.-]*:/.test(href) && !/^[/#?\\]/.test(href);
}

export function localFilePathFromHref(href: string | undefined, root?: string | null): string | null {
  if (!href) return null;
  let path: string;
  if (/^file:\/\//i.test(href)) {
    try {
      path = decodeURIComponent(new URL(href).pathname);
    } catch {
      return null;
    }
    if (/^\/[A-Za-z]:\//.test(path)) path = path.slice(1);
  } else if ((href.startsWith("/") && !href.startsWith("//")) || /^[A-Za-z]:[\\/]/.test(href)) {
    try {
      path = decodeURIComponent(href);
    } catch {
      path = href;
    }
  } else if (isRelativeFileHref(href) && root) {
    let relative: string;
    try {
      relative = decodeURIComponent(href);
    } catch {
      relative = href;
    }
    const segments = root.replace(/[\\/]+$/, "").split("/");
    for (const segment of relative.replace(/[?#].*$/, "").split("/")) {
      if (segment === "" || segment === ".") continue;
      if (segment === "..") {
        if (segments.length > 1) segments.pop();
        continue;
      }
      segments.push(segment);
    }
    path = segments.join("/");
  } else {
    return null;
  }
  return path.replace(/#.*$/, "").replace(/:\d+(?::\d+)?$/, "") || null;
}

async function activeThreadLinkRoots(): Promise<string[]> {
  const { activeThreadId, threads } = useAgentStore.getState();
  if (!activeThreadId) return [];
  const roots: string[] = [];
  const localRoot = useWorkspaceContextStore.getState().byThreadId[activeThreadId]?.root;
  const daemonThreadId = threads.find((thread) => thread.id === activeThreadId)?.daemonThreadId;
  if (localRoot) {
    roots.push(localRoot);
  } else if (daemonThreadId) {
    const reply = await getBridge()?.agentGetThreadWorkspaceContext?.(daemonThreadId);
    const root = (reply?.context as { root?: unknown } | null | undefined)?.root;
    if (typeof root === "string" && root) roots.push(root);
  }
  const dataDir = daemonThreadId ? await getBridge()?.getDataDir?.() : null;
  if (dataDir) roots.push(`${dataDir.replace(/[\\/]+$/, "")}/threads/${daemonThreadId}`);
  return roots;
}

async function resolveThreadFileLink(href: string): Promise<string | null> {
  const { activeThreadId, messages } = useAgentStore.getState();
  const dataDir = activeThreadId ? await getBridge()?.getDataDir?.() : null;
  const homeDir = dataDir && /[\\/]\.zorai[\\/]*$/.test(dataDir) ? dataDir.replace(/[\\/]\.zorai[\\/]*$/, "") : null;
  for (const candidate of fileLinkCandidatesFromTools(href, activeThreadId ? messages[activeThreadId] ?? [] : [], homeDir)) {
    if (await localPathExists(candidate)) return candidate;
  }
  return resolveRelativeFileLink(href, await activeThreadLinkRoots(), localPathExists);
}

async function localPathExists(path: string): Promise<boolean> {
  const slash = path.lastIndexOf("/");
  if (slash <= 0) return false;
  const entries = await getBridge()?.workspaceListDirectory?.(path.slice(0, slash), "").catch(() => null);
  return Boolean(entries?.some((entry) => entry.name === path.slice(slash + 1)));
}

export async function resolveRelativeFileLink(
  href: string,
  roots: string[],
  exists: (path: string) => Promise<boolean>,
): Promise<string | null> {
  const candidates = roots.map((root) => localFilePathFromHref(href, root)).filter((path): path is string => Boolean(path));
  for (const candidate of candidates) {
    if (await exists(candidate)) return candidate;
  }
  return candidates[0] ?? null;
}

function MarkdownLink({ href, children }: { href?: string; children?: ReactNode }) {
  const filePreview = useContext(ThreadFilePreviewContext);
  const openPreview = (path: string) => filePreview?.openThreadFilePreview({
    path,
    kind: "artifact",
    source: "message-link",
    isText: true,
    updatedAt: Date.now(),
  });
  return (
    <a
      href={href}
      target="_blank"
      rel="noopener noreferrer"
      onClick={(event) => {
        if (!href) return;
        event.preventDefault();
        if (filePreview && isRelativeFileHref(href)) {
          void resolveThreadFileLink(href).then((resolved) => {
            if (resolved) {
              openPreview(resolved);
            } else {
              pushToast(`Cannot open ${href}: this thread has no workspace or thread folder to resolve it against.`, "error");
            }
          });
          return;
        }
        const localPath = filePreview ? localFilePathFromHref(href) : null;
        if (localPath) {
          openPreview(localPath);
          return;
        }
        window.open(href, "_blank", "noopener,noreferrer");
      }}
    >
      {children}
    </a>
  );
}

function MarkdownImage({ src, alt }: { src?: string; alt?: string }) {
  const [open, setOpen] = useState(false);
  if (!src) return null;
  return (
    <>
      <img
        className="acp-md__image"
        src={src}
        alt={alt ?? ""}
        title={alt ? `${alt} — click to enlarge` : "Click to enlarge"}
        onClick={() => setOpen(true)}
      />
      <Presence value={open}>{(_, closing) => <ImageLightbox src={src} name={alt || "Image"} closing={closing} onClose={() => setOpen(false)} />}</Presence>
    </>
  );
}

const markdownComponents: Components = {
  a: ({ href, children }) => (
    <MarkdownLink href={href}>
      {href && looksLikeFileReference(href.replace(/^file:\/\//i, "").split(/[?#]/)[0]) ? <CodeFileIcon path={href.split(/[?#]/)[0]} /> : null}
      {children}
    </MarkdownLink>
  ),
  img: ({ src, alt }) => <MarkdownImage src={typeof src === "string" ? src : undefined} alt={alt} />,
  pre: ({ node, children }) => {
    const code = node?.children.find((child) => child.type === "element" && child.tagName === "code");
    const className = code && code.type === "element" ? code.properties.className : undefined;
    const language = Array.isArray(className)
      ? String(className.find((value) => String(value).startsWith("language-")) ?? "").slice("language-".length)
      : "";
    if (code && language && fencedBlockKind(language)) {
      return <FencedDataBlock language={language} source={hastText(code).replace(/\n$/, "")} />;
    }
    return <pre>{children}</pre>;
  },
  code: ({ className, children }) => {
    const isBlock = className?.startsWith("language-");
    if (isBlock) {
      return <code>{children}</code>;
    }
    const text = typeof children === "string" ? children : null;
    if (text && looksLikeFileReference(text)) {
      const path = text.replace(/:\d+(?::\d+)?$/, "");
      return (
        <MarkdownLink href={path}>
          <code className="acp-md-file-ref"><CodeFileIcon path={path} />{text}</code>
        </MarkdownLink>
      );
    }
    return <code>{children}</code>;
  },
  h1: ({ children }) => <h4>{children}</h4>,
  h2: ({ children }) => <h5>{children}</h5>,
  h3: ({ children }) => <h6>{children}</h6>,
  h4: ({ children }) => <h6>{children}</h6>,
  h5: ({ children }) => <h6>{children}</h6>,
  h6: ({ children }) => <h6>{children}</h6>,
  table: ({ node, children }) => <MarkdownTable node={node}>{children}</MarkdownTable>,
};

const MATH_TOKEN_OPEN = "\uE100";
const MATH_TOKEN_CLOSE = "\uE101";

export type ProtectedMath = {
  content: string;
  segments: string[];
};

const MATH_PAIRS = [
  { left: "\\begin{equation*}", right: "\\end{equation*}" },
  { left: "\\begin{equation}", right: "\\end{equation}" },
  { left: "\\begin{align*}", right: "\\end{align*}" },
  { left: "\\begin{align}", right: "\\end{align}" },
  { left: "\\begin{gather*}", right: "\\end{gather*}" },
  { left: "\\begin{gather}", right: "\\end{gather}" },
  { left: "\\[", right: "\\]" },
  { left: "\\(", right: "\\)" },
  { left: "$$", right: "$$" },
] as const;

export function protectMathSegments(source: string): ProtectedMath {
  const segments: string[] = [];
  let output = "";
  let cursor = 0;
  let fence: "`" | "~" | null = null;

  while (cursor < source.length) {
    if (cursor === 0 || source[cursor - 1] === "\n") {
      const lineEnd = source.indexOf("\n", cursor);
      const end = lineEnd < 0 ? source.length : lineEnd;
      const line = source.slice(cursor, end);
      const fenceMatch = line.match(/^\s*(`{3,}|~{3,})/);
      if (fenceMatch) {
        const marker = fenceMatch[1][0] as "`" | "~";
        if (fence === null) fence = marker;
        else if (fence === marker) fence = null;
        output += source.slice(cursor, lineEnd < 0 ? end : end + 1);
        cursor = lineEnd < 0 ? end : end + 1;
        continue;
      }
    }

    if (fence !== null) {
      output += source[cursor];
      cursor += 1;
      continue;
    }

    if (source[cursor] === "`") {
      let ticks = 1;
      while (source[cursor + ticks] === "`") ticks += 1;
      const marker = "`".repeat(ticks);
      const closing = source.indexOf(marker, cursor + ticks);
      const end = closing < 0 ? source.length : closing + ticks;
      output += source.slice(cursor, end);
      cursor = end;
      continue;
    }

    const pair = MATH_PAIRS.find(({ left }) => source.startsWith(left, cursor));
    if (pair) {
      const closing = source.indexOf(pair.right, cursor + pair.left.length);
      if (closing >= 0) {
        const end = closing + pair.right.length;
        const index = segments.push(source.slice(cursor, end)) - 1;
        output += `${MATH_TOKEN_OPEN}${index}${MATH_TOKEN_CLOSE}`;
        cursor = end;
        continue;
      }
    }

    if (source[cursor] === "$" && source[cursor - 1] !== "\\") {
      if (source[cursor + 1] === "$") {
        output += "$$";
        cursor += 2;
        continue;
      }
      const end = inlineDollarMathEnd(source, cursor);
      if (end >= 0) {
        const index = segments.push(`\\(${source.slice(cursor + 1, end)}\\)`) - 1;
        output += `${MATH_TOKEN_OPEN}${index}${MATH_TOKEN_CLOSE}`;
        cursor = end + 1;
        continue;
      }
    }

    output += source[cursor];
    cursor += 1;
  }

  return { content: output, segments };
}

function restoreMathSegments(root: HTMLElement, segments: string[]): void {
  if (segments.length === 0) return;
  const tokenPattern = new RegExp(`${MATH_TOKEN_OPEN}(\\d+)${MATH_TOKEN_CLOSE}`, "g");
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const textNodes: Text[] = [];
  while (walker.nextNode()) textNodes.push(walker.currentNode as Text);
  for (const node of textNodes) {
    if (node.parentElement?.closest("pre, code, .no-math, .katex")) continue;
    const restored = node.data.replace(tokenPattern, (_match, rawIndex: string) => {
      const index = Number(rawIndex);
      return Number.isInteger(index) ? segments[index] ?? _match : _match;
    });
    if (restored !== node.data) node.data = restored;
  }
}

export function markdownRenderMode(streaming: boolean | undefined): "plain" | "markdown" {
  return streaming ? "plain" : "markdown";
}

export type StreamingMarkdownBlocks = { blocks: string[]; consumed: number; text: string };

const DISPLAY_MATH_OPEN_CLOSE = new Map([["$$", "$$"], ["\\[", "\\]"]]);

export function advanceStreamingMarkdownBlocks(previous: StreamingMarkdownBlocks, content: string): StreamingMarkdownBlocks {
  const committed = content.startsWith(previous.text)
    ? previous
    : { blocks: [], consumed: 0, text: "" };
  const blocks = [...committed.blocks];
  let blockStart = committed.consumed;
  let cursor = blockStart;
  let fence: string | null = null;
  let math: string | null = null;
  let blankAt = -1;
  while (cursor < content.length) {
    const lineEnd = content.indexOf("\n", cursor);
    if (lineEnd < 0) break;
    const line = content.slice(cursor, lineEnd);
    const trimmed = line.trim();
    if (blankAt >= 0 && trimmed !== "" && fence === null && math === null) {
      if (!/^[ \t]/.test(line)) {
        blocks.push(content.slice(blockStart, cursor));
        blockStart = cursor;
      }
      blankAt = -1;
    }
    if (fence !== null) {
      if (trimmed.startsWith(fence) && trimmed.slice(fence.length).trim() === "") fence = null;
    } else if (math !== null) {
      if (trimmed === math) math = null;
    } else {
      const fenceMatch = line.match(/^\s{0,3}(`{3,}|~{3,})/);
      if (fenceMatch) fence = fenceMatch[1];
      else if (DISPLAY_MATH_OPEN_CLOSE.has(trimmed)) math = DISPLAY_MATH_OPEN_CLOSE.get(trimmed) ?? null;
      else if (trimmed === "") blankAt = cursor;
    }
    cursor = lineEnd + 1;
  }
  if (blankAt >= 0 && cursor < content.length && /^[^ \t\r\n]/.test(content.slice(cursor, cursor + 1))) {
    blocks.push(content.slice(blockStart, cursor));
    blockStart = cursor;
  }
  if (blocks.length === committed.blocks.length) return committed;
  return { blocks, consumed: blockStart, text: content.slice(0, blockStart) };
}

const StreamingMarkdown = memo(function StreamingMarkdown({ content }: { content: string }) {
  const stateRef = useRef<StreamingMarkdownBlocks>({ blocks: [], consumed: 0, text: "" });
  stateRef.current = advanceStreamingMarkdownBlocks(stateRef.current, content);
  const { blocks, consumed } = stateRef.current;
  const tail = content.slice(consumed);
  return (
    <div className="acp-md-stream">
      {blocks.map((block, index) => <MarkdownContent key={index} content={block} />)}
      {tail ? <div className="acp-md acp-md--streaming">{tail}</div> : null}
    </div>
  );
});

export const MarkdownContent = memo(function MarkdownContent({
  content,
  streaming = false,
}: {
  content: string;
  streaming?: boolean;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  const renderMode = markdownRenderMode(streaming);
  const protectedMath = useMemo(
    () => renderMode === "markdown" ? protectMathSegments(content) : { content, segments: [] },
    [content, renderMode],
  );

  const mathRendering = useMemo(() => ({
    segments: protectedMath.segments,
    render: (root: HTMLElement) => {
      restoreMathSegments(root, protectedMath.segments);
      renderMathInElement(root, {
        delimiters: [...KATEX_DELIMITERS],
        ignoredTags: ["script", "noscript", "style", "textarea", "pre", "code", "option"],
        ignoredClasses: ["no-math", "katex", "katex-display"],
        throwOnError: false,
        strict: "ignore",
      });
    },
  }), [protectedMath]);

  useEffect(() => {
    const root = rootRef.current;
    if (!root || renderMode === "plain") return;
    mathRendering.render(root);
  }, [mathRendering, renderMode]);

  if (renderMode === "plain") {
    return <StreamingMarkdown content={content} />;
  }
  return (
    <div ref={rootRef} className="acp-md">
      <MathSegmentsContext.Provider value={mathRendering}>
        <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents}>
          {protectedMath.content}
        </ReactMarkdown>
      </MathSegmentsContext.Provider>
    </div>
  );
});
