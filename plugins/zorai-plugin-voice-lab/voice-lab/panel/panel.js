const EXAMPLE = {
  title: "Zorai launch briefing",
  lexicon: { Zorai: "Zor-eye", PyTorch: "Pie-torch", MLflow: "M L flow" },
  cast: { host: { voice: "af_heart" }, guest: { voice: "am_michael", speed: 1.05 } },
  pause_ms: 350,
  check: { max_wer: 0.1, attempts: 3 },
  lines: [
    { speaker: "host", text: "Welcome to the Zorai briefing. Today we look at four new lab plugins." },
    { speaker: "guest", text: "Thanks! The headline is that everything runs locally, on your own GPU." },
    { speaker: "host", text: "So the voice you are hearing right now was rendered by Kokoro, and checked by Whisper." },
    { speaker: "guest", text: "Exactly. Every line is transcribed back. If a word goes missing, Zorai retries it.", pause_ms: 600 },
    { speaker: "host", text: "And the ML Research Lab tracks every PyTorch run in MLflow." },
    { speaker: "guest", text: "That's the briefing. Thanks for listening!" },
  ],
};

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
const media = (file) => `/.data/${file.split("/").map(encodeURIComponent).join("/")}`;
const percent = (value) => `${Math.round((value ?? 0) * 100)}%`;
const clone = (value) => JSON.parse(JSON.stringify(value));
let currentJob = null;
let lastStatus = null;
let voices = [];
let doc = clone(EXAMPLE);
let mode = "form";
let cloud = { speak: null, listen: null };
let transcribeRoute = null;
let projectInfo = { folder: "", location: null };

function renderLocation() {
  const name = $("#project").value;
  $("#location-text").textContent = !name
    ? ""
    : projectInfo.location
      ? `📁 Also saved to ${projectInfo.location}: ${name}.voice.yaml, and ${name}.mp3, .wav and .srt after each render`
      : `📁 Saved in Voice Lab's folder: ${projectInfo.folder}`;
  $("#choose-folder").textContent = projectInfo.location ? "Change folder…" : "Choose folder…";
  $("#choose-folder").hidden = !name;
  $("#clear-folder").hidden = !projectInfo.location;
}

function typedName() {
  const name = $("#new-name").value.trim().toLowerCase().replace(/[^a-z0-9_-]+/g, "-").replace(/^-+|-+$/g, "");
  if (!name) throw new Error("Type a project name first (letters, digits, - and _).");
  return name;
}
const engine = (kind) => doc.engine?.[kind] === "cloud" ? "cloud" : "local";

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
  const list = document.createElement("datalist");
  list.id = `models-${kind}-${Math.random().toString(36).slice(2, 8)}`;
  model.setAttribute("list", list.id);
  wrap.append(provider, model, list);
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
      list.replaceChildren(...(entry?.models ?? []).map((item) => new Option(item.name, item.id)));
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

const player = { audio: null, button: null, label: "", ticket: 0 };

function stopPlayback() {
  player.ticket += 1;
  player.audio?.pause();
  if (player.button) {
    player.button.textContent = player.label;
    player.button.disabled = false;
    player.button.setAttribute("aria-pressed", "false");
    player.button.classList.remove("is-playing");
  }
  player.audio = null;
  player.button = null;
}

async function togglePlay(button, getFile) {
  if (player.button === button) {
    stopPlayback();
    return;
  }
  stopPlayback();
  $("#mix").pause();
  const ticket = player.ticket;
  player.button = button;
  player.label = button.textContent;
  button.textContent = "…";
  button.disabled = true;
  try {
    const file = await getFile();
    if (ticket !== player.ticket) return;
    const audio = new Audio(media(file));
    player.audio = audio;
    audio.addEventListener("ended", () => { if (player.audio === audio) stopPlayback(); });
    button.textContent = "■";
    button.disabled = false;
    button.setAttribute("aria-pressed", "true");
    button.classList.add("is-playing");
    await audio.play();
  } catch (error) {
    if (ticket === player.ticket) stopPlayback();
    throw error;
  }
}

