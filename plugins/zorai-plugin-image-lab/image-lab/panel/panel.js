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

const $ = (selector, root = document) => root.querySelector(selector);
const media = (file) => `/.data/${file.split("/").map(encodeURIComponent).join("/")}`;
const percent = (value) => `${Math.round((value ?? 0) * 100)}%`;
const toast = (text) => void zorai("toast", { text }).catch(() => {});
const clone = (value) => JSON.parse(JSON.stringify(value));

function showError(error) {
  const box = $("#error");
  box.textContent = error ? String(error.message ?? error) : "";
  box.hidden = !error;
}

function guard(action) {
  return async (...args) => {
    showError(null);
    try { await action(...args); } catch (error) { showError(error); }
  };
}

const mediaCache = {};
function mediaChoices(kind) {
  mediaCache[kind] ??= zorai("mediaOptions", { kind }).catch(() => null);
  return mediaCache[kind];
}

function routePicker(kind, value, onChange) {
  const wrap = document.createElement("span");
  wrap.className = "route";
  const provider = document.createElement("select");
  provider.setAttribute("aria-label", "Cloud provider");
  const model = document.createElement("input");
  model.setAttribute("aria-label", "Cloud model");
  model.setAttribute("autocomplete", "off");
  const list = document.createElement("ul");
  list.className = "suggest";
  list.setAttribute("role", "listbox");
  list.hidden = true;
  const combo = document.createElement("span");
  combo.className = "combo";
  combo.append(model, list);
  wrap.append(provider, combo);
  let models = [];
  let active = -1;
  const matches = () => {
    const query = model.value.trim().toLowerCase();
    return models.filter((item) => !query || `${item.id} ${item.name}`.toLowerCase().includes(query)).slice(0, 60);
  };
  const pick = (id) => {
    model.value = id;
    list.hidden = true;
    emit();
  };
  const render = () => {
    const found = matches();
    active = Math.min(active, found.length - 1);
    list.replaceChildren(...found.map((item, index) => {
      const option = document.createElement("li");
      option.setAttribute("role", "option");
      option.setAttribute("aria-selected", String(index === active));
      option.append(Object.assign(document.createElement("b"), { textContent: item.id }), ...(item.name && item.name !== item.id ? [Object.assign(document.createElement("span"), { textContent: item.name })] : []));
      option.addEventListener("pointerdown", (event) => { event.preventDefault(); pick(item.id); });
      return option;
    }));
    list.hidden = !found.length || document.activeElement !== model;
    list.classList.toggle("up", model.getBoundingClientRect().bottom + 230 > innerHeight && model.getBoundingClientRect().top > 230);
    list.children[active]?.scrollIntoView({ block: "nearest" });
    return found;
  };
  model.addEventListener("focus", () => { active = -1; render(); });
  model.addEventListener("input", () => { active = -1; render(); });
  model.addEventListener("blur", () => { list.hidden = true; });
  model.addEventListener("keydown", (event) => {
    const found = matches();
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      if (!found.length) return;
      event.preventDefault();
      active = (active + (event.key === "ArrowDown" ? 1 : -1) + found.length) % found.length;
      render();
    } else if (event.key === "Enter" && !list.hidden && active >= 0) {
      event.preventDefault();
      pick(found[active].id);
    } else if (event.key === "Escape") {
      list.hidden = true;
    }
  });
  const emit = () => onChange(provider.value ? { provider: provider.value, ...(model.value.trim() ? { model: model.value.trim() } : {}) } : null);
  provider.append(new Option("zorai default", ""));
  model.disabled = true;
  void mediaChoices(kind).then((options) => {
    const current = options?.current ?? {};
    const providers = options?.providers ?? [];
    provider.replaceChildren(
      new Option(current.provider ? `zorai default · ${current.provider}${current.model ? ` · ${current.model}` : ""}` : "zorai default (not set)", ""),
      ...providers.map((entry) => new Option(`${entry.label}${entry.ready ? "" : " · no key"}`, entry.id)),
    );
    if (value?.provider && !providers.some((entry) => entry.id === value.provider)) provider.append(new Option(value.provider, value.provider));
    provider.value = value?.provider ?? "";
    model.value = value?.model ?? "";
    const fill = () => {
      const entry = providers.find((item) => item.id === provider.value);
      models = entry?.models ?? [];
      model.disabled = !provider.value;
      model.placeholder = provider.value ? `model (default for ${entry?.label ?? provider.value})` : current.model || "model";
    };
    fill();
    provider.onchange = () => { model.value = ""; fill(); emit(); };
  });
  model.addEventListener("change", emit);
  return wrap;
}

