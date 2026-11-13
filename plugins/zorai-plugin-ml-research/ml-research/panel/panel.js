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
const num = (value) => value === null || value === undefined ? "–" : Number(value).toPrecision(4).replace(/\.?0+(e|$)/, "$1");
const meanStd = (row) => row.mean === null || row.mean === undefined ? "–" : `${num(row.mean)}${row.std !== null && row.std !== undefined ? ` ± ${num(row.std)}` : ""}`;
const toast = (text) => void zorai("toast", { text }).catch(() => {});

function element(tag, props = {}, ...children) {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children.filter((child) => child !== null && child !== undefined));
  return node;
}

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

const saved = (() => {
  try {
    return window.localStorage;
  } catch {
    const memory = new Map();
    return { getItem: (key) => memory.get(key) ?? null, setItem: (key, value) => memory.set(key, String(value)) };
  }
})();
const state = { campaign: "", spec: null, contract: null, board: null, runs: [], attribution: null, report: null, job: null, selectedRun: null, active: [], paper: null, workspace: saved.getItem("ml-research:workspace") || "" };
const FREE_TABS = ["setup", "data", "runtimes"];

async function openTerminal(tab) {
  await zorai("terminal", tab);
  toast(`Opened ${tab.title} in a terminal tab`);
}

function elapsed(since) {
  const seconds = Math.max(0, Math.round(Date.now() / 1000 - since));
  return seconds < 90 ? `${seconds} s` : `${Math.round(seconds / 60)} min`;
}

function campaign() {
  if (!state.campaign) throw new Error("Open a repository or try the example first.");
  return state.campaign;
}

function selectTab(name) {
  for (const button of document.querySelectorAll("[role=tab]")) button.setAttribute("aria-selected", String(button.dataset.tab === name));
  for (const section of document.querySelectorAll(".tab")) section.hidden = section.id !== name || (!state.campaign && !FREE_TABS.includes(name));
  $("#no-campaign").hidden = Boolean(state.campaign) || FREE_TABS.includes(name);
  $("#loading").hidden = true;
  if (name === "runtimes" && !$("#rt-cards").children.length) guard(loadRuntimes)();
  if (name === "dashboard") guard(refreshDashboard)();
}

function markFunnel() {
  const done = {
    contract: Boolean(state.contract?.locked),
    investigate: Boolean(state.board?.rows?.some((row) => row.kind === "candidate" && row.better && row.status === "ok")),
    ablate: Boolean(state.attribution?.files?.length),
    report: Boolean(state.report),
  };
  for (const [tab, complete] of Object.entries(done)) document.querySelector(`[data-tab=${tab}]`).classList.toggle("done", complete);
}

async function loadCampaigns(select) {
  const { campaigns } = await call("campaigns");
  const picker = $("#campaign");
  picker.replaceChildren(...campaigns.map((item) => element("option", { value: item.name, textContent: `${item.name}${item.locked ? "" : " (not locked)"}` })));
  const wanted = select || state.campaign || saved.getItem("ml-research:campaign");
  state.campaign = campaigns.some((item) => item.name === wanted) ? wanted : campaigns[0]?.name ?? "";
  picker.value = state.campaign;
  picker.disabled = !campaigns.length;
  if (state.campaign) saved.setItem("ml-research:campaign", state.campaign);
  await refreshCampaign();
}

async function refreshCampaign() {
  state.spec = state.contract = state.board = state.attribution = state.report = null;
  state.runs = [];
  dash.logs.clear();
  dash.gpus.clear();
  const current = document.querySelector("[role=tab][aria-selected=true]")?.dataset.tab ?? "contract";
  selectTab(current);
  if (!state.campaign) return markFunnel();
  const [spec, contract] = await Promise.all([call("spec", { campaign: state.campaign }), call("contract", { campaign: state.campaign })]);
  state.spec = spec;
  state.contract = contract;
  renderContract();
  if (contract.locked) await refreshResults();
  markFunnel();
}

function renderContract() {
  const { spec, contract } = state;
  $("#c-repo").textContent = spec.repo;
  $("#c-yaml").textContent = spec.text || "(no experiment.yaml)";
  $("#c-problem").textContent = spec.problem ?? "";
  $("#c-problem").hidden = !spec.problem;
  const shown = contract.locked ? contract.spec : spec.spec;
  const rows = shown ? [
    ["Goal", shown.title || "–"],
    ["Metric", `${shown.metric} (${shown.direction === "min" ? "lower" : "higher"} is better)`],
    ["Command", shown.command],
    ["Seeds", shown.seeds.join(", ")],
    ["Baseline", shown.baseline],
    ["Candidates", shown.candidates.join(", ") || "–"],
    ["Time limit", `${shown.timeout_s} s per run`],
  ] : [];
  $("#c-spec").replaceChildren(...rows.flatMap(([key, value]) => [element("dt", { textContent: key }), element("dd", { textContent: value, className: key === "Command" ? "mono" : "" })]));
  const badge = $("#c-state");
  badge.textContent = contract.locked ? "Locked" : "Not locked";
  badge.className = `badge ${contract.locked ? "ok" : "warn"}`;
  const files = contract.locked ? Object.entries(contract.files) : (spec.spec?.protected ?? []).map((path) => [path, null]);
  const drift = new Set(contract.working_tree_drift ?? []);
  $("#c-files tbody").replaceChildren(...files.map(([path, digest]) => element("tr", {},
    element("td", { className: "mono", textContent: path }),
    element("td", { className: "mono muted", textContent: digest ? digest.slice(0, 16) : "not pinned yet" }),
    element("td", {}, digest ? element("span", { className: `badge ${drift.has(path) ? "warn" : "ok"}`, textContent: drift.has(path) ? "edited, not committed" : "matches" }) : ""),
  )));
  $("#lock").textContent = contract.locked ? "Re-lock (starts a new comparison)" : "Lock contract";
  $("#lock").classList.toggle("primary", !contract.locked);
  $("#c-seal").textContent = contract.locked ? `Seal ${contract.seal} at ${contract.baseline_ref} (${contract.baseline_sha.slice(0, 8)}), ${new Date(contract.locked_at * 1000).toLocaleString()}` : "";
  if (!$("#i-refs").value && shown) $("#i-refs").placeholder = shown.candidates.join(", ") || "branch names";
}

