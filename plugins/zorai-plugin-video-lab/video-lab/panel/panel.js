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
const seconds = (value) => `${Math.round(value ?? 0)} s`;
const toast = (text) => void zorai("toast", { text }).catch(() => {});
const FORMATS = ["16:9", "9:16", "1:1", "4:5"];
const MOTIONS = ["push-in", "pull-out", "pan-left", "pan-right", "still"];
const ACTIONS = { "": "Narrate only", click: "Click", type: "Type", press: "Press key", scroll: "Scroll", wait: "Wait", hover: "Hover", goto: "Go to" };
const KINDS = ["storyboard", "edit", "demo"];
const BLANK = {
  storyboard: { title: "", format: "16:9", narration: { engine: "local", voice: "af_heart" }, keyframes: "local", captions: "karaoke", shots: [{ id: "shot-1", beat: "Hook", seconds: 3, visual: "", narration: "", motion: "push-in" }] },
  edit: { title: "", sources: { main: "" }, tighten: { silence: 0.7, fillers: ["um", "uh", "erm"] }, formats: ["16:9"], captions: "karaoke", fit: "crop" },
  demo: { title: "", url: "", format: "16:9", narration: { engine: "local", voice: "am_michael" }, captions: "karaoke", steps: [{ say: "" }] },
};
const clone = (value) => JSON.parse(JSON.stringify(value));
const docs = { storyboard: clone(BLANK.storyboard), edit: clone(BLANK.edit), demo: clone(BLANK.demo) };
const modes = { storyboard: "form", edit: "form", demo: "form" };
let project = "";
let info = null;
let voices = [];
let estimateCache = null;
let currentJob = null;
let approveArmed = false;

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

const forFormats = Object.fromEntries(FORMATS.map((format) => [format, format]));

function voiceSelect(node, current, onChange) {
  const list = voices.length ? voices : [current || "af_heart"];
  node.replaceChildren(...list.map((voice) => element("option", { value: voice, textContent: voice, selected: voice === current })));
  node.onchange = () => onChange(node.value);
}

function cloudRoute(box, doc, kind, mediaKind, shown) {
  box.replaceChildren(...(shown ? [routePicker(mediaKind, doc.cloud?.[kind], (route) => {
    doc.cloud = { ...(doc.cloud ?? {}) };
    if (route) doc.cloud[kind] = route;
    else delete doc.cloud[kind];
    if (!Object.keys(doc.cloud).length) delete doc.cloud;
  })] : []));
}

function narrationControls(prefix, doc) {
  doc.narration ??= { engine: "local" };
  const voiceBox = $(`#${prefix}-voice`);
  const sync = () => {
    voiceBox.hidden = doc.narration.engine !== "local";
    if (doc.narration.engine === "local") voiceSelect(voiceBox, doc.narration.voice, (voice) => { doc.narration.voice = voice; });
    cloudRoute($(`#${prefix}-speak-route`), doc, "speak", "tts", doc.narration.engine === "cloud");
  };
  seg($(`#${prefix}-engine`), { local: "Local", cloud: "Cloud", none: "None" }, () => doc.narration.engine ?? "local", (engine) => {
    doc.narration.engine = engine;
    if (engine === "cloud") delete doc.narration.voice;
    if (engine === "local") doc.narration.voice ??= "af_heart";
    sync();
  });
  sync();
}

function renderPlan() {
  const doc = docs.storyboard;
  const shots = doc.shots ?? [];
  const total = shots.reduce((sum, shot) => sum + (Number(shot.seconds) || 4), 0);
  const paid = [(doc.keyframes ?? "local") === "cloud" ? `${shots.length} cloud keyframe${shots.length === 1 ? "" : "s"}` : "", doc.narration?.engine === "cloud" ? "cloud narration" : ""].filter(Boolean);
  $("#sb-plan").textContent = `${shots.length} shot${shots.length === 1 ? "" : "s"} · ${seconds(total)} or more · animatic ${paid.length ? `uses ${paid.join(" and ")} (paid)` : "runs free on this computer"}`;
}