function playButton(label, getFile) {
  const button = element("button", { type: "button", className: "play", textContent: "▶", title: label });
  button.setAttribute("aria-label", label);
  button.setAttribute("aria-pressed", "false");
  button.addEventListener("click", guard(() => togglePlay(button, getFile)));
  return button;
}

$("#mix").addEventListener("play", () => stopPlayback());

for (const tab of document.querySelectorAll("[role=tab]")) {
  tab.addEventListener("click", () => {
    for (const other of document.querySelectorAll("[role=tab]")) other.setAttribute("aria-selected", String(other === tab));
    for (const section of document.querySelectorAll(".tab")) section.hidden = section.id !== tab.dataset.tab;
    if (tab.dataset.tab === "setup") void guard(refreshChecks)();
  });
}

function speakers() {
  return Object.keys(doc.cast);
}

function previewFor(text, speaker) {
  const spec = doc.cast[speaker] ?? {};
  const cloudVoice = engine("speak") === "cloud";
  return async () => (await call("preview", { text, engine: engine("speak"), voice: cloudVoice ? spec.cloud_voice || "" : spec.voice || "af_heart", speed: Number(spec.speed) || 1, lang: spec.lang || "en-us", lexicon: doc.lexicon, route: doc.cloud?.speak })).file;
}

function renderEngine() {
  const options = {
    speak: [["local", "Local · Kokoro"], ["cloud", cloud.speak ? `Cloud · ${cloud.speak}` : "Cloud · not set up"]],
    listen: [["local", "Local · Whisper"], ["cloud", cloud.listen ? `Cloud · ${cloud.listen}` : "Cloud · not set up"]],
  };
  for (const kind of ["speak", "listen"]) {
    const box = $(`#e-${kind}`);
    box.innerHTML = "";
    box.parentElement.querySelector(".route")?.remove();
    for (const [value, label] of options[kind]) {
      const button = element("button", { type: "button", textContent: label, disabled: value === "cloud" && !cloud[kind] });
      button.setAttribute("aria-pressed", String(engine(kind) === value));
      button.addEventListener("click", () => {
        doc.engine = { ...(doc.engine ?? {}), [kind]: value };
        if (doc.engine[kind] === "local") delete doc.engine[kind];
        renderEngine();
        renderCast();
      });
      box.append(button);
    }
    if (engine(kind) === "cloud") {
      box.after(routePicker(kind === "speak" ? "tts" : "stt", doc.cloud?.[kind], (route) => {
        doc.cloud = { ...(doc.cloud ?? {}) };
        if (route) doc.cloud[kind] = route;
        else delete doc.cloud[kind];
      }));
    }
  }
  const paid = ["speak", "listen"].filter((kind) => engine(kind) === "cloud");
  $("#e-cost").hidden = paid.length === 0;
  $("#e-cost").textContent = paid.length ? `Cloud ${paid.map((kind) => (kind === "speak" ? "voices" : "listening")).join(" and ")} use the provider and model chosen here (zorai's audio settings when left on default). Each line, retry and preview is a paid request there.` : "";
}

function voiceSelect(value, onChange) {
  const select = element("select", { "aria-label": "Voice" });
  const options = voices.length ? voices : [value || "af_heart"];
  for (const voice of options) select.append(new Option(voice, voice));
  if (value && !options.includes(value)) select.append(new Option(value, value));
  select.value = value || "af_heart";
  select.addEventListener("change", () => onChange(select.value));
  return select;
}

function numberInput(value, attrs, onChange) {
  const input = element("input", { type: "number", value: value ?? "", ...attrs });
  input.addEventListener("input", () => onChange(input.value === "" ? undefined : Number(input.value)));
  return input;
}

function renameSpeaker(from, to) {
  to = to.trim().toLowerCase().replace(/[^a-z0-9_-]+/g, "-");
  if (!to || to === from || doc.cast[to]) return false;
  doc.cast = Object.fromEntries(Object.entries(doc.cast).map(([name, spec]) => [name === from ? to : name, spec]));
  for (const line of doc.lines) if (line.speaker === from) line.speaker = to;
  return true;
}