async function refreshResults() {
  const [board, runs] = await Promise.all([call("leaderboard", { campaign: state.campaign }), call("runs", { campaign: state.campaign })]);
  state.board = board;
  state.runs = runs.runs;
  renderBoard();
  renderRuns();
  if (!$("#dashboard").hidden) await refreshDashboard();
  await refreshAttribution();
  markFunnel();
}

function renderBoard() {
  const { board } = state;
  $("#l-metric").textContent = `${board.metric}, ${board.direction === "min" ? "lower" : "higher"} is better`;
  $("#board-empty").hidden = board.rows.length > 0;
  $("#board tbody").replaceChildren(...board.rows.map((row) => {
    const delta = row.kind === "baseline" ? "baseline" : row.delta === null ? "–" : `${row.delta > 0 ? "+" : ""}${num(row.delta)}`;
    const contract = row.status === "invalid"
      ? element("span", { className: "badge bad", textContent: `invalid: changed ${row.violations.join(", ")}`, title: "This branch modified protected evaluation files, so its score is not counted." })
      : element("span", { className: `badge ${row.status === "ok" ? "ok" : "warn"}`, textContent: row.status === "ok" ? "valid" : row.status });
    return element("tr", { className: row.status === "invalid" ? "invalid" : row.rank === 1 && row.kind === "candidate" && row.better ? "best" : "" },
      element("td", { textContent: row.rank ?? "–" }),
      element("td", { className: "code-cell", textContent: row.ref }),
      element("td", { className: "mono muted", textContent: row.sha.slice(0, 8) }),
      element("td", { textContent: meanStd(row) }),
      element("td", { textContent: delta + (row.better === true ? " ✓" : row.better === false ? " ✗" : "") }),
      element("td", { textContent: `${row.n}/${row.seeds}` }),
      element("td", {}, contract),
    );
  }));
}

function renderRuns() {
  $("#runs tbody").replaceChildren(...[...state.runs].reverse().map((run) => {
    const row = element("tr", { className: state.selectedRun === run.id ? "selected" : "" },
      element("td", { textContent: run.id }),
      element("td", { textContent: run.kind }),
      element("td", { textContent: run.label ? `${run.ref} without ${run.label}` : run.ref }),
      element("td", { textContent: run.seed }),
      element("td", {}, element("span", { className: `badge ${run.status === "ok" ? "ok" : run.status === "invalid" ? "bad" : "warn"}`, textContent: run.status })),
      element("td", { textContent: num(run.metric) }),
      element("td", { className: "muted", textContent: `${num(run.duration_s)} s` }),
      element("td", {}, element("button", { type: "button", className: "mini", textContent: "Watch", title: "Follow this run's log in a terminal tab", onclick: guard(async (event) => {
        event.stopPropagation();
        const { tabs } = await call("watch", { campaign: state.campaign, run: run.id });
        for (const tab of tabs) await openTerminal(tab);
      }) })),
    );
    row.onclick = guard(() => showLog(run.id));
    return row;
  }));
}

async function showLog(runId) {
  state.selectedRun = runId;
  renderRuns();
  const { run, log, truncated } = await call("log", { campaign: state.campaign, run: runId });
  $("#log-title").textContent = `Log of run ${run.id}: ${run.ref}${run.label ? ` without ${run.label}` : ""}, seed ${run.seed}`;
  $("#log").textContent = (truncated ? "…\n" : "") + (log || "(empty)");
}

async function refreshAttribution() {
  const winners = (state.board?.rows ?? []).filter((row) => row.kind === "candidate" && row.status === "ok");
  const picker = $("#a-target");
  const previous = picker.value;
  picker.replaceChildren(...winners.map((row) => element("option", { value: row.ref, textContent: `${row.ref} (${meanStd(row)})${row.better ? "" : ", not better"}` })));
  if (winners.some((row) => row.ref === previous)) picker.value = previous;
  $("#run-ablation").disabled = !winners.length;
  state.attribution = null;
  if (picker.value) {
    try { state.attribution = await call("attribution", { campaign: state.campaign, target: picker.value }); } catch { state.attribution = null; }
  }
  renderAttribution();
}

function renderAttribution() {
  const result = state.attribution;
  const files = result?.files ?? [];
  $("#a-empty").hidden = files.length > 0;
  $("#a-empty").textContent = state.board?.rows?.some((row) => row.kind === "candidate" && row.status === "ok") ? "No ablation yet. Run ablation reverts each changed file in turn." : "Run a sweep first; the ablation explains a candidate that has results.";
  $("#a-lift").textContent = result?.lift !== null && result?.lift !== undefined ? `Gain of ${result.target_ref} over the baseline: ${num(result.lift)} ${result.metric}` : "";
  $("#attribution tbody").replaceChildren(...files.map((row) => {
    const share = row.share === null ? null : Math.max(0, Math.min(1, row.share));
    return element("tr", {},
      element("td", { className: "mono", textContent: row.file }),
      element("td", { textContent: `${meanStd(row)} (n=${row.n})` }),
      element("td", { textContent: num(row.lift_lost) }),
      element("td", {}, element("div", { className: "share" }, element("div", { className: "track" }, element("div", { className: "bar-fill", style: `width:${(share ?? 0) * 100}%` })), element("span", { textContent: row.share === null ? "–" : `${Math.round(row.share * 100)}%` }))),
      element("td", { textContent: row.significant === true ? "yes (> 2σ)" : row.significant === false ? "no" : "needs more seeds" }),
    );
  }));
  markFunnel();
}