function renderStoryboard() {
  const doc = docs.storyboard;
  $("#sb-title").value = doc.title ?? "";
  $("#sb-title").oninput = (event) => { doc.title = event.target.value; };
  $("#sb-style").value = doc.style ?? "";
  $("#sb-style").oninput = (event) => { doc.style = event.target.value || undefined; };
  $("#sb-music").value = doc.music ?? "";
  $("#sb-music").oninput = (event) => { doc.music = event.target.value || undefined; };
  seg($("#sb-format"), forFormats, () => doc.format ?? "16:9", (value) => { doc.format = value; });
  seg($("#sb-keyframes"), { local: "Title cards", cloud: "Cloud images" }, () => doc.keyframes ?? "local", (value) => { doc.keyframes = value; cloudRoute($("#sb-image-route"), doc, "image", "image", value === "cloud"); });
  cloudRoute($("#sb-image-route"), doc, "image", "image", (doc.keyframes ?? "local") === "cloud");
  cloudRoute($("#sb-video-route"), doc, "video", "video", true);
  seg($("#sb-captions"), { karaoke: "Karaoke", plain: "Plain", none: "None" }, () => doc.captions ?? "karaoke", (value) => { doc.captions = value; });
  narrationControls("sb", doc);
  const strip = $("#sb-shots");
  doc.shots ??= [];
  const keyframes = Object.fromEntries((info?.animatic?.shots ?? []).filter((shot) => shot.keyframe && !shot.keyframe.startsWith("/")).map((shot) => [shot.id, shot.keyframe]));
  strip.replaceChildren(...doc.shots.map((shot, index) => {
    const move = (delta) => () => {
      const target = index + delta;
      if (target < 0 || target >= doc.shots.length) return;
      [doc.shots[index], doc.shots[target]] = [doc.shots[target], doc.shots[index]];
      renderStoryboard();
    };
    const head = element("div", { className: "shot-head" },
      element("span", { className: "num", textContent: String(index + 1).padStart(2, "0") }),
      input(shot.beat, (value) => { shot.beat = value; }, { attrs: { placeholder: "Beat", "aria-label": `Shot ${index + 1} beat` } }),
      element("button", { type: "button", className: "icon", textContent: "◀", title: "Move earlier", onclick: move(-1) }),
      element("button", { type: "button", className: "icon", textContent: "▶", title: "Move later", onclick: move(1) }),
      element("button", { type: "button", className: "icon", textContent: "✕", title: "Remove shot", onclick: () => { doc.shots.splice(index, 1); renderStoryboard(); } }),
    );
    const thumb = keyframes[shot.id] ? element("img", { src: media(keyframes[shot.id]), alt: `Shot ${index + 1} keyframe` }) : element("div", { className: "thumb-empty", textContent: "Keyframe appears after the animatic" });
    const settings = element("div", { className: "shot-settings" },
      element("label", { title: "Shortest length; narration can make it longer" }, input(shot.seconds ?? 4, (value) => { shot.seconds = Number(value) || 4; }, { attrs: { type: "number", min: 1, max: 30, step: 0.5, className: "num", "aria-label": "Seconds" } }), "s"),
      select(Object.fromEntries(MOTIONS.map((motion) => [motion, motion.replace("-", " ")])), shot.motion ?? "push-in", (value) => { shot.motion = value; }, "Camera move"),
      select({ video: "Video", still: "Still" }, shot.render ?? "video", (value) => { shot.render = value; }, "In the final render: a generated video or this still"),
    );
    return element("article", { className: "shot" }, head, element("div", { className: "thumb" }, thumb), settings,
      input(shot.visual, (value) => { shot.visual = value; }, { multiline: true, attrs: { placeholder: "What we see", "aria-label": `Shot ${index + 1} visual` } }),
      input(shot.narration, (value) => { shot.narration = value; }, { multiline: true, attrs: { placeholder: "What the narrator says", "aria-label": `Shot ${index + 1} narration`, className: "narration" } }),
    );
  }), element("button", { type: "button", className: "shot add", textContent: "+ Shot", onclick: () => {
    doc.shots.push({ id: `shot-${Date.now().toString(36)}`, beat: "", seconds: 3, visual: "", narration: "", motion: "push-in" });
    renderStoryboard();
    strip.scrollLeft = strip.scrollWidth;
  } }));
  renderPlan();
}

