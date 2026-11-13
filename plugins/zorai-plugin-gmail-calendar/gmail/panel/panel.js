let nextId = 0;
const pending = new Map();
function zorai(method, params) {
  const id = ++nextId;
  parent.postMessage({ zorai: "plugin-panel", id, method, params }, "*");
  return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
}
addEventListener("message", (event) => {
  const message = event.data;
  if (message?.zorai !== "plugin-panel") return;
  if (message.event === "theme") return applyTheme(message.theme);
  const request = pending.get(message.id);
  if (!request) return;
  pending.delete(message.id);
  message.ok ? request.resolve(message.result) : request.reject(new Error(message.error));
});
async function call(endpoint, params = {}) {
  const text = await zorai("call", { endpoint, params });
  try { return JSON.parse(text); } catch { return text; }
}
function applyTheme(theme) {
  if (!theme) return;
  for (const [key, value] of Object.entries(theme)) if (key.startsWith("--") && value) document.documentElement.style.setProperty(key, value);
  document.documentElement.dataset.appearance = theme.appearance ?? "dark";
}

const $ = (selector) => document.querySelector(selector);
const FILTERS = [
  ["Inbox", "in:inbox"],
  ["Unread", "is:unread in:inbox"],
  ["Starred", "is:starred"],
  ["Important", "is:important in:inbox"],
  ["Sent", "in:sent"],
  ["All mail", "in:all -in:spam -in:trash"],
];
let query = FILTERS[0][1];
let pageToken = null;
let loadTicket = 0;
let selectedId = null;
const metaCache = new Map();

function showError(error) {
  $("#error").textContent = error ? String(error.message ?? error) : "";
  $("#error").hidden = !error;
}
function guard(action) {
  return async (...args) => {
    showError(null);
    try { await action(...args); } catch (error) { showError(error); }
  };
}
function element(tag, props = {}, ...children) {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children.filter((child) => child !== null && child !== undefined && child !== false));
  return node;
}

function header(message, name) {
  return message.payload?.headers?.find((entry) => entry.name.toLowerCase() === name.toLowerCase())?.value ?? "";
}
function sender(value) {
  const match = value.match(/^\s*"?([^"<]*?)"?\s*<([^>]+)>/);
  return match ? { name: match[1].trim() || match[2], email: match[2] } : { name: value, email: value };
}
function shortDate(ms) {
  const date = new Date(Number(ms));
  const now = new Date();
  if (date.toDateString() === now.toDateString()) return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const days = (now - date) / 86400000;
  if (days < 6) return date.toLocaleDateString([], { weekday: "short" });
  return date.toLocaleDateString([], date.getFullYear() === now.getFullYear() ? { month: "short", day: "numeric" } : { year: "numeric", month: "short", day: "numeric" });
}
function decodeBody(data) {
  if (!data) return "";
  const binary = atob(data.replace(/-/g, "+").replace(/_/g, "/"));
  return new TextDecoder().decode(Uint8Array.from(binary, (char) => char.charCodeAt(0)));
}

function renderFilters() {
  const box = $("#filters");
  box.innerHTML = "";
  for (const [label, value] of FILTERS) {
    const chip = element("button", { type: "button", textContent: label });
    chip.setAttribute("aria-pressed", String(query === value));
    chip.addEventListener("click", () => { query = value; $("#search").value = ""; renderFilters(); void guard(loadList)(true); });
    box.append(chip);
  }
}

function rowFor(id) {
  const meta = metaCache.get(id);
  const row = element("li", { className: "row", tabIndex: 0 });
  row.dataset.id = id;
  if (!meta) {
    row.classList.add("loading");
    row.append(element("span", { className: "from", textContent: "…" }));
    return row;
  }
  const labels = meta.labelIds ?? [];
  if (labels.includes("UNREAD")) row.classList.add("unread");
  if (id === selectedId) row.classList.add("selected");
  const from = sender(header(meta, "From"));
  row.append(
    element("span", { className: "from", textContent: from.name, title: from.email }),
    element("span", { className: "date", textContent: shortDate(meta.internalDate) }),
    element("span", { className: "subject" },
      labels.includes("STARRED") ? element("span", { className: "star", textContent: "★ " }) : null,
      labels.includes("IMPORTANT") ? element("span", { className: "tag", textContent: "Important" }) : null,
      header(meta, "Subject") || "(no subject)"),
    element("span", { className: "snippet", innerHTML: "" }, decodeEntities(meta.snippet ?? "")),
  );
  row.addEventListener("click", () => void guard(openMessage)(id));
  row.addEventListener("keydown", (event) => { if (event.key === "Enter") void guard(openMessage)(id); });
  return row;
}
function decodeEntities(text) {
  const area = document.createElement("textarea");
  area.innerHTML = text;
  return area.value;
}
function replaceRow(id) {
  const old = document.querySelector(`#messages li[data-id="${CSS.escape(id)}"]`);
  if (old) old.replaceWith(rowFor(id));
}