function renderActive() {
  const active = state.active ?? [];
  $("#active-card").hidden = !active.length;
  $("#active tbody").replaceChildren(...active.map((entry) => element("tr", {},
    element("td", { textContent: entry.label ? `${entry.ref} without ${entry.label}` : entry.ref }),
    element("td", { textContent: entry.seed }),
    element("td", { textContent: entry.gpu ?? "–" }),
    element("td", { className: "muted", textContent: elapsed(entry.started_at) }),
    element("td", {}, element("button", { type: "button", className: "mini", textContent: "Watch", onclick: guard(async () => {
      const { tabs } = await call("watch", { campaign: state.campaign, job: state.job });
      const tab = tabs.find((candidate) => candidate.command.endsWith(entry.log.split("/").pop()) || candidate.command.includes(entry.log.split("/").pop()));
      if (!tab) throw new Error("That run has just finished.");
      await openTerminal(tab);
    }) })),
  )));
}

async function followJob(status, meter) {
  state.job = status.id;
  const box = $(meter);
  box.hidden = false;
  let seen = -1;
  while (!["done", "failed", "cancelled"].includes(status.state)) {
    box.querySelector(".meter span").style.width = `${Math.round((status.progress ?? 0) * 100)}%`;
    box.querySelector("p").textContent = status.message ?? "";
    state.active = status.active ?? [];
    renderActive();
    if ((status.runs?.length ?? 0) !== seen) {
      seen = status.runs?.length ?? 0;
      if (seen) await refreshResults();
    }
    await new Promise((resolve) => setTimeout(resolve, 900));
    status = await call("status", { job: status.id });
  }
  state.job = null;
  state.active = [];
  renderActive();
  box.hidden = true;
  await refreshResults();
  if (status.state === "failed") throw new Error(status.message || "The job failed.");
  toast(status.message);
  return status;
}

async function runJob(endpoint, params, meter) {
  return followJob(await call(endpoint, { campaign: campaign(), ...params }), meter);
}

function parallelParams() {
  const params = {};
  if ($("#i-parallel").value.trim()) params.parallel = $("#i-parallel").value.trim();
  if ($("#i-gpus").value.trim()) params.gpus = $("#i-gpus").value.trim();
  return params;
}

async function resumeJob() {
  if (!state.campaign || state.job) return;
  const { jobs } = await call("jobs");
  const running = jobs.find((job) => ["running", "queued"].includes(job.state) && job.params?.campaign === state.campaign && ["sweep", "ablate"].includes(job.kind));
  if (!running) return;
  const status = await call("status", { job: running.id });
  if (running.kind === "ablate") selectTab("ablate");
  else selectTab("investigate");
  await followJob(status, running.kind === "ablate" ? "#a-progress" : "#progress");
}

$("#watch-all").onclick = guard(async () => {
  if (!state.job) throw new Error("Nothing is running.");
  const { tabs } = await call("watch", { campaign: state.campaign, job: state.job });
  if (!tabs.length) throw new Error("Nothing is running right now.");
  for (const tab of tabs) await openTerminal(tab);
});

function lockedOrThrow() {
  if (!state.contract?.locked) throw new Error("Lock the evaluation contract first (step 1), so every run is judged by the same pinned evaluation.");
}

$("#sweep").onclick = guard(async () => {
  lockedOrThrow();
  const params = {};
  if ($("#i-refs").value.trim()) params.refs = $("#i-refs").value.trim();
  if ($("#i-seeds").value.trim()) params.seeds = $("#i-seeds").value.trim();
  if ($("#i-rerun").checked) params.rerun = "true";
  Object.assign(params, parallelParams());
  $("#sweep").disabled = true;
  try { await runJob("sweep", params, "#progress"); } finally { $("#sweep").disabled = false; }
});

$("#cancel").onclick = guard(async () => { if (state.job) await call("cancel", { job: state.job }); });

$("#run-ablation").onclick = guard(async () => {
  lockedOrThrow();
  const params = { target: $("#a-target").value, ...parallelParams() };
  if ($("#a-seeds").value.trim()) params.seeds = $("#a-seeds").value.trim();
  $("#run-ablation").disabled = true;
  try { await runJob("ablate", params, "#a-progress"); } finally { $("#run-ablation").disabled = false; }
});

$("#lock").onclick = guard(async () => {
  const relock = state.contract?.locked;
  const result = await call("lock", { campaign: campaign(), force: relock ? "true" : "" });
  toast(`Contract locked, seal ${result.seal}`);
  await refreshCampaign();
});

$("#open-folder").onclick = guard(async () => {
  const folder = await zorai("pickFolder", { title: "Repository with experiment.yaml" });
  if (!folder) return;
  const name = folder.split("/").filter(Boolean).pop().toLowerCase().replace(/[^a-z0-9_-]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 64) || "campaign";
  const opened = await call("open", { campaign: name, folder });
  toast(`Opened ${opened.name}`);
  await loadCampaigns(opened.name);
  selectTab("contract");
});

$("#example").onclick = guard(async () => {
  $("#example").disabled = true;
  try {
    const built = await call("example", {});
    const name = built.campaign.name;
    const status = await call("contract", { campaign: name });
    if (!status.locked) await call("lock", { campaign: name });
    toast(built.created ? `Built the toy campaign in ${built.folder}` : "Opened the toy campaign");
    await loadCampaigns(name);
    selectTab("investigate");
  } finally { $("#example").disabled = false; }
});

$("#campaign").onchange = guard(async () => {
  state.campaign = $("#campaign").value;
  saved.setItem("ml-research:campaign", state.campaign);
  await refreshCampaign();
});

$("#a-target").onchange = guard(refreshAttribution);

function inline(text) {
  const escaped = text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
  return escaped
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<b>$1</b>")
    .replace(/\[((?:run \d+(?:, )?)+)\]/g, '<span class="cite">[$1]</span>')
    .replace(/⟦([^⟧]+)⟧/g, '<mark class="ungrounded" title="Not in evidence.json">$1</mark>');
}