function sourceNames() {
  return Object.keys(docs.edit.sources ?? {});
}

function renderEdit() {
  const doc = docs.edit;
  doc.sources ??= {};
  $("#ed-title").value = doc.title ?? "";
  $("#ed-title").oninput = (event) => { doc.title = event.target.value; };
  $("#ed-sources").replaceChildren(...Object.entries(doc.sources).map(([name, path]) => {
    let currentName = name;
    const nameInput = input(name, (value) => {
      const clean = value.trim().replace(/\s+/g, "-");
      if (!clean || (clean !== currentName && clean in doc.sources)) return;
      doc.sources = Object.fromEntries(Object.entries(doc.sources).map(([key, val]) => [key === currentName ? clean : key, val]));
      for (const clip of doc.clips ?? []) if (clip.source === currentName) clip.source = clean;
      currentName = clean;
      fillFootage();
    }, { attrs: { className: "name", "aria-label": "Source name" } });
    return element("div", { className: "row" }, nameInput,
      input(path, (value) => { doc.sources[currentName] = value; }, { attrs: { placeholder: "~/Videos/recording.mp4", "aria-label": "Video file" } }),
      element("button", { type: "button", textContent: "Index", title: "Find scenes and transcribe with word timings", onclick: guard(() => indexSource(doc.sources[currentName])) }),
      element("button", { type: "button", className: "icon", textContent: "✕", title: "Remove source", onclick: () => { delete doc.sources[currentName]; renderEdit(); } }),
    );
  }));
  doc.clips ??= [];
  $("#ed-clips-empty").hidden = doc.clips.length > 0;
  const names = Object.fromEntries(sourceNames().map((name) => [name, name]));
  $("#ed-clips").replaceChildren(...doc.clips.map((clip, index) => element("div", { className: "row" },
    select(names, clip.source, (value) => { clip.source = value; }, "Clip source"),
    element("label", {}, "in", input(clip.in ?? "0:00", (value) => { clip.in = value; }, { attrs: { className: "time" } })),
    element("label", {}, "out", input(clip.out ?? "", (value) => { clip.out = value; }, { attrs: { className: "time", placeholder: "end" } })),
    element("button", { type: "button", className: "icon", textContent: "✕", title: "Remove clip", onclick: () => { doc.clips.splice(index, 1); if (!doc.clips.length) delete doc.clips; renderEdit(); } }),
  )));
  const tighten = doc.tighten && doc.tighten !== false;
  $("#ed-tighten").checked = Boolean(tighten);
  $("#ed-silence").value = doc.tighten?.silence ?? 0.7;
  $("#ed-fillers").value = (doc.tighten?.fillers ?? ["um", "uh", "erm"]).join(", ");
  const syncTighten = () => {
    doc.tighten = $("#ed-tighten").checked ? { silence: Number($("#ed-silence").value) || 0.7, fillers: $("#ed-fillers").value.split(",").map((word) => word.trim()).filter(Boolean) } : undefined;
  };
  $("#ed-tighten").onchange = syncTighten;
  $("#ed-silence").oninput = syncTighten;
  $("#ed-fillers").oninput = syncTighten;
  doc.formats ??= ["16:9"];
  $("#ed-formats").replaceChildren(...FORMATS.map((format) => {
    const box = element("input", { type: "checkbox", checked: doc.formats.includes(format) });
    box.addEventListener("change", () => {
      doc.formats = FORMATS.filter((item) => item === format ? box.checked : doc.formats.includes(item));
    });
    return element("label", {}, box, format);
  }));
  seg($("#ed-captions"), { karaoke: "Karaoke", plain: "Plain", none: "None" }, () => doc.captions ?? "none", (value) => { doc.captions = value; });
  seg($("#ed-fit"), { crop: "Fill (crop)", pad: "Letterbox" }, () => doc.fit ?? "crop", (value) => { doc.fit = value; });
  fillFootage();
}