function element(tag, props = {}, ...children) {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children.filter((child) => child !== null && child !== undefined));
  return node;
}

function seg(container, options, get, set) {
  container.replaceChildren(...Object.entries(options).map(([value, label]) => {
    const button = element("button", { type: "button", textContent: label });
    button.setAttribute("aria-pressed", String(get() === value));
    button.addEventListener("click", () => {
      set(value);
      for (const other of container.children) other.setAttribute("aria-pressed", String(other === button));
    });
    return button;
  }));
}

function input(value, onInput, props = {}) {
  const node = element(props.multiline ? "textarea" : "input", { value: value ?? "", ...props.attrs });
  if (props.multiline) {
    node.rows = 2;
    const fit = () => { if (!node.offsetParent) return; node.style.height = "auto"; node.style.height = `${node.scrollHeight + 2}px`; };
    requestAnimationFrame(fit);
    node.addEventListener("input", fit);
  }
  node.addEventListener("input", () => onInput(node.value));
  return node;
}

function select(options, value, onChange, label) {
  const node = element("select", { "aria-label": label });
  for (const [key, text] of Object.entries(options)) node.append(element("option", { value: key, textContent: text, selected: key === value }));
  node.addEventListener("change", () => onChange(node.value));
  return node;
}
function section(kind) {
  return $(`#${kind}`);
}

function showProblem(kind, problem) {
  const box = $(".problem", section(kind));
  box.textContent = problem ? `⚠ ${problem}` : "";
  box.hidden = !problem;
}

async function setMode(kind, mode) {
  const root = section(kind);
  if (mode === modes[kind]) return;
  if (mode === "yaml") {
    $(".yaml", root).value = (await call("dump", { document: docs[kind] })).text;
  } else {
    const parsed = await call("parse", { kind, text: $(".yaml", root).value });
    if (!parsed.document) throw new Error(`The YAML could not be read: ${parsed.problem}`);
    docs[kind] = parsed.document;
    showProblem(kind, parsed.problem);
    renderers[kind]();
  }
  modes[kind] = mode;
  $(".form", root).hidden = mode !== "form";
  $(".yaml", root).hidden = mode !== "yaml";
  for (const button of root.querySelectorAll("[data-mode]")) button.setAttribute("aria-pressed", String(button.dataset.mode === mode));
}

function requireProject() {
  if (!project) throw new Error("Create a project first: type a name and press New.");
}

async function save(kind, quiet = false) {
  requireProject();
  const root = section(kind);
  const result = modes[kind] === "yaml" ? await call("save", { project, kind, text: $(".yaml", root).value }) : await call("save", { project, kind, document: docs[kind] });
  if (result.document) docs[kind] = result.document;
  showProblem(kind, result.problem);
  if (!quiet) toast(`Saved ${kind}${result.saved.length > 1 ? ` and copied to ${info?.location}` : ""}`);
  if (result.problem) throw new Error(result.problem);
  return result;
}

async function follow(job, kind) {
  const box = $(".progress", section(kind));
  currentJob = job.id;
  box.hidden = false;
  $(".cancel", box).onclick = guard(() => call("cancel", { job: job.id }));
  let status = job;
  while (status && !["done", "failed", "cancelled"].includes(status.state)) {
    $(".meter span", box).style.width = percent(status.progress);
    $("p", box).textContent = status.message;
    await new Promise((resolve) => setTimeout(resolve, 900));
    status = await call("status", { job: job.id });
  }
  box.hidden = true;
  currentJob = null;
  if (status.state === "failed") throw new Error(`${status.message}${status.error && !status.error.endsWith(status.message) ? `\n${status.error}` : ""}`);
  if (status.state === "done") toast(status.message);
  return status;
}
function renderLocation() {
  $("#location-text").textContent = !project ? "" : info?.location ? `📁 Copies go to ${info.location}` : "";
  $("#choose-folder").hidden = !project;
  $("#choose-folder").textContent = info?.location ? "Change folder…" : "Choose folder…";
  $("#clear-folder").hidden = !info?.location;
}

async function refreshProjects(select = project) {
  const { projects } = await call("projects");
  const box = $("#project");
  box.replaceChildren(element("option", { value: "", textContent: projects.length ? "Choose a project" : "No projects yet" }), ...projects.map((entry) => element("option", { value: entry.name, textContent: `${entry.name} · ${entry.documents.join(", ")}`, selected: entry.name === select })));
  return projects;
}

function typedName() {
  const name = $("#new-name").value.trim().toLowerCase().replace(/[^a-z0-9_-]+/g, "-").replace(/^-+|-+$/g, "");
  if (!name) throw new Error("Type a project name first (letters, digits, - and _).");
  return name;
}