function renderMarkdown(markdown, base) {
  const html = [];
  const lines = markdown.split("\n");
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (line.startsWith("```")) {
      const code = [];
      for (index += 1; index < lines.length && !lines[index].startsWith("```"); index += 1) code.push(lines[index]);
      html.push(`<pre class="code"><code>${inline(code.join("\n")).replace(/<\/?code>/g, "")}</code></pre>`);
    } else if (line.startsWith("|")) {
      const rows = [];
      for (; index < lines.length && lines[index].startsWith("|"); index += 1) rows.push(lines[index]);
      index -= 1;
      const cells = (row) => row.slice(1, -1).split(" | ").map((cell) => cell.trim());
      html.push(`<table><thead><tr>${cells(rows[0]).map((cell) => `<th>${inline(cell)}</th>`).join("")}</tr></thead><tbody>${rows.slice(2).map((row) => `<tr>${cells(row).map((cell) => `<td>${inline(cell)}</td>`).join("")}</tr>`).join("")}</tbody></table>`);
    } else if (/^!\[[^\]]*\]\(([^)]+)\)$/.test(line)) {
      const [, alt, src] = line.match(/^!\[([^\]]*)\]\(([^)]+)\)$/);
      html.push(`<img alt="${inline(alt)}" src="${media(`${base}/${src}`)}?v=${Date.now()}">`);
    } else if (/^#{1,3} /.test(line)) {
      const level = line.match(/^#+/)[0].length;
      html.push(`<h${level}>${inline(line.slice(level + 1))}</h${level}>`);
    } else if (/^\s*- /.test(line)) {
      const items = [];
      for (; index < lines.length && /^\s*- /.test(lines[index]); index += 1) items.push(`<li${/^\s+- /.test(lines[index]) ? ' class="nested"' : ""}>${inline(lines[index].replace(/^\s*- /, ""))}</li>`);
      index -= 1;
      html.push(`<ul>${items.join("")}</ul>`);
    } else if (/^_.+_$/.test(line.trim())) {
      html.push(`<p class="placeholder">${inline(line.trim().slice(1, -1))}</p>`);
    } else if (line.trim()) {
      html.push(`<p>${inline(line)}</p>`);
    }
  }
  return html.join("");
}

$("#build-report").onclick = guard(async () => {
  lockedOrThrow();
  $("#build-report").disabled = true;
  try {
    state.report = await call("report", { campaign: campaign() });
    const base = state.report.file.split("/").slice(0, -1).join("/");
    $("#r-body").innerHTML = renderMarkdown(state.report.markdown, base);
    $("#r-path").textContent = state.report.path;
    markFunnel();
  } finally { $("#build-report").disabled = false; }
});

function renderPaper(result) {
  state.paper = { ...state.paper, ...result };
  const base = state.paper.manuscript_file ? state.paper.manuscript_file.split("/").slice(0, -1).join("/") : "";
  $("#p-path").textContent = state.paper.manuscript ?? "";
  const badge = $("#p-state");
  if (result.flagged) {
    badge.textContent = result.ok ? `All ${result.checked} numbers grounded` : `${result.flagged.length} sentence${result.flagged.length === 1 ? "" : "s"} with invented numbers`;
    badge.className = `badge ${result.ok ? "ok" : "bad"}`;
    $("#p-flagged").replaceChildren(...result.flagged.map((item) => element("li", {}, element("b", { textContent: `Line ${item.line}: ` }), element("span", { className: "bad-text", textContent: item.numbers.join(", ") }), ` in “${item.text.slice(0, 220)}”`)));
    $("#p-body").innerHTML = renderMarkdown(result.marked, base);
  }
}

$("#build-paper").onclick = guard(async () => {
  lockedOrThrow();
  const built = await call("paper", { campaign: campaign() });
  state.paper = built;
  toast(built.created ? "Paper workspace ready" : "Evidence refreshed; your manuscript was kept");
  renderPaper(await call("audit", { campaign: campaign() }));
  markFunnel();
});

$("#audit-paper").onclick = guard(async () => {
  renderPaper(await call("audit", { campaign: campaign() }));
});

$("#write-paper").onclick = guard(async () => {
  lockedOrThrow();
  if (!state.paper?.manuscript) state.paper = await call("paper", { campaign: campaign() });
  await ask(`Write the paper for ML Research Lab campaign "${state.campaign}".\n\n1. Read ${state.paper.evidence}. It holds every number you may use, each with the run ids behind it.\n2. Edit ${state.paper.manuscript}: replace every "_Write ..._" line with prose (Abstract, Introduction, Method, Experimental setup, Results, Ablations, Limitations, Reproducibility). Use only numbers from evidence.json, cite runs like [run 3, run 4], keep the generated tables, and state limitations honestly (few seeds, effects under two standard deviations, invalid runs).\n3. Run /ml-research.audit --campaign ${state.campaign} and fix every flagged sentence. Never invent a number to make it pass.`);
});

$("#review-paper").onclick = guard(async () => {
  if (!state.paper?.manuscript) throw new Error("Build the paper workspace first.");
  await ask(`Review ${state.paper.manuscript} like a NeurIPS reviewer: soundness, whether every claim is backed by ${state.paper.evidence} and the cited runs, missing baselines or ablations, seed variance, and clarity. Give a score and the changes that would raise it.`);
});