function fillFootage() {
  const box = $("#ed-footage");
  const previous = box.value;
  box.replaceChildren(...sourceNames().map((name) => element("option", { value: name, textContent: name, selected: name === previous })));
}

async function showFootage() {
  const path = docs.edit.sources?.[$("#ed-footage").value];
  $("#ed-scenes").replaceChildren();
  $("#ed-hits").replaceChildren();
  if (!path) { $("#ed-footage-info").textContent = "Add a source file first."; return; }
  const summary = await call("media", { path });
  if (!summary.indexed) { $("#ed-footage-info").textContent = "Not indexed yet. Press Index next to the source."; return; }
  $("#ed-footage-info").textContent = `${seconds(summary.duration)} · ${summary.width}×${summary.height} · ${summary.scenes.length} scenes · ${summary.words} words${summary.language ? ` · ${summary.language}` : ""}`;
  $("#ed-scenes").replaceChildren(...summary.scenes.map((scene) => element("button", { type: "button", className: "scene", title: `Use ${clock(scene.start)}–${clock(scene.end)} as a clip`, onclick: () => addClip(scene.start, scene.end) },
    element("img", { src: media(scene.thumb), alt: "" }), element("span", { textContent: `${clock(scene.start)}` }))));
}

function clock(value) {
  const minutes = Math.floor(value / 60);
  return `${minutes}:${(value - minutes * 60).toFixed(1).padStart(4, "0")}`;
}

function addClip(start, end) {
  docs.edit.clips ??= [];
  docs.edit.clips.push({ source: $("#ed-footage").value, in: clock(Math.max(0, start)), out: clock(end) });
  renderEdit();
  toast(`Added ${clock(start)}–${clock(end)} as clip ${docs.edit.clips.length}`);
}

async function search() {
  const path = docs.edit.sources?.[$("#ed-footage").value];
  const result = await call("search", { path, query: $("#ed-query").value });
  $("#ed-hits").replaceChildren(...(result.hits.length ? result.hits.map((hit) => element("li", {},
    element("time", { textContent: clock(hit.start) }), " ", hit.text, " ",
    element("button", { type: "button", textContent: "+ Clip", onclick: () => addClip(hit.start - 0.3, hit.end + 0.3) }))) : [element("li", { className: "muted", textContent: "Nothing found." })]));
}

async function indexSource(path) {
  if (!path) throw new Error("Type the video file path first.");
  await follow(await call("index", { path }), "edit");
  $("#ed-footage").value = sourceNames().find((name) => docs.edit.sources[name] === path) ?? $("#ed-footage").value;
  await showFootage();
  await showPlan();
}