async function loadJobs() {
  const { jobs } = await call("jobs");
  $("#jobs-list").replaceChildren(...jobs.map((job) => {
    const row = element("tr");
    row.append(
      element("td", { textContent: new Date(job.created_at * 1000).toLocaleString() }),
      element("td", { textContent: `${job.kind}${job.params?.project ? ` · ${job.params.project}` : job.params?.path ? ` · ${job.params.path.split("/").pop()}` : ""}` }),
      element("td", {}, element("span", { className: `badge ${job.state === "done" ? "ok" : job.state === "failed" ? "warn" : ""}`, textContent: job.state })),
      element("td", { textContent: job.message ?? "" }),
      element("td", {}, ["running", "queued"].includes(job.state) ? element("button", { type: "button", textContent: "Cancel", onclick: guard(async () => { await call("cancel", { job: job.id }); await loadJobs(); }) }) : null),
    );
    return row;
  }));
}

async function loadChecks() {
  const report = await call("check");
  const chip = $("#ready");
  chip.textContent = report.ready ? "Ready" : "Needs setup";
  chip.className = `chip ${report.ready ? "ok" : "warn"}`;
  $("#checks").replaceChildren(...report.items.map((item) => {
    const row = element("tr");
    row.append(element("td", { textContent: item.ok ? "✅" : item.required === false ? "➖" : "❌" }), element("td", { textContent: item.name }), element("td", { textContent: item.detail }));
    return row;
  }));
}

const KINDS = ["brief", "uikit", "figure"];
const ASPECTS = ["1:1", "3:2", "16:9", "2:3", "9:16"];
const BLANK = {
  brief: { title: "", brief: "", engine: "cloud", aspect: "1:1", variants: 4, brand: { palette: [], style: "", avoid: [] }, rubric: [], critic: true, max_rounds: 3, pass_score: 8 },
  uikit: { output: "", engine: "cloud", brand: { palette: [] }, assets: [{ name: "empty-state", kind: "illustration", prompt: "", sizes: [512, 256] }] },
  figure: { title: "", data: "", kind: "bar", x: "", y: "", verify: true },
};
const docs = { brief: clone(BLANK.brief), uikit: clone(BLANK.uikit), figure: clone(BLANK.figure) };
const modes = { brief: "form", uikit: "form", figure: "form" };
let project = "";
let info = null;
let currentJob = null;
let currentSheet = null;
let drawing = null;

const list = (text) => text.split(",").map((part) => part.trim()).filter(Boolean);
const hexOk = (value) => /^#[0-9a-fA-F]{6}$/.test(value);

function cloudRoute(box, doc, kind, mediaKind, shown) {
  box.replaceChildren(...(shown ? [routePicker(mediaKind, doc.cloud?.[kind], (route) => {
    doc.cloud = { ...(doc.cloud ?? {}) };
    if (route) doc.cloud[kind] = route;
    else delete doc.cloud[kind];
    if (!Object.keys(doc.cloud).length) delete doc.cloud;
  })] : []));
}