function renderCast() {
  const box = $("#f-cast");
  box.innerHTML = "";
  for (const [name, spec] of Object.entries(doc.cast)) {
    const nameInput = element("input", { value: name, className: "name", "aria-label": "Speaker name" });
    nameInput.addEventListener("change", () => { if (!renameSpeaker(name, nameInput.value)) nameInput.value = name; renderForm(); });
    const remove = element("button", { type: "button", className: "icon", textContent: "✕", title: "Remove speaker", disabled: speakers().length < 2 });
    remove.addEventListener("click", () => {
      const fallback = speakers().find((other) => other !== name);
      delete doc.cast[name];
      for (const line of doc.lines) if (line.speaker === name) line.speaker = fallback;
      renderForm();
    });
    const cloudVoiceInput = element("input", { value: spec.cloud_voice ?? "", placeholder: `voice (default ${cloud.speakVoice || "from settings"})`, "aria-label": "Cloud voice" });
    cloudVoiceInput.addEventListener("input", () => { if (cloudVoiceInput.value.trim()) spec.cloud_voice = cloudVoiceInput.value.trim(); else delete spec.cloud_voice; });
    box.append(element("div", { className: "row" },
      nameInput,
      engine("speak") === "cloud" ? cloudVoiceInput : voiceSelect(spec.voice, (voice) => { spec.voice = voice; }),
      numberInput(spec.speed ?? 1, { min: 0.5, max: 2, step: 0.05, className: "speed", title: "Speed", "aria-label": "Speed" }, (speed) => { spec.speed = speed; }),
      playButton(`Hear ${name}`, () => previewFor($("#f-sample").value || "Hello there.", name)()),
      remove,
    ));
  }
}

function renderLines() {
  const list = $("#f-lines");
  list.innerHTML = "";
  doc.lines.forEach((line, index) => {
    const speaker = element("select", { "aria-label": `Speaker for line ${index + 1}` });
    for (const name of speakers()) speaker.append(new Option(name, name));
    speaker.value = line.speaker;
    speaker.addEventListener("change", () => { line.speaker = speaker.value; });
    const text = element("textarea", { value: line.text, rows: 2, "aria-label": `Line ${index + 1}` });
    const fit = () => { text.style.height = "auto"; text.style.height = `${text.scrollHeight + 2}px`; };
    text.addEventListener("input", () => { line.text = text.value; fit(); });
    requestAnimationFrame(fit);
    const pause = numberInput(line.pause_ms, { min: 0, max: 10000, step: 50, placeholder: String(doc.pause_ms ?? 350), className: "pause", title: "Pause after this line (ms)", "aria-label": "Pause after (ms)" }, (value) => {
      if (value === undefined) delete line.pause_ms; else line.pause_ms = value;
    });
    const move = (offset) => {
      const target = index + offset;
      if (target < 0 || target >= doc.lines.length) return;
      [doc.lines[index], doc.lines[target]] = [doc.lines[target], doc.lines[index]];
      renderLines();
    };
    const up = element("button", { type: "button", className: "icon", textContent: "↑", title: "Move up", disabled: index === 0 });
    up.addEventListener("click", () => move(-1));
    const down = element("button", { type: "button", className: "icon", textContent: "↓", title: "Move down", disabled: index === doc.lines.length - 1 });
    down.addEventListener("click", () => move(1));
    const remove = element("button", { type: "button", className: "icon", textContent: "✕", title: "Remove line", disabled: doc.lines.length < 2 });
    remove.addEventListener("click", () => { doc.lines.splice(index, 1); renderLines(); });
    list.append(element("li", {},
      element("div", { className: "line-head" }, speaker, pause, playButton(`Hear line ${index + 1}`, () => previewFor(line.text, line.speaker)()), up, down, remove),
      text,
    ));
  });
}