function renderDemo() {
  const doc = docs.demo;
  $("#dm-title").value = doc.title ?? "";
  $("#dm-title").oninput = (event) => { doc.title = event.target.value; };
  $("#dm-url").value = doc.url ?? "";
  $("#dm-url").oninput = (event) => { doc.url = event.target.value; };
  seg($("#dm-format"), forFormats, () => doc.format ?? "16:9", (value) => { doc.format = value; });
  seg($("#dm-captions"), { karaoke: "Karaoke", plain: "Plain", none: "None" }, () => doc.captions ?? "karaoke", (value) => { doc.captions = value; });
  seg($("#dm-zoom"), { off: "Off", "1.5": "1.5×", "2": "2×" }, () => doc.zoom === false ? "off" : String(doc.zoom ?? 1.5), (value) => { if (value === "1.5") delete doc.zoom; else doc.zoom = value === "off" ? false : Number(value); });
  narrationControls("dm", doc);
  doc.steps = (doc.steps ?? []).map((step) => typeof step === "string" ? { say: step } : step);
  $("#dm-steps").replaceChildren(...doc.steps.map((step, index) => {
    const action = Object.keys(ACTIONS).find((key) => key && key in step) ?? "";
    const value = element("span", { className: "grow-row" });
    const fillValue = (kind) => {
      if (kind === "type") {
        step.type = typeof step.type === "object" && step.type ? step.type : { selector: "", text: "" };
        value.replaceChildren(input(step.type.selector, (text) => { step.type.selector = text; }, { attrs: { placeholder: "#search" } }), input(step.type.text, (text) => { step.type.text = text; }, { attrs: { placeholder: "text to type" } }));
      } else if (kind) {
        const hints = { click: "#button or text=Save", press: "Enter", scroll: "500", wait: "1.5", hover: "#menu", goto: "http://localhost:3000/docs" };
        value.replaceChildren(input(step[kind] ?? "", (text) => { step[kind] = ["scroll", "wait"].includes(kind) ? Number(text) || text : text; }, { attrs: { placeholder: hints[kind] } }));
      } else {
        value.replaceChildren();
      }
    };
    const actionBox = select(ACTIONS, action, (kind) => {
      for (const key of Object.keys(ACTIONS)) if (key && key !== kind) delete step[key];
      if (kind && !(kind in step)) step[kind] = kind === "scroll" ? 500 : kind === "wait" ? 1 : "";
      fillValue(kind);
    }, "Action");
    fillValue(action);
    return element("li", {},
      element("div", { className: "row" }, actionBox, value,
        element("button", { type: "button", className: "icon", textContent: "↑", title: "Move up", onclick: () => { if (index) { [doc.steps[index - 1], doc.steps[index]] = [doc.steps[index], doc.steps[index - 1]]; renderDemo(); } } }),
        element("button", { type: "button", className: "icon", textContent: "✕", title: "Remove step", onclick: () => { doc.steps.splice(index, 1); renderDemo(); } })),
      input(step.say ?? "", (text) => { if (text) step.say = text; else delete step.say; }, { multiline: true, attrs: { placeholder: "Narration while this happens (optional)" } }),
    );
  }));
}

const renderers = { storyboard: renderStoryboard, edit: renderEdit, demo: renderDemo };

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

function showStoryboardResult() {
  const result = info?.final?.hash === info?.animatic?.hash && info?.final ? info.final : info?.animatic;
  $("#sb-result").hidden = !result;
  if (!result) return;
  $("#sb-video-title").textContent = result.stage === "final" ? "Final video" : "Animatic";
  const source = media(result.file);
  if (!$("#sb-video").src.endsWith(source)) $("#sb-video").src = `${source}?v=${Date.now()}`;
  $("#sb-video-meta").textContent = `${seconds(result.duration)} · ${result.shots.length} shots · encoded with ${result.encoder}${info.location ? ` · copied to ${info.location}` : ""}`;
  const heard = result.shots.filter((shot) => shot.heard);
  $("#sb-heard-card").hidden = !heard.length;
  $("#sb-heard").replaceChildren(...heard.map((shot) => element("li", {}, element("strong", { textContent: shot.beat || shot.id }), ` ${shot.heard}`)));
}

async function refreshEstimate() {
  if (!project || !info?.documents?.storyboard) return;
  try {
    estimateCache = await call("estimate", { project });
  } catch (error) {
    estimateCache = null;
    $("#sb-gate-text").textContent = `Finish the storyboard to see what a final render costs: ${error.message ?? error}`;
    $("#sb-gate-shots").replaceChildren();
    $("#sb-approve").disabled = true;
    return;
  }
  const estimate = estimateCache;
  const provider = estimate.video?.provider ? `${estimate.video.provider} · ${estimate.video.model}` : "no video provider set in zorai's video settings";
  const cost = estimate.cost != null ? `≈ $${estimate.cost.toFixed(2)}` : estimate.video_calls ? "cost unknown (set your provider's price per second in Video Lab settings)" : "nothing to pay for";
  $("#sb-gate-text").textContent = !estimate.animatic_ready
    ? "Build and watch the animatic for this version first. The final render unlocks when the animatic matches the storyboard."
    : `${estimate.video_calls} shot${estimate.video_calls === 1 ? "" : "s"} to generate, ${estimate.video_seconds} s of video in total, with ${provider}: ${cost}.${estimate.image_calls ? ` Plus ${estimate.image_calls} cloud keyframes.` : ""}`;
  $("#sb-gate-shots").replaceChildren(...estimate.shots.map((shot) => element("li", { textContent: `${shot.beat || shot.id}: ${shot.generate ? `generate ${shot.seconds} s` : shot.cached ? "already generated (cached)" : "keeps its animated keyframe or clip"}` })));
  $("#sb-approve").disabled = !estimate.animatic_ready || (estimate.video_calls > 0 && !estimate.video?.provider);
  disarm();
}

