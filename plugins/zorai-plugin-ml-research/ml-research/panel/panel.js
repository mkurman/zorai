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

const state = { campaign: "", spec: null, contract: null, board: null, runs: [], attribution: null, report: null, job: null, selectedRun: null, active: [], paper: null, workspace: localStorage.getItem("ml-research:workspace") || "" };
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
  if (name === "runtimes" && !$("#rt-cards").children.length) guard(loadRuntimes)();
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
  const wanted = select || state.campaign || localStorage.getItem("ml-research:campaign");
  state.campaign = campaigns.some((item) => item.name === wanted) ? wanted : campaigns[0]?.name ?? "";
  picker.value = state.campaign;
  picker.disabled = !campaigns.length;
  if (state.campaign) localStorage.setItem("ml-research:campaign", state.campaign);
  await refreshCampaign();
}

async function refreshCampaign() {
  state.spec = state.contract = state.board = state.attribution = state.report = null;
  state.runs = [];
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
  localStorage.setItem("ml-research:campaign", state.campaign);
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
  localStorage.setItem("ml-research:workspace", folder);
  await loadRuntimes();
});

$("#rt-repo").onclick = guard(async () => {
  if (!state.spec?.repo) throw new Error("Open a campaign first.");
  state.workspace = state.spec.repo;
  localStorage.setItem("ml-research:workspace", state.workspace);
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

for (const button of document.querySelectorAll("[role=tab]")) button.onclick = () => selectTab(button.dataset.tab);

guard(async () => {
  applyTheme(await zorai("theme").catch(() => null));
  await loadCampaigns();
  await check();
  await resumeJob();
})();