async function loadList(reset) {
  const ticket = ++loadTicket;
  if (reset) {
    pageToken = null;
    $("#messages").innerHTML = "";
  }
  $("#more").hidden = true;
  $("#list-status").textContent = "Loading…";
  const page = await call("ui_messages", { query, max_results: "25", ...(pageToken ? { page_token: pageToken } : {}) });
  if (ticket !== loadTicket) return;
  const ids = (page.messages ?? []).map((message) => message.id);
  pageToken = page.nextPageToken ?? null;
  for (const id of ids) $("#messages").append(rowFor(id));
  $("#list-status").textContent = ids.length || $("#messages").children.length ? "" : "No messages here.";
  for (const id of ids) {
    if (ticket !== loadTicket) return;
    if (!metaCache.has(id)) metaCache.set(id, await call("ui_message_meta", { message_id: id }));
    replaceRow(id);
  }
  $("#more").hidden = !pageToken;
}

async function loadCounts() {
  const inbox = await call("ui_inbox_counts");
  $("#counts").textContent = `${inbox.messagesUnread ?? 0} unread`;
}

const ALLOWED = new Set(["P", "BR", "DIV", "SPAN", "B", "STRONG", "I", "EM", "U", "UL", "OL", "LI", "H1", "H2", "H3", "H4", "H5", "H6", "BLOCKQUOTE", "PRE", "CODE", "TABLE", "THEAD", "TBODY", "TR", "TD", "TH", "HR", "A"]);
function safeHtml(html) {
  const source = new DOMParser().parseFromString(html, "text/html");
  const copy = (node) => {
    if (node.nodeType === Node.TEXT_NODE) return document.createTextNode(node.textContent);
    if (node.nodeType !== Node.ELEMENT_NODE) return null;
    if (["SCRIPT", "STYLE", "HEAD", "TITLE", "META", "LINK", "IFRAME", "OBJECT", "SVG", "IMG", "FORM", "INPUT", "BUTTON", "TEMPLATE"].includes(node.tagName)) {
      return node.tagName === "IMG" && node.getAttribute("alt") ? document.createTextNode(`[${node.getAttribute("alt")}]`) : null;
    }
    const target = ALLOWED.has(node.tagName) ? document.createElement(node.tagName === "A" ? "span" : node.tagName) : document.createDocumentFragment();
    if (node.tagName === "A") target.className = "link";
    for (const child of node.childNodes) {
      const copied = copy(child);
      if (copied) target.append(copied);
    }
    if (node.tagName === "A" && node.getAttribute("href")?.startsWith("http") && node.textContent.trim() !== node.getAttribute("href")) {
      target.append(document.createTextNode(` (${node.getAttribute("href")})`));
    }
    return target;
  };
  const fragment = document.createDocumentFragment();
  for (const child of source.body.childNodes) {
    const copied = copy(child);
    if (copied) fragment.append(copied);
  }
  return fragment;
}

function bodyOf(payload) {
  const parts = [];
  const walk = (part) => {
    if (!part) return;
    parts.push(part);
    for (const child of part.parts ?? []) walk(child);
  };
  walk(payload);
  const html = parts.find((part) => part.mimeType === "text/html" && part.body?.data);
  const text = parts.find((part) => part.mimeType === "text/plain" && part.body?.data);
  const attachments = parts.filter((part) => part.filename && part.body?.attachmentId).map((part) => ({ name: part.filename, size: part.body.size ?? 0 }));
  return { html: html ? decodeBody(html.body.data) : null, text: text ? decodeBody(text.body.data) : null, attachments };
}