async function loadRuntimes() {
  $("#rt-workspace").textContent = state.workspace || "none chosen";
  const found = await call("runtimes", state.workspace ? { workspace: state.workspace } : {});
  const labels = { ready: "Ready", "needs-setup": "Needs setup", missing: "Missing" };
  $("#rt-cards").replaceChildren(...found.profiles.map((profile) => {
    const launch = (mode, prompt) => guard(async () => openTerminal(await call("launch", { runtime: profile.id, mode: mode.id, prompt, workspace: state.workspace })));
    const modes = profile.modes.map((mode) => {
      if (!mode.requires_prompt) return element("button", { type: "button", className: mode.recommended ? "primary" : "", textContent: mode.label, title: mode.description, onclick: launch(mode) });
      const field = element("input", { placeholder: mode.prompt_placeholder ?? "", size: 18, "aria-label": `${mode.label} value` });
      return element("span", { className: "prompt-mode" }, field, element("button", { type: "button", textContent: mode.label, title: mode.description, onclick: (event) => launch(mode, field.value)(event) }));
    });
    const setup = profile.setup_command ? element("div", { className: "setup" },
      element("code", { textContent: profile.setup_command.command }),
      element("button", { type: "button", textContent: profile.readiness === "needs-setup" ? "Run setup" : "Install", onclick: guard(() => openTerminal(profile.setup_command)) })) : null;
    return element("section", { className: "card runtime" },
      element("header", {}, element("h2", { textContent: profile.label }), element("span", { className: `badge ${profile.readiness === "ready" ? "ok" : profile.readiness === "missing" ? "bad" : "warn"}`, textContent: labels[profile.readiness] })),
      element("p", { className: "small", textContent: profile.description }),
      element("p", { className: "muted small", textContent: `${profile.version ? `${profile.version}. ` : ""}${profile.notes.join(" ")}` }),
      element("ul", { className: "checks" }, ...profile.checks.map((check) => element("li", { className: check.exists ? "ok-text" : "muted", textContent: `${check.exists ? "✓" : "✗"} ${check.label}` }))),
      setup,
      element("div", { className: "modes" }, ...modes),
      element("a", { href: profile.homepage, target: "_blank", rel: "noreferrer", className: "small", textContent: "Project page" }),
    );
  }));
}

$("#rt-pick").onclick = guard(async () => {
  const folder = await zorai("pickFolder", { title: "Training workspace" });
  if (!folder) return;
  state.workspace = folder;
  saved.setItem("ml-research:workspace", folder);
  await loadRuntimes();
});

$("#rt-repo").onclick = guard(async () => {
  if (!state.spec?.repo) throw new Error("Open a campaign first.");
  state.workspace = state.spec.repo;
  saved.setItem("ml-research:workspace", state.workspace);
  await loadRuntimes();
});

$("#rt-refresh").onclick = guard(loadRuntimes);

$("#report-to-message").onclick = guard(async () => {
  if (!state.report) throw new Error("Write the report first.");
  await zorai("insertText", { text: `ML Research Lab report for ${state.campaign}: ${state.report.path}` });
});

function boardText() {
  if (!state.board?.rows?.length) return "No runs yet.";
  return state.board.rows.map((row) => `- ${row.ref} (${row.sha.slice(0, 8)}, ${row.kind}): ${meanStd(row)} over ${row.n}/${row.seeds} seeds, ${row.status}${row.violations.length ? ` (changed ${row.violations.join(", ")})` : ""}, runs ${row.run_ids.join(", ")}`).join("\n");
}

function ask(text) {
  return zorai("ask", { text });
}

$("#ask-results").onclick = guard(() => {
  const spec = state.contract?.spec ?? state.spec?.spec;
  return ask(`ML Research Lab campaign "${state.campaign}" in ${state.spec?.repo}. Metric ${spec?.metric} (${spec?.direction === "min" ? "lower" : "higher"} is better).\n\nLeaderboard:\n${boardText()}\n\nWhat do these results tell us, are the differences larger than the seed noise, and which idea should we try next? Use the ml-research plugin to run anything new; never edit the protected files (${Object.keys(state.contract?.files ?? {}).join(", ")}).`);
});

$("#ask-ablation").onclick = guard(() => {
  const result = state.attribution;
  if (!result?.files?.length) throw new Error("Run an ablation first.");
  const rows = result.files.map((row) => `- without ${row.file}: ${meanStd(row)}, lift lost ${num(row.lift_lost)} (${row.share === null ? "–" : Math.round(row.share * 100) + "%"}), clear effect: ${row.significant}`).join("\n");
  return ask(`Leave-one-out ablation of ${result.target_ref} in ML Research Lab campaign "${state.campaign}" (${result.metric}, gain over baseline ${num(result.lift)}):\n${rows}\n\nExplain why the change works, which parts are dead weight, and whether we need more seeds.`);
});

$("#ask-review").onclick = guard(() => {
  if (!state.report) throw new Error("Write the report first.");
  return ask(`Review this research report like a NeurIPS reviewer: soundness, whether every claim is backed by the cited runs, missing baselines or ablations, and seed variance. Report file: ${state.report.path}\n\n${state.report.markdown}`);
});

$("#run-sql").onclick = guard(async () => {
  $("#run-sql").disabled = true;
  try {
    const result = await call("sql", { query: $("#sql").value, limit: Number($("#sql-limit").value) || 200, ...(state.campaign ? { campaign: state.campaign } : {}) });
    $("#sql-result thead").replaceChildren(element("tr", {}, ...result.columns.map((column) => element("th", { textContent: column }))));
    $("#sql-result tbody").replaceChildren(...result.rows.map((row) => element("tr", {}, ...row.map((value) => element("td", { textContent: value === null ? "NULL" : typeof value === "object" ? JSON.stringify(value) : typeof value === "number" ? num(value) : String(value) })))));
    $("#sql-note").textContent = `${result.rows.length} row${result.rows.length === 1 ? "" : "s"}${result.truncated ? `, stopped at the limit of ${result.limit}` : ""}.`;
    state.lastSql = result;
  } finally { $("#run-sql").disabled = false; }
});

$("#ask-data").onclick = guard(() => {
  const result = state.lastSql;
  const preview = result ? [result.columns.join(" | "), ...result.rows.slice(0, 20).map((row) => row.map((value) => JSON.stringify(value)).join(" | "))].join("\n") : "(not run yet)";
  return ask(`I queried data with DuckDB in ML Research Lab${state.campaign ? ` (campaign ${state.campaign}, repository ${state.spec?.repo})` : ""}:\n\n${$("#sql").value}\n\nFirst rows:\n${preview}\n\nWhat stands out, and is there contamination or leakage between splits worth checking? Use /ml-research.data-sql for follow-up queries.`);
});