function renderBrief() {
  const doc = docs.brief;
  doc.brand ??= {};
  doc.brand.palette ??= [];
  const brand = doc.brand;
  $("#br-title").value = doc.title ?? "";
  $("#br-title").oninput = (event) => { doc.title = event.target.value; };
  $("#br-text").value = doc.brief ?? "";
  $("#br-text").oninput = (event) => { doc.brief = event.target.value; };
  $("#br-style").value = brand.style ?? "";
  $("#br-style").oninput = (event) => { brand.style = event.target.value || undefined; };
  $("#br-avoid").value = (brand.avoid ?? []).join(", ");
  $("#br-avoid").oninput = (event) => { brand.avoid = list(event.target.value); };
  $("#br-css").value = brand.from_css ?? "";
  $("#br-css").oninput = (event) => { if (event.target.value) brand.from_css = event.target.value; else delete brand.from_css; };
  $("#br-palette").replaceChildren(...brand.palette.map((color, index) => {
    const picker = element("input", { type: "color", value: hexOk(color) ? color : "#000000", title: color });
    const text = element("input", { value: color, className: "hex", "aria-label": "Colour" });
    const set = (value) => { brand.palette[index] = value.toUpperCase(); picker.value = hexOk(value) ? value : picker.value; text.value = value.toUpperCase(); };
    picker.addEventListener("input", () => set(picker.value));
    text.addEventListener("change", () => { if (hexOk(text.value)) set(text.value); });
    return element("div", { className: "swatch" }, picker, text, element("button", { type: "button", className: "icon", textContent: "✕", title: "Remove colour", onclick: () => { brand.palette.splice(index, 1); renderBrief(); } }));
  }), element("button", { type: "button", className: "swatch add", textContent: "+ Colour", onclick: () => { brand.palette.push("#D9A548"); renderBrief(); } }));
  seg($("#br-engine"), { cloud: "Cloud", local: "Local" }, () => doc.engine ?? "cloud", (value) => { doc.engine = value; cloudRoute($("#br-image-route"), doc, "image", "image", value === "cloud"); void showEstimate(); });
  cloudRoute($("#br-image-route"), doc, "image", "image", (doc.engine ?? "cloud") === "cloud");
  seg($("#br-aspect"), Object.fromEntries(ASPECTS.map((aspect) => [aspect, aspect])), () => doc.aspect ?? "1:1", (value) => { doc.aspect = value; });
  doc.rubric ??= [];
  $("#br-rubric").replaceChildren(...doc.rubric.map((rule, index) => element("div", { className: "row" },
    input(rule, (value) => { doc.rubric[index] = value; }, { attrs: { placeholder: "Something the critic must see", "aria-label": `Rule ${index + 1}` } }),
    element("button", { type: "button", className: "icon", textContent: "✕", title: "Remove rule", onclick: () => { doc.rubric.splice(index, 1); renderBrief(); } }),
  )));
  $("#br-critic").checked = doc.critic !== false;
  $("#br-critic").onchange = (event) => { doc.critic = event.target.checked; cloudRoute($("#br-vision-route"), doc, "vision", "vision", doc.critic); };
  cloudRoute($("#br-vision-route"), doc, "vision", "vision", doc.critic !== false);
  for (const [id, key, fallback] of [["#br-variants", "variants", 4], ["#br-rounds", "max_rounds", 3], ["#br-pass", "pass_score", 8]]) {
    $(id).value = doc[key] ?? fallback;
    $(id).oninput = (event) => { doc[key] = Number(event.target.value) || fallback; };
  }
}

async function showEstimate() {
  if (!project || !info?.documents?.brief) { $("#br-estimate").textContent = ""; return; }
  try {
    const estimate = await call("estimate", { project });
    $("#br-estimate").textContent = estimate.images_min === estimate.images_max
      ? `${estimate.images_min} images`
      : `${estimate.images_min}–${estimate.images_max} images, up to ${estimate.critic_calls_max} critic checks`;
  } catch {
    $("#br-estimate").textContent = "";
  }
}

function badge(text, tone, title) {
  return element("span", { className: `badge ${tone ?? ""}`, textContent: text, title: title ?? "" });
}