function disarm() {
  approveArmed = false;
  $("#sb-approve").textContent = "Approve render";
  $("#sb-approve").classList.remove("armed");
}

function showCutResult() {
  const cut = info?.cut;
  $("#ed-result").hidden = !cut;
  if (!cut) return;
  const removed = cut.removed ?? {};
  $("#ed-result-text").textContent = `${seconds(cut.original)} cut to ${seconds(cut.duration)} (${removed.fillers ?? 0} filler words, ${removed.pauses ?? 0} long pauses removed) · ${cut.encoder}`;
  $("#ed-videos").replaceChildren(...cut.outputs.map((output) => element("figure", {}, element("video", { src: `${media(output.file)}?v=${Date.now()}`, controls: true, preload: "metadata" }), element("figcaption", { textContent: output.format }))));
}

function showDemoResult() {
  const demo = info?.demo;
  $("#dm-result").hidden = !demo;
  if (!demo) return;
  $("#dm-video").src = `${media(demo.file)}?v=${Date.now()}`;
  $("#dm-meta").textContent = `${seconds(demo.duration)} · ${demo.steps.length} steps · ${demo.encoder}`;
}

async function showPlan() {
  if (!project || !info?.documents?.edit) { $("#ed-plan").textContent = ""; return; }
  try {
    const plan = await call("plan", { project });
    $("#ed-plan").textContent = plan.needs_index.length
      ? `Index ${plan.needs_index.join(", ")} to tighten and caption it.`
      : `Plan: ${seconds(plan.original)} → ${seconds(plan.duration)} in ${plan.segments} pieces (${plan.removed.fillers} filler words, ${plan.removed.pauses} pauses removed).`;
  } catch (error) {
    $("#ed-plan").textContent = String(error.message ?? error);
  }
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

async function openProject(name) {
  project = name;
  $("#project").value = name;
  info = name ? await call("load", { project: name }) : null;
  for (const kind of KINDS) {
    const stored = info?.documents?.[kind];
    docs[kind] = stored?.document ?? clone(BLANK[kind]);
    showProblem(kind, stored?.problem);
    if (modes[kind] === "yaml") $(".yaml", section(kind)).value = stored?.text ?? (await call("dump", { document: docs[kind] })).text;
    renderers[kind]();
  }
  renderLocation();
  showStoryboardResult();
  showCutResult();
  showDemoResult();
  await Promise.all([refreshEstimate(), showPlan(), showFootage().catch(() => {})]);
}

async function reload() {
  info = await call("load", { project });
  renderLocation();
  showStoryboardResult();
  showCutResult();
  showDemoResult();
  renderStoryboard();
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
    if (kind === "storyboard") await refreshEstimate();
    if (kind === "edit") await showPlan();
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
  await call("save", { project: name, kind: "storyboard", document: BLANK.storyboard });
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
  toast(`New renders of ${project} are also copied to ${folder}`);
}));
$("#clear-folder").addEventListener("click", guard(async () => {
  await call("location", { project, folder: "" });
  await reload();
}));