function size(bytes) {
  return bytes > 1048576 ? `${(bytes / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

let snackTimer = null;
function snack(text, undo) {
  const bar = $("#snackbar");
  bar.replaceChildren(element("span", { textContent: text }));
  if (undo) {
    const button = element("button", { type: "button", textContent: "Undo" });
    button.addEventListener("click", guard(async () => { bar.hidden = true; await undo(); }));
    bar.append(button);
  }
  bar.hidden = false;
  clearTimeout(snackTimer);
  snackTimer = setTimeout(() => { bar.hidden = true; }, 8000);
}

async function changeLabels(id, change) {
  const updated = await call("ui_modify", { message_id: id, change });
  const meta = metaCache.get(id);
  if (meta) meta.labelIds = updated.labelIds ?? meta.labelIds;
  replaceRow(id);
  return updated;
}

function removeRow(id) {
  document.querySelector(`#messages li[data-id="${CSS.escape(id)}"]`)?.remove();
  if (selectedId === id) {
    selectedId = null;
    document.body.classList.remove("reading");
    $("#reader").replaceChildren(element("p", { className: "muted empty", textContent: "Select a message to read it here." }));
  }
}

function utf8Base64Url(text) {
  const bytes = new TextEncoder().encode(text);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function encodedWord(text) {
  return /^[\x20-\x7e]*$/.test(text) ? text : `=?UTF-8?B?${btoa(String.fromCharCode(...new TextEncoder().encode(text)))}?=`;
}

function utf8Base64Lines(text) {
  const bytes = new TextEncoder().encode(text);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/.{1,76}/g, (line) => `${line}\r\n`);
}

function quoted(message) {
  const original = bodyOf(message.payload);
  const text = original.text ?? (original.html ? new DOMParser().parseFromString(original.html, "text/html").body.textContent : "");
  if (!text.trim()) return "";
  const when = new Date(Number(message.internalDate)).toLocaleString();
  return `\n\nOn ${when}, ${header(message, "From")} wrote:\n${text.trim().slice(0, 4000).split(/\r?\n/).map((line) => `> ${line}`).join("\n")}`;
}

function replyMime(message, to, subject, body) {
  const messageId = header(message, "Message-ID") || header(message, "Message-Id");
  const references = [header(message, "References"), messageId].filter(Boolean).join(" ");
  const lines = [
    `To: ${to}`,
    `Subject: ${encodedWord(subject)}`,
    messageId ? `In-Reply-To: ${messageId}` : null,
    references ? `References: ${references}` : null,
    "MIME-Version: 1.0",
    "Content-Type: text/plain; charset=UTF-8",
    "Content-Transfer-Encoding: base64",
    "",
    utf8Base64Lines(`${body}${quoted(message)}`.replace(/\r?\n/g, "\r\n")),
  ].filter((line) => line !== null);
  return lines.join("\r\n");
}

function replyBox(message) {
  const replyTo = header(message, "Reply-To") || header(message, "From");
  const subject = header(message, "Subject") || "";
  const recipient = sender(replyTo);
  const toInput = element("input", { value: replyTo, "aria-label": "To" });
  const subjectInput = element("input", { value: /^re:/i.test(subject) ? subject : `Re: ${subject}`, "aria-label": "Subject" });
  const body = element("textarea", { rows: 8, placeholder: "Write your reply…", "aria-label": "Reply" });
  const send = element("button", { type: "button", className: "primary", textContent: `Send to ${recipient.name}` });
  const cancel = element("button", { type: "button", textContent: "Discard" });
  const box = element("div", { className: "compose" },
    element("label", {}, "To", toInput),
    element("label", {}, "Subject", subjectInput),
    body,
    element("div", { className: "actions" }, send, cancel));
  toInput.addEventListener("input", () => { send.textContent = `Send to ${sender(toInput.value).name || "…"}`; });
  cancel.addEventListener("click", () => box.remove());
  send.addEventListener("click", guard(async () => {
    if (!toInput.value.trim()) throw new Error("Add a recipient.");
    if (!body.value.trim()) throw new Error("Write something before sending.");
    send.disabled = true;
    try {
      await call("reply_in_thread", { thread_id: message.threadId, raw_base64: utf8Base64Url(replyMime(message, toInput.value.trim(), subjectInput.value.trim(), body.value)) });
      box.remove();
      snack(`Reply sent to ${sender(toInput.value).name}.`);
    } finally {
      send.disabled = false;
    }
  }));
  requestAnimationFrame(() => body.focus());
  return box;
}