function variantCard(variant) {
  let shown = variant.best;
  const card = element("article", { className: "variant" });
  const frame = element("div", { className: "frame" });
  const image = element("img", { alt: `Variant ${variant.index}` });
  const boxes = element("div", { className: "boxes" });
  const draft = element("div", { className: "draft", hidden: true });
  frame.append(image, boxes, draft);
  const meta = element("div", { className: "meta" });
  const rounds = element("div", { className: "rounds" });
  const fixRow = element("div", { className: "row fix", hidden: true });
  const fixText = element("input", { placeholder: "What should change in the box?", "aria-label": "Fix instruction" });
  fixRow.append(fixText, element("button", { type: "button", className: "primary", textContent: "Apply fix", onclick: guard(() => applyFix(variant, fixText.value)) }), element("button", { type: "button", textContent: "Cancel", onclick: () => stopDrawing() }));

  const show = (entry) => {
    shown = entry;
    image.src = media(entry.file);
    const card = entry.card;
    const critic = card.critic;
    boxes.replaceChildren(...(critic?.issues ?? []).filter((issue) => issue.box).map((issue) => {
      const [x0, y0, x1, y1] = issue.box;
      return element("div", { className: "issue", title: issue.label, style: `left:${x0 * 100}%;top:${y0 * 100}%;width:${(x1 - x0) * 100}%;height:${(y1 - y0) * 100}%` }, element("span", { textContent: issue.label }));
    }));
    const palette = card.palette;
    const passMark = Number(docs.brief.pass_score ?? 8);
    meta.replaceChildren(...[
      element("div", { className: "row" },
        element("strong", { textContent: `${variant.index}` }),
        badge(`${card.total.toFixed(1)} / 10`, variant.passed === null ? "" : card.total >= passMark ? "ok" : "warn", `Overall score (passes at ${passMark})`),
        critic ? null : badge(card.critic_error ? "critic failed" : "no critic", "warn", card.critic_error ?? "Only colour and sharpness were checked"),
        palette ? badge(`ΔE ${palette.mean_delta_e}`, palette.pass ? "ok" : "warn", "Average colour distance to the brand palette (under 12 fits)") : null,
        card.sharpness < 40 ? badge("soft", "warn", `Sharpness ${card.sharpness}`) : null,
        entry.fix ? badge(entry.fix.method === "inpaint" ? "region repainted" : "regenerated with fix", "") : null,
      ),
      palette ? element("div", { className: "chips" }, ...palette.colors.slice(0, 6).map((color) => element("span", { className: "chip-color", title: `${color.hex} · ${Math.round(color.share * 100)}% · nearest ${color.nearest} (ΔE ${color.delta_e})`, style: `background:${color.hex}` }))) : null,
      critic ? element("ul", { className: "checks" }, ...critic.checks.map((check) => element("li", { className: check.pass ? "pass" : "fail", textContent: `${check.pass ? "✓" : "✗"} ${check.rule}${check.note ? ` — ${check.note}` : ""}` })), ...critic.issues.map((issue) => element("li", { className: "fail", textContent: `⚠ ${issue.label}` }))) : null,
      critic?.suggestion ? element("p", { className: "muted small", textContent: `Critic: ${critic.suggestion}` }) : null,
    ].filter(Boolean));
    for (const thumb of rounds.children) thumb.classList.toggle("is-active", thumb.dataset.file === entry.file);
  };

  rounds.replaceChildren(...variant.rounds.map((entry) => {
    const thumb = element("button", { type: "button", className: "round", title: `Round ${entry.round}: ${entry.card.total.toFixed(1)}` }, element("img", { src: media(entry.file), alt: "" }), element("span", { textContent: `${entry.round}${entry === variant.best || entry.file === variant.best.file ? " ★" : ""}` }));
    thumb.dataset.file = entry.file;
    thumb.addEventListener("click", () => show(entry));
    return thumb;
  }));

  frame.addEventListener("pointerdown", (event) => {
    if (drawing?.variant !== variant.index) return;
    const rect = frame.getBoundingClientRect();
    const start = { x: (event.clientX - rect.left) / rect.width, y: (event.clientY - rect.top) / rect.height };
    frame.setPointerCapture(event.pointerId);
    const move = (moveEvent) => {
      const x = Math.min(1, Math.max(0, (moveEvent.clientX - rect.left) / rect.width));
      const y = Math.min(1, Math.max(0, (moveEvent.clientY - rect.top) / rect.height));
      drawing.box = [Math.min(start.x, x), Math.min(start.y, y), Math.max(start.x, x), Math.max(start.y, y)].map((value) => Math.round(value * 1000) / 1000);
      const [x0, y0, x1, y1] = drawing.box;
      Object.assign(draft.style, { left: `${x0 * 100}%`, top: `${y0 * 100}%`, width: `${(x1 - x0) * 100}%`, height: `${(y1 - y0) * 100}%` });
      draft.hidden = false;
    };
    const up = () => { frame.removeEventListener("pointermove", move); frame.removeEventListener("pointerup", up); fixText.focus(); };
    frame.addEventListener("pointermove", move);
    frame.addEventListener("pointerup", up);
  });

  const actions = element("div", { className: "row" },
    element("button", { type: "button", textContent: "Fix a region", title: "Drag a box on the image, then say what should change", onclick: () => {
      stopDrawing();
      const issue = shown.card.critic?.issues?.find((item) => item.box);
      drawing = { variant: variant.index, box: issue?.box ?? null, card, fixRow, draft, frame };
      frame.classList.add("is-drawing");
      fixRow.hidden = false;
      if (issue) {
        const [x0, y0, x1, y1] = issue.box;
        Object.assign(draft.style, { left: `${x0 * 100}%`, top: `${y0 * 100}%`, width: `${(x1 - x0) * 100}%`, height: `${(y1 - y0) * 100}%` });
        draft.hidden = false;
        fixText.value = issue.label ? `Fix: ${issue.label}` : "";
      }
    } }),
    element("button", { type: "button", textContent: "Ask", title: "Discuss this variant with zorai", onclick: guard(() => zorai("ask", { text: `About Image Lab project "${project}", variant ${variant.index} (${shown.file}, score ${shown.card.total}): ${shown.card.critic?.suggestion ? `the critic said "${shown.card.critic.suggestion}". ` : ""}` })) }),
    element("button", { type: "button", textContent: "Add to message", onclick: guard(() => zorai("insertText", { text: `Image Lab variant ${variant.index} of "${project}": ~/.zorai/plugin-data/image-lab/${shown.file}` })) }),
  );
  card.append(frame, rounds, meta, actions, fixRow);
  show(variant.best);
  return card;
}