function renderLexicon() {
  const box = $("#f-lexicon");
  box.innerHTML = "";
  for (const [word, spoken] of Object.entries(doc.lexicon)) {
    const wordInput = element("input", { value: word, placeholder: "Zorai", "aria-label": "Word" });
    const spokenInput = element("input", { value: spoken, placeholder: "Zor-eye", "aria-label": "Say it as" });
    const update = () => {
      doc.lexicon = Object.fromEntries(Object.entries(doc.lexicon).map(([key, value]) => (key === word ? [wordInput.value, spokenInput.value] : [key, value])));
    };
    wordInput.addEventListener("change", () => { update(); renderLexicon(); });
    spokenInput.addEventListener("input", () => { doc.lexicon[word] = spokenInput.value; });
    const remove = element("button", { type: "button", className: "icon", textContent: "✕", title: "Remove word" });
    remove.addEventListener("click", () => { delete doc.lexicon[word]; renderLexicon(); });
    box.append(element("div", { className: "row" }, wordInput, element("span", { className: "muted", textContent: "→" }), spokenInput, playButton(`Hear ${word}`, () => previewFor(word, speakers()[0])()), remove));
  }
}

function renderForm() {
  stopPlayback();
  $("#f-title").value = doc.title ?? "";
  $("#f-pause").value = doc.pause_ms ?? "";
  $("#f-wer").value = doc.check?.max_wer === undefined ? "" : Math.round(doc.check.max_wer * 100);
  $("#f-attempts").value = doc.check?.attempts ?? "";
  renderEngine();
  renderCast();
  renderLines();
  renderLexicon();
}

function normalize(document) {
  const next = clone(document ?? {});
  next.title = next.title ?? "";
  next.cast = next.cast && Object.keys(next.cast).length ? next.cast : { host: { voice: "af_heart" } };
  next.lines = Array.isArray(next.lines) && next.lines.length ? next.lines : [{ speaker: Object.keys(next.cast)[0], text: "" }];
  next.lexicon = next.lexicon ?? {};
  next.check = next.check ?? {};
  next.engine = next.engine ?? {};
  if (next.pause_ms === null) delete next.pause_ms;
  return next;
}

$("#f-title").addEventListener("input", () => { doc.title = $("#f-title").value; });
$("#f-pause").addEventListener("input", () => { const value = $("#f-pause").value; if (value === "") delete doc.pause_ms; else doc.pause_ms = Number(value); renderLines(); });
$("#f-wer").addEventListener("input", () => { const value = $("#f-wer").value; if (value === "") delete doc.check.max_wer; else doc.check.max_wer = Number(value) / 100; });
$("#f-attempts").addEventListener("input", () => { const value = $("#f-attempts").value; if (value === "") delete doc.check.attempts; else doc.check.attempts = Number(value); });
$("#add-speaker").addEventListener("click", () => {
  let number = speakers().length + 1;
  while (doc.cast[`speaker${number}`]) number += 1;
  const used = Object.values(doc.cast).map((spec) => spec.voice);
  doc.cast[`speaker${number}`] = { voice: voices.find((voice) => !used.includes(voice)) ?? "af_heart" };
  renderForm();
});
$("#add-line").addEventListener("click", () => {
  const last = doc.lines.at(-1)?.speaker;
  const names = speakers();
  doc.lines.push({ speaker: names[(names.indexOf(last) + 1) % names.length] ?? names[0], text: "" });
  renderLines();
  $("#f-lines").lastElementChild?.querySelector("textarea")?.focus();
});
$("#add-word").addEventListener("click", () => {
  let word = "Word";
  for (let number = 2; word in doc.lexicon; number += 1) word = `Word${number}`;
  doc.lexicon[word] = "";
  renderLexicon();
  $("#f-lexicon").lastElementChild?.querySelector("input")?.select();
});

function setProblem(problem) {
  $("#problem").textContent = problem ?? "";
  $("#problem").hidden = !problem;
}