async function openMessage(id) {
  selectedId = id;
  for (const row of document.querySelectorAll("#messages li")) row.classList.toggle("selected", row.dataset.id === id);
  document.body.classList.add("reading");
  const reader = $("#reader");
  reader.innerHTML = "";
  reader.append(element("p", { className: "muted", textContent: "Opening…" }));
  const message = await call("ui_message", { message_id: id });
  if (selectedId !== id) return;
  if ((message.labelIds ?? []).includes("UNREAD")) {
    message.labelIds = (await changeLabels(id, { removeLabelIds: ["UNREAD"] })).labelIds ?? message.labelIds;
    void loadCounts().catch(() => {});
  }
  const subject = header(message, "Subject") || "(no subject)";
  const from = header(message, "From");
  const body = bodyOf(message.payload);
  const content = element("div", { className: "body" });
  if (body.html) content.append(safeHtml(body.html));
  else content.append(element("pre", { textContent: body.text ?? message.snippet ?? "" }));
  const askAbout = element("button", { type: "button", className: "primary", textContent: "Ask about this email" });
  askAbout.addEventListener("click", guard(() => zorai("ask", { text: `About the email "${subject}" from ${from} (message id ${id}): ` })));
  const reply = element("button", { type: "button", textContent: "Draft a reply with the agent" });
  reply.addEventListener("click", guard(() => zorai("ask", { text: `Draft a reply to the email "${subject}" from ${from} (message id ${id}). Do not send it. The reply should ` })));
  const labels = () => metaCache.get(id)?.labelIds ?? message.labelIds ?? [];
  const toolbar = element("div", { className: "toolbar" });
  const renderToolbar = () => {
    const inInbox = labels().includes("INBOX");
    const starred = labels().includes("STARRED");
    const unread = labels().includes("UNREAD");
    const button = (text, title, action) => {
      const node = element("button", { type: "button", textContent: text, title });
      node.addEventListener("click", guard(action));
      return node;
    };
    toolbar.replaceChildren(
      button("↩ Reply", "Reply", () => { if (!reader.querySelector(".compose")) reader.querySelector(".actions").after(replyBox(message)); }),
      inInbox
        ? button("Archive", "Remove from the inbox", async () => {
          await changeLabels(id, { removeLabelIds: ["INBOX"] });
          if (query.includes("in:inbox")) removeRow(id); else renderToolbar();
          snack("Archived.", async () => { await changeLabels(id, { addLabelIds: ["INBOX"] }); await loadList(true); });
        })
        : button("Move to inbox", "Put back in the inbox", async () => { await changeLabels(id, { addLabelIds: ["INBOX"] }); renderToolbar(); snack("Moved to the inbox."); }),
      button("🗑 Delete", "Move to Trash", async () => {
        await call("trash_message", { message_id: id });
        removeRow(id);
        snack("Moved to Trash.", async () => { await call("untrash_message", { message_id: id }); metaCache.delete(id); await loadList(true); });
      }),
      button(unread ? "Mark read" : "Mark unread", unread ? "Mark as read" : "Mark as unread", async () => {
        await changeLabels(id, unread ? { removeLabelIds: ["UNREAD"] } : { addLabelIds: ["UNREAD"] });
        renderToolbar();
        void loadCounts().catch(() => {});
      }),
      button(starred ? "★ Unstar" : "☆ Star", starred ? "Remove star" : "Star", async () => {
        await changeLabels(id, starred ? { removeLabelIds: ["STARRED"] } : { addLabelIds: ["STARRED"] });
        renderToolbar();
      }),
    );
  };
  renderToolbar();
  const back = element("button", { type: "button", className: "back", textContent: "← Messages" });
  back.addEventListener("click", () => document.body.classList.remove("reading"));
  reader.innerHTML = "";
  reader.append(
    back,
    toolbar,
    element("h1", { textContent: subject }),
    element("dl", {},
      element("dt", { textContent: "From" }), element("dd", { textContent: from }),
      element("dt", { textContent: "To" }), element("dd", { textContent: header(message, "To") }),
      element("dt", { textContent: "Date" }), element("dd", { textContent: new Date(Number(message.internalDate)).toLocaleString() })),
    element("div", { className: "actions" }, askAbout, reply),
    body.attachments.length ? element("ul", { className: "attachments" }, ...body.attachments.map((file) => element("li", { textContent: `📎 ${file.name} · ${size(file.size)}` }))) : null,
    content,
  );
}

$("#search-form").addEventListener("submit", (event) => {
  event.preventDefault();
  const value = $("#search").value.trim();
  query = value || FILTERS[0][1];
  renderFilters();
  void guard(loadList)(true);
});
$("#refresh").addEventListener("click", guard(async () => { metaCache.clear(); await Promise.all([loadList(true), loadCounts()]); }));
$("#more").addEventListener("click", guard(() => loadList(false)));
$("#ask-inbox").addEventListener("click", guard(() => zorai("ask", { text: "Look at my inbox and " })));

renderFilters();
guard(async () => { await Promise.all([loadList(true), loadCounts().catch(() => {})]); })();