function stopDrawing() {
  if (!drawing) return;
  drawing.frame.classList.remove("is-drawing");
  drawing.fixRow.hidden = true;
  drawing.draft.hidden = true;
  drawing = null;
}

async function applyFix(variant, instruction) {
  if (!drawing?.box) throw new Error("Drag a box over the part of the image to change.");
  if (!instruction.trim()) throw new Error("Say what should change in the box.");
  const box = drawing.box;
  stopDrawing();
  await follow(await call("fix", { project, sheet: currentSheet?.id, variant: variant.index, box, prompt: instruction }), "brief");
  await showSheet(currentSheet?.id);
}

async function showSheet(sheetId) {
  const box = $("#sheet");
  let sheet = null;
  try {
    sheet = project ? await call("load-sheet", sheetId ? { project, sheet: sheetId } : { project }) : null;
  } catch {
    sheet = null;
  }
  currentSheet = sheet;
  box.hidden = !sheet;
  if (!sheet) return;
  $("#sheet-title").textContent = `Contact sheet · ${sheet.title}`;
  $("#sheet-note").textContent = sheet.critic_note ?? `${sheet.images} images · ${sheet.variants.filter((variant) => variant.passed).length} of ${sheet.variants.length} passed the critic`;
  $("#variants").replaceChildren(...sheet.variants.map(variantCard));
  const { sheets } = await call("sheets", { project });
  $("#sheet-pick").replaceChildren(...sheets.map((entry) => element("option", { value: entry.id, textContent: `${entry.id} · ${entry.variants} variants`, selected: entry.id === sheet.id })));
}

function renderKit() {
  const doc = docs.uikit;
  $("#kit-output").value = doc.output ?? "";
  $("#kit-output").oninput = (event) => { doc.output = event.target.value; };
  $("#kit-theme").value = doc.theme ?? "";
  $("#kit-theme").oninput = (event) => { if (event.target.value) doc.theme = event.target.value; else delete doc.theme; };
  seg($("#kit-engine"), { cloud: "Cloud", local: "Local" }, () => doc.engine ?? "cloud", (value) => { doc.engine = value; cloudRoute($("#kit-image-route"), doc, "image", "image", value === "cloud"); });
  cloudRoute($("#kit-image-route"), doc, "image", "image", (doc.engine ?? "cloud") === "cloud");
  doc.assets ??= [];
  $("#kit-assets").replaceChildren(...doc.assets.map((asset, index) => element("div", { className: "row" },
    input(asset.name, (value) => { asset.name = value.trim().toLowerCase(); }, { attrs: { className: "name", placeholder: "name", "aria-label": "Asset name" } }),
    select({ illustration: "Illustration", icon: "Icon", photo: "Photo" }, asset.kind ?? "illustration", (value) => { asset.kind = value; }, "Kind"),
    input(asset.prompt, (value) => { asset.prompt = value; }, { attrs: { placeholder: "What it shows", "aria-label": "Asset prompt" } }),
    input((asset.sizes ?? []).join(", "), (value) => { asset.sizes = list(value).map(Number).filter(Boolean); }, { attrs: { className: "sizes", placeholder: "512, 256", "aria-label": "Sizes in pixels" } }),
    element("button", { type: "button", className: "icon", textContent: "✕", title: "Remove asset", onclick: () => { doc.assets.splice(index, 1); renderKit(); } }),
  )));
}

function showKit() {
  const kit = info?.uikit;
  $("#kit-result").hidden = !kit;
  if (!kit) return;
  $("#kit-result-text").textContent = `${kit.assets.length} assets and ${kit.files} files in ${kit.output} (index.ts and manifest.json included)`;
  $("#kit-previews").replaceChildren(...kit.assets.map((asset) => element("figure", {},
    element("div", { className: "checker" }, element("img", { src: `${media(asset.preview)}?v=${Date.now()}`, alt: asset.name })),
    element("figcaption", {}, element("strong", { textContent: asset.name }), ` · ${asset.files.map((file) => `${file.size}px`).join(", ")}${asset.palette ? ` · ΔE ${asset.palette.mean_delta_e}` : ""}${asset.background ? ` · ${Math.round(asset.background.background_removed * 100)}% background removed` : ""}`),
  )));
}