async function check() {
  const report = await call("check", state.campaign ? { campaign: state.campaign } : {});
  const chip = $("#ready");
  chip.textContent = report.ready ? (report.gpus.length ? `Ready · ${report.gpus[0].name.replace("NVIDIA GeForce ", "")}, ${(report.gpus[0].free_mib / 1024).toFixed(1)} GB free` : "Ready · CPU only") : "Needs setup";
  chip.className = `chip ${report.ready ? "ok" : "warn"}`;
  $("#checks tbody").replaceChildren(...report.items.map((item) => element("tr", {},
    element("td", { textContent: item.ok ? "✅" : item.required ? "❌" : "⚠️" }),
    element("td", { textContent: item.name }),
    element("td", { className: "muted", textContent: item.detail }),
  )));
}

$("#recheck").onclick = guard(check);

const SVG = "http://www.w3.org/2000/svg";
const dash = { logs: new Map(), gpus: new Map(), busy: false, hover: null };

function svg(tag, attrs = {}, ...children) {
  const node = document.createElementNS(SVG, tag);
  for (const [key, value] of Object.entries(attrs)) if (value !== undefined && value !== null) node.setAttribute(key, value);
  node.append(...children);
  return node;
}

function dashSources() {
  const live = (state.active ?? []).map((entry) => ({ path: entry.log, ref: entry.ref, label: entry.label, seed: entry.seed, gpu: entry.gpu, started_at: entry.started_at, kind: entry.kind, status: "running", live: true }));
  const livePaths = new Set(live.map((entry) => entry.path));
  const done = state.runs.filter((run) => run.log && !livePaths.has(run.log)).map((run) => ({ path: run.log, ref: run.ref, label: run.label, seed: run.seed, kind: run.kind, status: run.status, id: run.id, live: false }));
  return [...live, ...done];
}

async function refreshDashboard() {
  if (dash.busy || !state.campaign || $("#dashboard").hidden) return;
  const sources = dashSources();
  const wanted = sources.filter((source) => {
    const known = dash.logs.get(source.path);
    return !known || known.more || source.live || known.live;
  });
  dash.busy = true;
  try {
    if (wanted.length || state.active?.length) {
      const result = await call("series", { campaign: state.campaign, logs: wanted.map((source) => ({ path: source.path, offset: dash.logs.get(source.path)?.offset ?? 0, last: dash.logs.get(source.path)?.last ?? 0, explicit: dash.logs.get(source.path)?.explicit ?? false })), gpus: state.active?.length ? "true" : "" });
      for (const part of result.logs) {
        const known = dash.logs.get(part.path) ?? { series: {}, progress: null };
        for (const [key, points] of Object.entries(part.series)) (known.series[key] ??= []).push(...points);
        dash.logs.set(part.path, { ...known, offset: part.offset, last: part.last, explicit: part.explicit, more: part.more, progress: part.progress ?? known.progress, live: sources.find((source) => source.path === part.path)?.live });
      }
      const now = Date.now();
      for (const [index, gpu] of (result.gpus ?? []).entries()) {
        const history = dash.gpus.get(index) ?? { name: gpu.name, points: [] };
        history.points.push([now, (gpu.total_mib - gpu.free_mib) / gpu.total_mib, gpu.utilization / 100, gpu]);
        if (history.points.length > 1200) history.points.shift();
        dash.gpus.set(index, history);
      }
      $("#d-updated").textContent = `Updated ${new Date(now).toLocaleTimeString()}`;
    }
  } finally {
    dash.busy = false;
  }
  renderDashboard(sources);
}

function identity(source) {
  return source.label ? `${source.ref} without ${source.label}` : source.ref;
}

function dashColors(sources) {
  const spec = state.spec?.spec;
  const order = [...new Set([...(spec?.candidates ?? []), ...sources.map(identity)])].filter((name) => name !== spec?.baseline);
  return (name) => {
    if (name === spec?.baseline) return { color: "var(--text-muted, #a89f8c)", dashed: true };
    const slot = order.indexOf(name);
    return slot >= 0 && slot < 8 ? { color: `var(--s${slot + 1})`, dashed: false } : { color: "var(--text-muted, #a89f8c)", dashed: false, other: true };
  };
}

function smooth(points) {
  let value = null;
  return points.map(([step, raw]) => [step, value = value === null ? raw : value * 0.85 + raw * 0.15]);
}

function thin(points, limit) {
  if (points.length <= limit) return points;
  const stride = Math.ceil(points.length / limit);
  return points.filter((_, index) => index % stride === 0 || index === points.length - 1);
}

function ticks(min, max, count) {
  if (min === max) return [min];
  const raw = (max - min) / count;
  const power = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 2.5, 5, 10].map((factor) => factor * power).find((candidate) => candidate >= raw);
  const out = [];
  for (let value = Math.ceil(min / step) * step; value <= max + step * 1e-9; value += step) out.push(Number(value.toPrecision(12)));
  return out;
}

function nearest(points, step) {
  let low = 0, high = points.length - 1;
  while (low < high) {
    const middle = (low + high) >> 1;
    if (points[middle][0] < step) low = middle + 1; else high = middle;
  }
  const before = points[Math.max(0, low - 1)];
  return Math.abs(before[0] - step) < Math.abs(points[low][0] - step) ? before : points[low];
}