$("#sb-animatic").addEventListener("click", guard(async () => {
  await save("storyboard", true);
  await follow(await call("animatic", { project }), "storyboard");
  await reload();
  await refreshEstimate();
}));
$("#sb-estimate").addEventListener("click", guard(refreshEstimate));
$("#sb-route-save").addEventListener("click", guard(async () => {
  await save("storyboard", true);
  await reload();
  await refreshEstimate();
}));
$("#sb-approve").addEventListener("click", guard(async () => {
  if (!approveArmed) {
    approveArmed = true;
    const cost = estimateCache?.cost != null ? `$${estimateCache.cost.toFixed(2)}` : "an unknown amount";
    $("#sb-approve").textContent = estimateCache?.video_calls ? `Confirm: spend ${cost}` : "Confirm render";
    $("#sb-approve").classList.add("armed");
    setTimeout(disarm, 8000);
    return;
  }
  disarm();
  await follow(await call("render", { project, approve: true }), "storyboard");
  await reload();
  await refreshEstimate();
}));
$("#sb-ask").addEventListener("click", guard(async () => {
  requireProject();
  const yaml = (await call("dump", { document: docs.storyboard })).text;
  await zorai("ask", { text: `Improve the Video Lab storyboard of project "${project}" (tighter hook, clearer beats, narration that fits each shot's length). Save it with /video-lab.save --project ${project} --kind storyboard and build the animatic.\n\n\`\`\`yaml\n${yaml}\`\`\`\n\nAlso: ` });
}));
$("#sb-ask-review").addEventListener("click", guard(async () => {
  const result = info?.final ?? info?.animatic;
  if (!result) throw new Error("Build the animatic first.");
  await zorai("ask", { text: `Review the Video Lab ${result.stage} of project "${project}" (${result.path}). Shots: ${result.shots.map((shot) => `${shot.beat || shot.id} ${shot.seconds}s "${shot.heard ?? ""}"`).join("; ")}. Look at pacing, the hook and the call to action, and suggest storyboard changes. ` });
}));
$("#sb-to-chat").addEventListener("click", guard(async () => {
  const result = info?.final ?? info?.animatic;
  if (!result) throw new Error("Build the animatic first.");
  await zorai("insertText", { text: `Video Lab ${result.stage} for "${project}": ${seconds(result.duration)}, ${result.shots.length} shots. File: ${result.path}` });
}));

$("#ed-add-source").addEventListener("click", () => {
  docs.edit.sources ??= {};
  let number = Object.keys(docs.edit.sources).length + 1;
  while (`source-${number}` in docs.edit.sources) number += 1;
  docs.edit.sources[`source-${number}`] = "";
  renderEdit();
});
$("#ed-add-clip").addEventListener("click", () => {
  docs.edit.clips ??= [];
  docs.edit.clips.push({ source: sourceNames()[0] ?? "main", in: "0:00", out: "0:10" });
  renderEdit();
});
$("#ed-footage").addEventListener("change", guard(showFootage));
$("#ed-search").addEventListener("click", guard(search));
$("#ed-query").addEventListener("keydown", (event) => { if (event.key === "Enter") guard(search)(); });
$("#ed-cut").addEventListener("click", guard(async () => {
  await save("edit", true);
  await follow(await call("cut", { project }), "edit");
  await reload();
  await showPlan();
}));

$("#dm-add-step").addEventListener("click", () => {
  docs.demo.steps ??= [];
  docs.demo.steps.push({ click: "", say: "" });
  renderDemo();
});
$("#dm-record").addEventListener("click", guard(async () => {
  await save("demo", true);
  await follow(await call("demo", { project }), "demo");
  await reload();
}));

async function resumeRunning() {
  const { jobs } = await call("jobs");
  const running = jobs.find((job) => ["running", "queued"].includes(job.state) && job.params?.project === project);
  const kind = { animatic: "storyboard", render: "storyboard", cut: "edit", demo: "demo" }[running?.kind];
  if (!kind) return;
  await follow(running, kind);
  await reload();
  await refreshEstimate();
}

$("#jobs-refresh").addEventListener("click", guard(loadJobs));
$("#recheck").addEventListener("click", guard(loadChecks));

void zorai("theme", {}).then(applyTheme).catch(() => {});
guard(async () => {
  for (const kind of KINDS) renderers[kind]();
  const projects = await refreshProjects();
  if (projects.length) await openProject(projects[0].name);
  renderLocation();
  call("voices").then((result) => { voices = result.voices ?? []; renderStoryboard(); renderDemo(); }).catch(() => {});
  await loadChecks();
  await resumeRunning();
})();
$("#storyboard").addEventListener("input", renderPlan);
$("#storyboard").addEventListener("click", () => queueMicrotask(renderPlan));