function renderFigure() {
  const doc = docs.figure;
  const bind = (id, key, transform = (value) => value || undefined) => {
    const node = $(id);
    node.value = Array.isArray(doc[key]) ? doc[key].join(", ") : doc[key] ?? "";
    node.oninput = (event) => { const value = transform(event.target.value); if (value === undefined) delete doc[key]; else doc[key] = value; };
  };
  bind("#fig-title", "title");
  bind("#fig-data", "data");
  bind("#fig-x", "x");
  bind("#fig-y", "y", (value) => { const columns = list(value); return columns.length > 1 ? columns : columns[0]; });
  bind("#fig-error", "error");
  bind("#fig-ylabel", "ylabel");
  bind("#fig-digits", "digits", (value) => Number(value) || undefined);
  seg($("#fig-kind"), { bar: "Bar", line: "Line", scatter: "Scatter" }, () => doc.kind ?? "bar", (value) => { doc.kind = value; });
  $("#fig-verify").checked = doc.verify !== false;
  $("#fig-verify").onchange = (event) => { doc.verify = event.target.checked; cloudRoute($("#fig-vision-route"), doc, "vision", "vision", doc.verify); };
  cloudRoute($("#fig-vision-route"), doc, "vision", "vision", doc.verify !== false);
}

function showFigure() {
  const figure = info?.figure;
  $("#fig-result").hidden = !figure;
  if (!figure) return;
  $("#fig-image").src = `${media(figure.png)}?v=${Date.now()}`;
  const check = figure.check;
  $("#fig-check").replaceChildren(!check ? "" : check.ok === null ? badge("labels not checked", "", check.error) : check.ok ? badge("✓ every label read back correctly", "ok") : badge(check.missing_labels?.length ? `✗ not read back: ${check.missing_labels.join(", ")}` : `✗ ${check.error ?? "title differs"}`, "warn", check.title_read ? `Read title: ${check.title_read}` : ""));
  $("#fig-files").textContent = `${figure.svg_path} (and .png) · ${figure.rows} rows`;
}

const renderers = { brief: renderBrief, uikit: renderKit, figure: renderFigure };

async function openProject(name) {
  project = name;
  $("#project").value = name;
  stopDrawing();
  info = name ? await call("load", { project: name }) : null;
  for (const kind of KINDS) {
    const stored = info?.documents?.[kind];
    docs[kind] = stored?.document ?? clone(BLANK[kind]);
    showProblem(kind, stored?.problem);
    if (modes[kind] === "yaml") $(".yaml", section(kind)).value = stored?.text ?? (await call("dump", { document: docs[kind] })).text;
    renderers[kind]();
  }
  renderLocation();
  showKit();
  showFigure();
  await Promise.all([showSheet(), showEstimate()]);
}

async function reload() {
  info = await call("load", { project });
  renderLocation();
  showKit();
  showFigure();
}

for (const tab of document.querySelectorAll("nav [data-tab]")) {
  tab.addEventListener("click", guard(async () => {
    for (const other of document.querySelectorAll("nav [data-tab]")) other.setAttribute("aria-selected", String(other === tab));
    for (const panel of document.querySelectorAll(".tab")) panel.hidden = panel.id !== tab.dataset.tab;
    $("#project-bar").hidden = ["jobs", "setup"].includes(tab.dataset.tab);
    if (tab.dataset.tab === "jobs") await loadJobs();
    if (tab.dataset.tab === "setup") await loadChecks();
  }));
}

for (const kind of KINDS) {
  const root = section(kind);
  for (const button of root.querySelectorAll("[data-mode]")) button.addEventListener("click", guard(() => setMode(kind, button.dataset.mode)));
  $(".save", root).addEventListener("click", guard(async () => {
    await save(kind);
    await reload();
    if (kind === "brief") await showEstimate();
  }));
  $(".example", root).addEventListener("click", guard(async () => {
    const example = await call("example", { kind });
    docs[kind] = example.document;
    if (modes[kind] === "yaml") $(".yaml", root).value = example.text;
    showProblem(kind, example.problem);
    renderers[kind]();
  }));
}

$("#project").addEventListener("change", guard(() => openProject($("#project").value)));
$("#new-project").addEventListener("click", guard(async () => {
  const name = typedName();
  const existing = await refreshProjects();
  if (existing.some((entry) => entry.name === name)) throw new Error(`A project called ${name} already exists. Pick it from the list.`);
  project = name;
  await call("save", { project: name, kind: "brief", document: BLANK.brief });
  await refreshProjects(name);
  await openProject(name);
  $("#new-name").value = "";
  toast(`Created ${name}`);
}));
$("#rename").addEventListener("click", guard(async () => {
  requireProject();
  const result = await call("rename", { project, to: typedName() });
  await refreshProjects(result.name);
  await openProject(result.name);
  $("#new-name").value = "";
}));
$("#choose-folder").addEventListener("click", guard(async () => {
  requireProject();
  const folder = await zorai("pickFolder", { title: `Folder for ${project}` });
  if (!folder) return;
  await call("location", { project, folder });
  await reload();
  toast(`New images from ${project} are also copied to ${folder}`);
}));
$("#clear-folder").addEventListener("click", guard(async () => {
  await call("location", { project, folder: "" });
  await reload();
}));