async function setMode(next) {
  if (next === mode) return;
  if (next === "yaml") {
    $("#script").value = (await call("dump", { document: doc })).script;
  } else {
    const parsed = await call("parse", { script: $("#script").value });
    setProblem(parsed.problem);
    if (!parsed.document) throw new Error(`Fix the YAML first: ${parsed.problem}`);
    doc = normalize(parsed.document);
    renderForm();
  }
  showMode(next);
}

function showMode(next) {
  mode = next;
  $("#form").hidden = mode !== "form";
  $("#script").hidden = mode !== "yaml";
  for (const button of document.querySelectorAll("[data-mode]")) button.setAttribute("aria-pressed", String(button.dataset.mode === mode));
}
for (const button of document.querySelectorAll("[data-mode]")) button.addEventListener("click", guard(() => setMode(button.dataset.mode)));

async function saveCurrent() {
  const name = $("#project").value;
  if (!name) throw new Error("Create a project first: type a name and press New.");
  if (mode === "form") await call("save", { project: name, document: doc });
  else await call("save", { project: name, script: $("#script").value });
  const parsed = await call("load", { project: name });
  setProblem(parsed.problem);
  return parsed;
}

async function refreshProjects(select) {
  const { projects } = await call("projects");
  const picker = $("#project");
  picker.innerHTML = "";
  for (const project of projects) picker.append(new Option(project.name, project.name));
  if (select) picker.value = select;
  if (picker.value) await openProject(picker.value);
  else { projectInfo = { folder: "", location: null }; renderLocation(); renderForm(); }
}

async function openProject(name) {
  const project = await call("load", { project: name });
  projectInfo = { folder: project.folder, location: project.location };
  renderLocation();
  setProblem(project.problem);
  $("#script").value = project.script;
  if (project.document) {
    doc = normalize(project.document);
    renderForm();
    showMode("form");
  } else {
    showMode("yaml");
  }
  showLatest(project.latest);
}

function showLatest(latest) {
  lastStatus = latest ?? null;
  $("#result").hidden = !latest;
  if (!latest) return;
  $("#mix").src = media(latest.outputs.mp3);
  renderReport(latest.report, latest.outputs);
}

function renderReport(report, outputs) {
  const rows = [
    ["Length", `${Math.round(report.duration_s)} s`],
    ["Lines", `${report.lines} (${report.reused} reused from earlier renders)`],
    ["Listening check", report.flagged.length ? `${report.flagged.length} line(s) still above ${percent(report.max_wer)}: ${report.flagged.join(", ")}` : `all passed, average ${percent(report.mean_wer)} wrong`],
    ["Loudness", `${report.loudness.after_lufs} LUFS (was ${report.loudness.before_lufs}), peak ${report.loudness.true_peak} dBTP`],
    ["Heard on", report.listener],
  ];
  $("#report").innerHTML = "";
  for (const [label, value] of rows) $("#report").append(element("dt", { textContent: label }), element("dd", { textContent: value }));
  $("#paths").textContent = report.exported?.length ? `Saved to ${report.exported[0].replace(/\.mp3$/, "")} (.mp3, .wav, .srt)` : `~/.zorai/plugin-data/voice-lab/${outputs.mp3}`;
}

function renderResults(lines, maxWer) {
  const body = $("#lines tbody");
  body.innerHTML = "";
  $("#lines").hidden = !lines?.length;
  $("#studio-empty").hidden = Boolean(lines?.length);
  for (const line of lines ?? []) {
    const row = body.insertRow();
    row.insertCell().textContent = line.index;
    row.insertCell().textContent = line.speaker;
    const text = row.insertCell();
    text.textContent = line.text;
    if (line.wer > 0) text.title = `Heard: ${line.heard}`;
    const badge = element("span", { className: `badge ${line.wer <= maxWer ? "ok" : "warn"}`, textContent: `${percent(line.wer)}${line.cached ? " · reused" : line.attempts?.length > 1 ? ` · ${line.attempts.length} tries` : ""}` });
    badge.title = line.missed?.length ? `Missing: ${line.missed.join(", ")}` : "Every word was heard";
    row.insertCell().append(badge);
    row.insertCell().append(playButton(`Play rendered line ${line.index}`, async () => line.file));
  }
}