function renderDashboard(sources = dashSources()) {
  const keys = new Set();
  for (const source of sources) for (const key of Object.keys(dash.logs.get(source.path)?.series ?? {})) keys.add(key);
  const picker = $("#d-key");
  const metric = state.spec?.spec?.metric;
  const chosen = keys.has(picker.value) ? picker.value : keys.has(metric) ? metric : keys.has("loss") ? "loss" : [...keys][0] ?? "";
  picker.replaceChildren(...[...keys].sort((a, b) => (b === metric) - (a === metric) || a.localeCompare(b)).map((key) => element("option", { value: key, textContent: key })));
  picker.value = chosen;
  const colorOf = dashColors(sources);
  const shown = sources.filter((source) => $("#d-invalid").checked || source.status !== "invalid");
  const lines = shown.map((source) => {
    const raw = dash.logs.get(source.path)?.series?.[chosen] ?? [];
    return { source, name: identity(source), style: colorOf(identity(source)), raw, points: $("#d-smooth").checked ? smooth(raw) : raw };
  }).filter((line) => line.points.length);
  renderRunCards(sources, chosen, colorOf);
  renderDashTable(sources, chosen);
  renderGpus();
  $("#d-title").textContent = chosen ? `${chosen} by step` : "Curves";
  $("#d-empty").hidden = lines.some((line) => line.points.length > 1);
  $("#d-empty-runs").hidden = sources.length > 0;
  $("#d-empty-curves").hidden = sources.length === 0;
  const logScale = $("#d-log").checked && lines.every((line) => line.points.every(([, value]) => value > 0));
  $("#d-log").disabled = !lines.every((line) => line.points.every(([, value]) => value > 0));
  const names = [...new Set(lines.map((line) => line.name))];
  $("#d-legend").replaceChildren(...names.map((name) => {
    const style = colorOf(name);
    return element("span", {}, element("i", { className: `swatch${style.dashed ? " dashed" : ""}`, style: `border-color: ${style.color}` }), style.other ? `${name} (other)` : name);
  }));
  drawChart(lines, logScale, names.length);
}

function drawChart(lines, logScale, identities) {
  const box = $("#d-chart");
  box.replaceChildren();
  if (!lines.length) return;
  const width = Math.max(320, box.clientWidth || 720), height = 300;
  const pad = { left: 56, right: identities <= 4 ? 120 : 16, top: 12, bottom: 28 };
  const y = (value) => logScale ? Math.log10(value) : value;
  const all = lines.flatMap((line) => line.points);
  let [x0, x1] = [Math.min(...all.map(([step]) => step)), Math.max(...all.map(([step]) => step))];
  let [y0, y1] = [Math.min(...all.map(([, value]) => y(value))), Math.max(...all.map(([, value]) => y(value)))];
  if (x0 === x1) [x0, x1] = [x0 - 1, x1 + 1];
  if (y0 === y1) [y0, y1] = [y0 - Math.abs(y0 || 1) * 0.1, y1 + Math.abs(y1 || 1) * 0.1];
  const spread = (y1 - y0) * 0.06;
  [y0, y1] = [y0 - spread, y1 + spread];
  const sx = (step) => pad.left + (step - x0) / (x1 - x0) * (width - pad.left - pad.right);
  const sy = (value) => pad.top + (1 - (y(value) - y0) / (y1 - y0)) * (height - pad.top - pad.bottom);
  const plot = svg("svg", { viewBox: `0 0 ${width} ${height}`, height, role: "img", "aria-label": `${$("#d-key").value} by step for ${lines.length} runs` });
  const grid = svg("g", { class: "grid" }), axis = svg("g", { class: "axis" });
  const yTicks = logScale ? ticks(Math.floor(y0), Math.ceil(y1), 4).filter((value) => value >= y0 && value <= y1).map((value) => 10 ** value) : ticks(y0, y1, 4);
  for (const value of yTicks) {
    grid.append(svg("line", { x1: pad.left, x2: width - pad.right, y1: sy(value), y2: sy(value) }));
    axis.append(svg("text", { x: pad.left - 8, y: sy(value) + 4, "text-anchor": "end" }, num(value)));
  }
  for (const value of ticks(x0, x1, 6).filter((value) => value >= x0 && value <= x1)) axis.append(svg("text", { x: sx(value), y: height - 8, "text-anchor": "middle" }, num(value)));
  plot.append(grid, axis);
  const limit = Math.round(width * 1.5);
  for (const line of lines) {
    const points = thin(line.points, limit);
    const d = points.map(([step, value], index) => `${index ? "L" : "M"}${sx(step).toFixed(1)},${sy(value).toFixed(1)}`).join("");
    plot.append(svg("path", { class: "curve", d, stroke: line.style.color, "stroke-dasharray": line.style.dashed ? "6 4" : line.source.status === "invalid" ? "2 3" : undefined, "stroke-opacity": line.source.live ? 1 : 0.7 }));
    const [step, value] = line.points.at(-1);
    if (points.length === 1 || line.source.live) plot.append(svg("circle", { class: line.source.live ? "live" : "", cx: sx(step), cy: sy(value), r: 4.5, fill: line.style.color }));
  }
  if (identities <= 4) {
    const ends = new Map();
    for (const line of lines) {
      const end = line.points.at(-1);
      const previous = ends.get(line.name);
      if (!previous || end[0] > previous.step) ends.set(line.name, { step: end[0], y: sy(end[1]) });
    }
    const placed = [...ends.entries()].sort((a, b) => a[1].y - b[1].y);
    let floor = -Infinity;
    for (const [name, end] of placed) {
      const at = Math.max(end.y + 4, floor + 13);
      floor = at;
      plot.append(svg("text", { class: "end-label", x: width - pad.right + 8, y: at }, name.length > 18 ? `${name.slice(0, 17)}…` : name));
    }
  }
  const cross = svg("line", { class: "cross", y1: pad.top, y2: height - pad.bottom, visibility: "hidden" });
  const hit = svg("rect", { x: pad.left, y: 0, width: width - pad.left - pad.right, height, fill: "transparent" });
  const tip = element("div", { className: "tip", hidden: true });
  hit.addEventListener("pointermove", (event) => {
    const rect = plot.getBoundingClientRect();
    const px = (event.clientX - rect.left) * width / rect.width;
    const step = x0 + (px - pad.left) / (width - pad.left - pad.right) * (x1 - x0);
    const at = lines.map((line) => nearest(line.points, step)[0]).reduce((best, candidate) => Math.abs(candidate - step) < Math.abs(best - step) ? candidate : best);
    const rows = lines.map((line) => ({ line, point: nearest(line.points, at) })).filter(({ point }) => Math.abs(sx(point[0]) - sx(at)) < 24);
    cross.setAttribute("x1", sx(at)); cross.setAttribute("x2", sx(at)); cross.setAttribute("visibility", "visible");
    tip.replaceChildren(element("b", { textContent: `step ${num(at)}` }), ...rows.slice(0, 12).map(({ line, point }) => element("div", {},
      element("i", { className: `swatch${line.style.dashed ? " dashed" : ""}`, style: `border-color: ${line.style.color}` }),
      element("span", { textContent: `${line.name} · seed ${line.source.seed}${line.source.live ? " · live" : ""}` }),
      element("span", { textContent: num(point[1]) }),
    )));
    tip.hidden = false;
    const left = (sx(at) / width) * rect.width;
    tip.style.left = `${Math.min(left + 12, rect.width - tip.offsetWidth - 4)}px`;
    tip.style.top = `${pad.top}px`;
  });
  hit.addEventListener("pointerleave", () => { cross.setAttribute("visibility", "hidden"); tip.hidden = true; });
  plot.append(cross, hit);
  box.append(plot, tip);
}