$("#br-add-rule").addEventListener("click", () => { docs.brief.rubric ??= []; docs.brief.rubric.push(""); renderBrief(); });
$("#br-css-read").addEventListener("click", guard(async () => {
  const path = $("#br-css").value.trim();
  if (!path) throw new Error("Type the path of a stylesheet with CSS colour variables.");
  const { colors } = await call("theme", { path });
  const brand = docs.brief.brand;
  $("#br-css-found").replaceChildren(...Object.entries(colors).map(([name, hex]) => {
    const chosen = brand.palette.includes(hex);
    return element("button", { type: "button", className: `chip-pick${chosen ? " is-on" : ""}`, title: chosen ? "In the palette" : "Add to the palette", onclick: () => { if (!brand.palette.includes(hex)) brand.palette.push(hex); renderBrief(); $("#br-css-read").click(); } }, element("span", { className: "chip-color", style: `background:${hex}` }), `${name} ${hex}`);
  }));
}));
$("#br-run").addEventListener("click", guard(async () => {
  await save("brief", true);
  await follow(await call("sheet", { project }), "brief");
  await reload();
  await showSheet();
}));
$("#br-ask").addEventListener("click", guard(async () => {
  requireProject();
  const yaml = (await call("dump", { document: docs.brief })).text;
  await zorai("ask", { text: `Sharpen the Image Lab brief of project "${project}": make the brief concrete, the rubric checkable by a vision model, and the avoid list complete. Save it with /image-lab.save --project ${project} --kind brief, then make a contact sheet.\n\n\`\`\`yaml\n${yaml}\`\`\`\n\nAlso: ` });
}));
$("#sheet-pick").addEventListener("change", guard(() => showSheet($("#sheet-pick").value)));
$("#sheet-ask").addEventListener("click", guard(async () => {
  if (!currentSheet) return;
  const summary = currentSheet.variants.map((variant) => `variant ${variant.index}: ${variant.best.card.total}${variant.best.card.critic?.suggestion ? ` (${variant.best.card.critic.suggestion})` : ""}`).join("; ");
  await zorai("ask", { text: `Review Image Lab contact sheet ${currentSheet.id} of project "${project}" (~/.zorai/plugin-data/image-lab/${currentSheet.contact_sheet}). Scores: ${summary}. Which variant should we ship and what should the next brief change? ` });
}));

$("#kit-pick").addEventListener("click", guard(async () => {
  const folder = await zorai("pickFolder", { title: "Folder for the generated assets" });
  if (folder) { docs.uikit.output = folder; renderKit(); }
}));
$("#kit-add").addEventListener("click", () => { docs.uikit.assets ??= []; docs.uikit.assets.push({ name: `asset-${docs.uikit.assets.length + 1}`, kind: "icon", prompt: "", sizes: [128, 64] }); renderKit(); });
$("#kit-run").addEventListener("click", guard(async () => {
  await save("uikit", true);
  await follow(await call("uikit", { project }), "uikit");
  await reload();
}));
$("#fig-run").addEventListener("click", guard(async () => {
  await save("figure", true);
  await follow(await call("figure", { project }), "figure");
  await reload();
}));

$("#jobs-refresh").addEventListener("click", guard(loadJobs));
$("#recheck").addEventListener("click", guard(loadChecks));
$("#install-local").addEventListener("click", guard(async () => {
  const button = $("#install-local");
  if (button.dataset.armed !== "1") {
    button.dataset.armed = "1";
    button.textContent = "Confirm: download about 12 GB";
    button.classList.add("armed");
    setTimeout(() => { button.dataset.armed = ""; button.textContent = "Install local engine…"; button.classList.remove("armed"); }, 8000);
    return;
  }
  button.dataset.armed = "";
  button.textContent = "Install local engine…";
  button.classList.remove("armed");
  await follow(await call("setup"), "setup");
  await loadChecks();
}));

async function resumeRunning() {
  const { jobs } = await call("jobs");
  const running = jobs.find((job) => ["running", "queued"].includes(job.state) && job.params?.project === project);
  const kind = { sheet: "brief", fix: "brief", uikit: "uikit", figure: "figure" }[running?.kind];
  if (!kind) return;
  await follow(running, kind);
  await reload();
  await showSheet();
}

void zorai("theme", {}).then(applyTheme).catch(() => {});
guard(async () => {
  for (const kind of KINDS) renderers[kind]();
  const projects = await refreshProjects();
  if (projects.length) await openProject(projects[0].name);
  renderLocation();
  await loadChecks();
  await resumeRunning();
})();