async function follow(job, view) {
  currentJob = job.id;
  view.box.hidden = false;
  let status = job;
  while (status && !["done", "failed", "cancelled"].includes(status.state)) {
    view.box.querySelector(".meter span").style.width = percent(status.progress);
    view.box.querySelector("p").textContent = status.message;
    view.onProgress?.(status);
    await new Promise((resolve) => setTimeout(resolve, 900));
    status = await call("status", { job: job.id });
  }
  view.box.hidden = true;
  currentJob = null;
  if (status.state === "failed") throw new Error(`${status.message}${status.error && status.error !== status.message ? `\n${status.error}` : ""}`);
  if (status.state === "done") view.onDone?.(status);
  return status;
}

const studioView = {
  get box() { return $("#progress"); },
  onProgress: (status) => renderResults(status.lines, doc.check?.max_wer ?? 0.1),
  onDone: (status) => {
    renderResults(status.lines, status.report.max_wer);
    showLatest({ outputs: status.outputs, report: status.report });
    void zorai("toast", { text: status.message }).catch(() => {});
  },
};

$("#project").addEventListener("change", guard(() => openProject($("#project").value)));
$("#new-project").addEventListener("click", guard(async () => {
  const name = typedName();
  if ([...$("#project").options].some((option) => option.value === name)) throw new Error(`A project called ${name} already exists.`);
  await call("save", { project: name, document: { title: name, cast: { host: { voice: "af_heart" } }, lines: [{ speaker: "host", text: "Hello, and welcome." }] } });
  $("#new-name").value = "";
  await refreshProjects(name);
}));
$("#example").addEventListener("click", guard(async () => {
  doc = clone(EXAMPLE);
  if (mode === "yaml") $("#script").value = (await call("dump", { document: doc })).script;
  renderForm();
}));
$("#save").addEventListener("click", guard(async () => {
  const saved = await call("save", mode === "form" ? { project: $("#project").value, document: doc } : { project: $("#project").value, script: $("#script").value });
  setProblem((await call("load", { project: $("#project").value })).problem);
  void zorai("toast", { text: `Saved ${saved.saved.length > 1 ? `to Voice Lab and ${projectInfo.location}` : "in Voice Lab's folder"}` }).catch(() => {});
}));
$("#save-as").addEventListener("click", guard(async () => {
  const name = typedName();
  if ([...$("#project").options].some((option) => option.value === name)) throw new Error(`A project called ${name} already exists.`);
  await call("save", mode === "form" ? { project: name, document: doc } : { project: name, script: $("#script").value });
  $("#new-name").value = "";
  await refreshProjects(name);
}));
$("#rename").addEventListener("click", guard(async () => {
  const current = $("#project").value;
  if (!current) throw new Error("There is no project to rename yet.");
  const name = typedName();
  await saveCurrent();
  await call("rename", { project: current, to: name });
  $("#new-name").value = "";
  await refreshProjects(name);
}));
$("#choose-folder").addEventListener("click", guard(async () => {
  const name = $("#project").value;
  const folder = await zorai("pickFolder", { title: `Folder for ${name}` });
  if (!folder) return;
  projectInfo.location = (await call("location", { project: name, folder })).location;
  await saveCurrent();
  renderLocation();
  void zorai("toast", { text: `${name} is now also saved to ${folder}` }).catch(() => {});
}));
$("#clear-folder").addEventListener("click", guard(async () => {
  projectInfo.location = (await call("location", { project: $("#project").value, folder: "" })).location;
  renderLocation();
}));
$("#render").addEventListener("click", guard(async () => {
  $("#render").disabled = true;
  try {
    const saved = await saveCurrent();
    if (saved.problem) throw new Error(saved.problem);
    stopPlayback();
    await follow(await call("render", { project: $("#project").value }), studioView);
  } finally {
    $("#render").disabled = false;
  }
}));
$("#cancel").addEventListener("click", guard(async () => { if (currentJob) await call("cancel", { job: currentJob }); }));
$("#to-message").addEventListener("click", guard(async () => {
  const report = lastStatus?.report;
  const outputs = lastStatus?.outputs;
  if (!report) throw new Error("Render something first.");
  await zorai("insertText", { text: `Voice Lab rendered "${report.title}": ${Math.round(report.duration_s)} s, ${report.lines} lines, average ${percent(report.mean_wer)} misheard, ${report.loudness.after_lufs} LUFS. File: ~/.zorai/plugin-data/voice-lab/${outputs.mp3}` });
}));