function renderRunCards(sources, key, colorOf) {
  $("#d-running").replaceChildren(...sources.filter((source) => source.live).map((source) => {
    const known = dash.logs.get(source.path);
    const points = known?.series?.[key] ?? [];
    const progress = known?.progress;
    const style = colorOf(identity(source));
    const share = progress?.total ? progress.done / progress.total : null;
    return element("article", { className: "run-card" },
      element("header", {}, element("i", { className: `swatch${style.dashed ? " dashed" : ""}`, style: `border-color: ${style.color}` }), `${identity(source)} · seed ${source.seed}`),
      element("span", { className: "muted small", textContent: `${source.gpu !== null && source.gpu !== undefined ? `GPU ${source.gpu} · ` : ""}running for ${elapsed(source.started_at)}` }),
      element("span", { className: "value", textContent: points.length ? num(points.at(-1)[1]) : "–", title: key }),
      element("span", { className: "muted small", textContent: points.length ? `${key} at step ${num(points.at(-1)[0])}` : `no ${key || "progress"} printed yet` }),
      share === null ? "" : element("div", { className: "meter" }, element("span", { style: `width: ${Math.round(share * 100)}%` })),
      share === null ? "" : element("span", { className: "muted small", textContent: `${progress.done} / ${progress.total}${progress.remaining ? ` · ${progress.remaining} left` : ""}` }),
    );
  }));
}

function renderDashTable(sources, key) {
  const lower = state.spec?.spec?.direction !== "max" || key !== state.spec?.spec?.metric;
  $("#d-table tbody").replaceChildren(...sources.map((source) => {
    const points = dash.logs.get(source.path)?.series?.[key] ?? [];
    const best = points.length ? points.reduce((winner, point) => (lower ? point[1] < winner[1] : point[1] > winner[1]) ? point : winner) : null;
    return element("tr", {},
      element("td", { textContent: source.id ?? "live" }),
      element("td", { textContent: identity(source) }),
      element("td", { textContent: source.seed }),
      element("td", {}, element("span", { className: `badge ${source.status === "ok" ? "ok" : source.status === "invalid" ? "bad" : "warn"}`, textContent: source.status })),
      element("td", { textContent: points.length ? num(points.at(-1)[0]) : "–" }),
      element("td", { textContent: points.length ? num(points.at(-1)[1]) : "–" }),
      element("td", { textContent: best ? `${num(best[1])} @ ${num(best[0])}` : "–" }),
    );
  }));
}

function renderGpus() {
  $("#d-gpu-card").hidden = !dash.gpus.size;
  $("#d-gpus").replaceChildren(...[...dash.gpus.entries()].map(([index, history]) => {
    const last = history.points.at(-1)[3];
    const span = Math.max(1, history.points.at(-1)[0] - history.points[0][0]);
    const path = (field) => history.points.map((point, at) => `${at ? "L" : "M"}${((point[0] - history.points[0][0]) / span * 300).toFixed(1)},${(52 - point[field] * 50).toFixed(1)}`).join("");
    return element("div", {},
      element("div", { className: "small", textContent: `GPU ${index} · ${last.name.replace("NVIDIA GeForce ", "")}` }),
      element("div", { className: "muted small", textContent: `${((last.total_mib - last.free_mib) / 1024).toFixed(1)} of ${(last.total_mib / 1024).toFixed(1)} GB · ${last.utilization}% busy` }),
      svg("svg", { viewBox: "0 0 300 54", preserveAspectRatio: "none", role: "img", "aria-label": `GPU ${index} memory and utilisation over time` },
        svg("path", { d: path(1), stroke: "var(--s1)", "vector-effect": "non-scaling-stroke" }),
        svg("path", { d: path(2), stroke: "var(--s2)", "vector-effect": "non-scaling-stroke" })),
      element("div", { className: "legend small" }, element("span", {}, element("i", { className: "swatch", style: "border-color: var(--s1)" }), "memory"), element("span", {}, element("i", { className: "swatch", style: "border-color: var(--s2)" }), "busy")),
    );
  }));
}

for (const id of ["#d-key", "#d-smooth", "#d-log", "#d-invalid"]) $(id).onchange = () => renderDashboard();
addEventListener("resize", () => { if (!$("#dashboard").hidden) renderDashboard(); });
setInterval(() => { if (state.active?.length || [...dash.logs.values()].some((log) => log.more)) guard(refreshDashboard)(); }, 3000);

for (const button of document.querySelectorAll("[role=tab]")) button.onclick = () => selectTab(button.dataset.tab);

guard(async () => {
  applyTheme(await zorai("theme").catch(() => null));
  await loadCampaigns();
  await check();
  await resumeJob();
})();
