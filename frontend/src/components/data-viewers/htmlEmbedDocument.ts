export const EMBED_CONTENT_SECURITY_POLICY = [
  "default-src 'none'",
  "script-src 'unsafe-inline'",
  "style-src 'unsafe-inline'",
  "img-src zorai-media: data: blob:",
  "media-src zorai-media: data: blob:",
  "font-src data:",
  "connect-src 'none'",
  "frame-src 'none'",
  "form-action 'none'",
  "base-uri zorai-media:",
].join("; ");

export const EMBED_HEIGHT_MESSAGE = "zoraiEmbedHeight";

const HEIGHT_REPORTER = `<script>(() => {
  const send = () => parent.postMessage({ ${EMBED_HEIGHT_MESSAGE}: Math.ceil(document.documentElement.scrollHeight) }, "*");
  addEventListener("DOMContentLoaded", () => { new ResizeObserver(send).observe(document.documentElement); send(); });
  addEventListener("load", send);
})();</script>`;

export function buildEmbedDocument(content: string, baseHref: string | null): string {
  const policy = `<meta http-equiv="Content-Security-Policy" content="${EMBED_CONTENT_SECURITY_POLICY}">`;
  const base = baseHref ? `<base href="${baseHref.replace(/"/g, "&quot;")}">` : "";
  return `${policy}${base}${HEIGHT_REPORTER}${content}`;
}

const AGENT_EMBED_TAG = /<agent-embed\s+[^>]*?src\s*=\s*["']([^"']+)["'][^>]*?(?:\/>|>\s*<\/agent-embed>)/gi;

export function expandAgentEmbedTags(markdown: string): string {
  if (!markdown.includes("<agent-embed")) return markdown;
  let fence: string | null = null;
  return markdown
    .split("\n")
    .map((line) => {
      const marker = /^\s*(`{3,}|~{3,})/.exec(line)?.[1];
      if (marker) {
        if (fence === null) fence = marker[0];
        else if (marker[0] === fence) fence = null;
        return line;
      }
      if (fence !== null) return line;
      return line.replace(AGENT_EMBED_TAG, (_tag, src: string) => `\n\`\`\`agent-embed\n${src.trim()}\n\`\`\`\n`);
    })
    .join("\n");
}

export function embedFileName(src: string): string {
  return src.replace(/^file:\/\//i, "").split(/[\\/]/).pop() || "embed.html";
}