$("#transcribe-go").addEventListener("click", guard(async () => {
  const path = $("#path").value.trim();
  if (!path) throw new Error("Paste the full path of an audio or video file.");
  $("#t-result").hidden = true;
  const status = await follow(await call("transcribe", { path, language: $("#language").value.trim(), engine: $("#t-engine").value, cloud_provider: transcribeRoute?.provider, cloud_model: transcribeRoute?.model }), { get box() { return $("#t-progress"); } });
  if (status.state !== "done") return;
  const list = $("#segments");
  list.innerHTML = "";
  for (const segment of status.segments) {
    const item = document.createElement("li");
    const time = document.createElement("time");
    time.textContent = `${segment.start.toFixed(1)}s`;
    item.append(time, ` ${segment.text}`);
    list.append(item);
  }
  $("#t-paths").textContent = `~/.zorai/plugin-data/voice-lab/${status.outputs.srt}`;
  $("#t-result").hidden = false;
  $("#t-to-message").onclick = guard(() => zorai("insertText", { text: status.segments.map((segment) => segment.text).join(" ") }));
}));

async function refreshChecks() {
  const report = await call("check");
  const body = $("#checks tbody");
  body.innerHTML = "";
  for (const item of report.items) {
    const row = body.insertRow();
    row.insertCell().textContent = item.ok ? "✅" : "❌";
    row.insertCell().textContent = item.name;
    row.insertCell().textContent = item.detail;
  }
  const item = (name) => report.items.find((entry) => entry.name === name);
  const speak = item("Cloud voice");
  const listen = item("Cloud transcription");
  cloud = {
    speak: speak?.ok ? speak.detail.split(" · ").slice(0, 2).join(" · ") : null,
    speakVoice: speak?.ok ? (speak.detail.match(/voice (\S+)/)?.[1] ?? "") : "",
    listen: listen?.ok ? listen.detail.split(" · ").slice(0, 2).join(" · ").replace(/^(\S+) · (.{0,40}).*$/, "$1 · $2") : null,
  };
  $("#t-engine").options[1].disabled = !cloud.listen;
  renderEngine();
  $("#ready").textContent = report.ready ? "Ready" : "Needs setup";
  $("#ready").className = `chip ${report.ready ? "ok" : "warn"}`;
  return report;
}
$("#recheck").addEventListener("click", guard(refreshChecks));
$("#install").addEventListener("click", guard(async () => {
  $("#install").disabled = true;
  try {
    await follow(await call("setup"), { get box() { return $("#s-progress"); } });
    await refreshChecks();
    await loadVoices();
  } finally {
    $("#install").disabled = false;
  }
}));

async function loadVoices() {
  voices = (await call("voices")).voices;
  renderCast();
}

async function resumeRunning() {
  const { jobs } = await call("jobs");
  const running = jobs.find((job) => job.state === "running" && job.kind === "render");
  if (running) await follow(running, studioView);
}

guard(async () => {
  renderForm();
  await refreshProjects();
  const report = await refreshChecks();
  if (report.ready) await loadVoices();
  await resumeRunning();
})();

$("#t-engine").addEventListener("change", () => {
  $("#t-route").replaceChildren(...($("#t-engine").value === "cloud" ? [routePicker("stt", transcribeRoute, (route) => { transcribeRoute = route; })] : []));
});
