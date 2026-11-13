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
const DAY = 86400000;
const HOUR_PX = 44;
let view = "week";
let anchor = startOfDay(new Date());
let calendars = [];
let enabled = new Set();
let events = [];
let loadTicket = 0;

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
function startOfDay(date) {
  const copy = new Date(date);
  copy.setHours(0, 0, 0, 0);
  return copy;
}
function addDays(date, days) {
  const copy = new Date(date);
  copy.setDate(copy.getDate() + days);
  return copy;
}
function startOfWeek(date) {
  return addDays(startOfDay(date), -((date.getDay() + 6) % 7));
}
function sameDay(a, b) {
  return a.toDateString() === b.toDateString();
}
function isoDate(date) {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}
function time(date) {
  return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function range() {
  if (view === "week") {
    const start = startOfWeek(anchor);
    return { start, end: addDays(start, 7) };
  }
  if (view === "month") {
    const first = new Date(anchor.getFullYear(), anchor.getMonth(), 1);
    const start = startOfWeek(first);
    return { start, end: addDays(start, 42) };
  }
  return { start: startOfDay(anchor), end: addDays(startOfDay(anchor), 14) };
}

function rangeTitle() {
  const { start, end } = range();
  if (view === "month") return anchor.toLocaleDateString([], { month: "long", year: "numeric" });
  const last = addDays(end, -1);
  const from = start.toLocaleDateString([], { month: "long", day: "numeric" });
  const to = start.getMonth() === last.getMonth() ? String(last.getDate()) : last.toLocaleDateString([], { month: "long", day: "numeric" });
  return `${from} – ${to}, ${last.getFullYear()}`;
}

function normalize(item, calendar) {
  const allDay = Boolean(item.start?.date);
  const start = allDay ? new Date(`${item.start.date}T00:00:00`) : new Date(item.start?.dateTime);
  const end = allDay ? new Date(`${item.end?.date ?? item.start.date}T00:00:00`) : new Date(item.end?.dateTime ?? item.start?.dateTime);
  return { ...item, allDay, start, end, calendar };
}

async function loadCalendars() {
  const list = await call("ui_calendars");
  calendars = (list.items ?? []).sort((a, b) => Number(Boolean(b.primary)) - Number(Boolean(a.primary)) || (a.summary ?? "").localeCompare(b.summary ?? ""));
  enabled = new Set(calendars.filter((calendar) => calendar.selected !== false || calendar.primary).map((calendar) => calendar.id));
  renderCalendars();
}

function renderCalendars() {
  const box = $("#calendars");
  box.innerHTML = "";
  for (const calendar of calendars) {
    const chip = element("button", { type: "button", title: calendar.description || calendar.id },
      element("span", { className: "swatch" }),
      calendar.summaryOverride || calendar.summary || calendar.id);
    chip.style.setProperty("--swatch", calendar.backgroundColor || "var(--accent, #d9a548)");
    chip.setAttribute("aria-pressed", String(enabled.has(calendar.id)));
    chip.addEventListener("click", () => {
      if (enabled.has(calendar.id)) enabled.delete(calendar.id); else enabled.add(calendar.id);
      renderCalendars();
      render();
      if (enabled.has(calendar.id)) void guard(loadEvents)();
    });
    box.append(chip);
  }
}

async function loadEvents() {
  const ticket = ++loadTicket;
  const { start, end } = range();
  const wanted = calendars.filter((calendar) => enabled.has(calendar.id));
  const collected = [];
  $("#view").classList.add("loading");
  try {
    for (const calendar of wanted) {
      const page = await call("ui_events", { calendar_id: calendar.id, time_min: start.toISOString(), time_max: end.toISOString() });
      if (ticket !== loadTicket) return;
      for (const item of page.items ?? []) if (item.status !== "cancelled" && (item.start?.date || item.start?.dateTime)) collected.push(normalize(item, calendar));
      events = collected.slice();
      render();
    }
    events = collected;
    render();
  } finally {
    if (ticket === loadTicket) $("#view").classList.remove("loading");
  }
}

function visibleEvents() {
  return events.filter((event) => enabled.has(event.calendar.id)).sort((a, b) => a.start - b.start || b.end - a.end);
}
function onDay(event, day) {
  const next = addDays(day, 1);
  return event.start < next && (event.end > day || (event.end.getTime() === event.start.getTime() && event.start >= day));
}
function color(event) {
  return event.calendar.backgroundColor || "var(--accent, #d9a548)";
}

function chip(event, withTime = true) {
  const node = element("button", { type: "button", className: `chip-event${event.allDay ? " all-day" : ""}` },
    withTime && !event.allDay ? element("span", { className: "when", textContent: time(event.start) }) : null,
    event.summary || "(no title)");
  node.style.setProperty("--event", color(event));
  node.addEventListener("click", () => openDetails(event));
  return node;
}

function renderWeek() {
  const { start } = range();
  const days = Array.from({ length: 7 }, (_, index) => addDays(start, index));
  const today = startOfDay(new Date());
  const list = visibleEvents();
  const grid = element("div", { className: "week" });
  const head = element("div", { className: "week-head" }, element("div", { className: "gutter" }));
  const allDay = element("div", { className: "week-allday" }, element("div", { className: "gutter muted", textContent: "all day" }));
  const body = element("div", { className: "week-body" });
  const hours = element("div", { className: "gutter hours" });
  for (let hour = 0; hour < 24; hour += 1) hours.append(element("div", { className: "hour", textContent: hour ? `${String(hour).padStart(2, "0")}:00` : "" }));
  body.append(hours);
  for (const day of days) {
    const label = element("button", { type: "button", className: `day-head${sameDay(day, today) ? " today" : ""}` },
      element("span", { textContent: day.toLocaleDateString([], { weekday: "short" }) }),
      element("strong", { textContent: day.getDate() }));
    label.addEventListener("click", () => { anchor = day; setView("agenda"); });
    head.append(label);
    const dayAll = element("div", { className: "allday-cell" });
    for (const event of list.filter((event) => event.allDay && onDay(event, day))) dayAll.append(chip(event, false));
    allDay.append(dayAll);
    const column = element("div", { className: `day-col${sameDay(day, today) ? " today" : ""}`, title: "Click to add an event" });
    column.addEventListener("click", (click) => {
      if (click.target !== column) return;
      const minutes = Math.floor((click.offsetY / HOUR_PX) * 2) * 30;
      const start = new Date(day.getTime() + minutes * 60000);
      void guard(() => eventForm({ start, end: new Date(start.getTime() + 3600000), allDay: false }))();
    });
    const timed = list.filter((event) => !event.allDay && onDay(event, day)).map((event) => ({
      event,
      from: Math.max(0, (event.start - day) / 60000),
      to: Math.min(1440, Math.max((event.end - day) / 60000, (event.start - day) / 60000 + 20)),
    }));
    layoutLanes(timed);
    for (const block of timed) {
      const short = block.to - block.from < 45;
      const node = element("button", { type: "button", className: `block${short ? " short" : ""}`, title: `${block.event.summary || "(no title)"}, ${time(block.event.start)} – ${time(block.event.end)}` },
        element("strong", { textContent: block.event.summary || "(no title)" }),
        short && block.lanes > 1 ? null : element("span", { textContent: short ? time(block.event.start) : `${time(block.event.start)} – ${time(block.event.end)}` }));
      node.style.setProperty("--event", color(block.event));
      node.style.top = `${(block.from / 60) * HOUR_PX}px`;
      node.style.height = `${Math.max(18, ((block.to - block.from) / 60) * HOUR_PX - 2)}px`;
      node.style.left = `calc(${(block.lane / block.lanes) * 100}% + 2px)`;
      node.style.width = `calc(${100 / block.lanes}% - 4px)`;
      node.addEventListener("click", () => openDetails(block.event));
      column.append(node);
    }
    if (sameDay(day, today)) {
      const now = new Date();
      column.append(element("div", { className: "now", style: `top:${((now - day) / 3600000) * HOUR_PX}px` }));
    }
    body.append(column);
  }
  grid.append(head, allDay, body);
  $("#view").replaceChildren(grid);
  body.scrollTop = 7 * HOUR_PX;
}

function layoutLanes(blocks) {
  blocks.sort((a, b) => a.from - b.from || b.to - a.to);
  let cluster = [];
  let clusterEnd = -1;
  const finish = () => {
    const lanes = Math.max(1, ...cluster.map((block) => block.lane + 1));
    for (const block of cluster) block.lanes = lanes;
    cluster = [];
  };
  for (const block of blocks) {
    if (block.from >= clusterEnd && cluster.length) finish();
    const used = new Set(cluster.filter((other) => other.to > block.from).map((other) => other.lane));
    let lane = 0;
    while (used.has(lane)) lane += 1;
    block.lane = lane;
    cluster.push(block);
    clusterEnd = Math.max(clusterEnd, block.to);
  }
  if (cluster.length) finish();
}

function renderMonth() {
  const { start } = range();
  const today = startOfDay(new Date());
  const list = visibleEvents();
  const grid = element("div", { className: "month" });
  for (let index = 0; index < 7; index += 1) grid.append(element("div", { className: "month-dow muted", textContent: addDays(start, index).toLocaleDateString([], { weekday: "short" }) }));
  for (let index = 0; index < 42; index += 1) {
    const day = addDays(start, index);
    const dayEvents = list.filter((event) => onDay(event, day));
    const cell = element("div", { className: `month-cell${day.getMonth() !== anchor.getMonth() ? " outside" : ""}${sameDay(day, today) ? " today" : ""}` });
    const number = element("button", { type: "button", className: "month-day", textContent: day.getDate() });
    number.addEventListener("click", () => { anchor = day; setView("week"); });
    cell.append(number, ...dayEvents.slice(0, 3).map((event) => chip(event)));
    if (dayEvents.length > 3) {
      const more = element("button", { type: "button", className: "more", textContent: `+${dayEvents.length - 3} more` });
      more.addEventListener("click", () => { anchor = day; setView("agenda"); });
      cell.append(more);
    }
    grid.append(cell);
  }
  $("#view").replaceChildren(grid);
}

function renderAgenda() {
  const { start } = range();
  const list = visibleEvents();
  const box = element("div", { className: "agenda" });
  for (let index = 0; index < 14; index += 1) {
    const day = addDays(start, index);
    const dayEvents = list.filter((event) => onDay(event, day));
    if (!dayEvents.length) continue;
    box.append(element("h2", { className: sameDay(day, new Date()) ? "today" : "", textContent: day.toLocaleDateString([], { weekday: "long", month: "long", day: "numeric" }) }));
    for (const event of dayEvents) {
      const row = element("button", { type: "button", className: "agenda-row" },
        element("span", { className: "when", textContent: event.allDay ? "All day" : `${time(event.start)} – ${time(event.end)}` }),
        element("span", { className: "dot" }),
        element("span", { className: "title" }, event.summary || "(no title)", event.location ? element("span", { className: "muted", textContent: ` · ${event.location}` }) : null));
      row.style.setProperty("--event", color(event));
      row.addEventListener("click", () => openDetails(event));
      box.append(row);
    }
  }
  if (!box.children.length) box.append(element("p", { className: "muted", textContent: "Nothing scheduled in the next two weeks." }));
  $("#view").replaceChildren(box);
}

function render() {
  $("#range").textContent = rangeTitle();
  $("#jump").value = isoDate(anchor);
  if (view === "week") renderWeek();
  else if (view === "month") renderMonth();
  else renderAgenda();
}

function setView(next) {
  view = next;
  for (const button of document.querySelectorAll("[data-view]")) button.setAttribute("aria-pressed", String(button.dataset.view === view));
  render();
  void guard(loadEvents)();
}

function move(direction) {
  anchor = view === "month" ? new Date(anchor.getFullYear(), anchor.getMonth() + direction, 1) : addDays(anchor, direction * (view === "week" ? 7 : 14));
  render();
  void guard(loadEvents)();
}

function textOf(html) {
  const doc = new DOMParser().parseFromString(String(html ?? "").replace(/<br\s*\/?>/gi, "\n").replace(/<\/p>/gi, "\n"), "text/html");
  return doc.body.textContent.trim();
}

function whenText(event) {
  if (event.allDay) {
    const last = addDays(event.end, -1);
    return sameDay(event.start, last) || event.end <= event.start
      ? `${event.start.toLocaleDateString([], { weekday: "long", month: "long", day: "numeric" })}, all day`
      : `${event.start.toLocaleDateString([], { month: "long", day: "numeric" })} – ${last.toLocaleDateString([], { month: "long", day: "numeric" })}, all day`;
  }
  return `${event.start.toLocaleDateString([], { weekday: "long", month: "long", day: "numeric" })}, ${time(event.start)} – ${time(event.end)}`;
}

function writable(calendar) {
  return calendar && ["owner", "writer"].includes(calendar.accessRole);
}

function pad(number) {
  return String(number).padStart(2, "0");
}
function localInput(date) {
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

function eventForm({ event, start, end, allDay }) {
  const editing = Boolean(event);
  const choices = calendars.filter(writable);
  if (!choices.length) throw new Error("None of your calendars can be edited.");
  const box = $("#details");
  const title = element("input", { value: event?.summary ?? "", placeholder: "Title", "aria-label": "Title" });
  const allDayBox = element("input", { type: "checkbox", checked: allDay });
  const startInput = element("input", { "aria-label": "Starts" });
  const endInput = element("input", { "aria-label": "Ends" });
  const setKinds = () => {
    const startDay = startInput.value.slice(0, 10) || isoDate(start);
    const endDay = endInput.value.slice(0, 10) || startDay;
    const startTime = startInput.value.length > 10 ? startInput.value.slice(11, 16) : "09:00";
    const endTime = endInput.value.length > 10 ? endInput.value.slice(11, 16) : "10:00";
    if (allDayBox.checked) {
      startInput.type = endInput.type = "date";
      startInput.value = startDay;
      endInput.value = endDay < startDay ? startDay : endDay;
    } else {
      startInput.type = endInput.type = "datetime-local";
      startInput.value = `${startDay}T${startTime}`;
      endInput.value = `${endDay}T${endTime}`;
    }
  };
  startInput.type = endInput.type = allDay ? "date" : "datetime-local";
  startInput.value = allDay ? isoDate(start) : localInput(start);
  endInput.value = allDay ? isoDate(addDays(end, -1) >= start ? addDays(end, -1) : start) : localInput(end);
  allDayBox.addEventListener("change", setKinds);
  let previousStart = startInput.value;
  startInput.addEventListener("change", () => {
    if (!startInput.value || !previousStart || !endInput.value) { previousStart = startInput.value; return; }
    const parse = (value) => new Date(value.length === 10 ? `${value}T00:00:00` : value);
    const shift = parse(startInput.value) - parse(previousStart);
    const moved = new Date(parse(endInput.value).getTime() + shift);
    endInput.value = allDayBox.checked ? isoDate(moved) : localInput(moved);
    previousStart = startInput.value;
  });
  const calendarSelect = element("select", { "aria-label": "Calendar", disabled: editing });
  for (const calendar of editing ? [event.calendar] : choices) calendarSelect.append(new Option(calendar.summaryOverride || calendar.summary || calendar.id, calendar.id));
  const location = element("input", { value: event?.location ?? "", placeholder: "Location", "aria-label": "Location" });
  const notes = element("textarea", { rows: 4, placeholder: "Notes", "aria-label": "Notes", value: event?.description ? textOf(event.description) : "" });
  const save = element("button", { type: "button", className: "primary", textContent: editing ? "Save changes" : "Create event" });
  title.addEventListener("keydown", (key) => { if (key.key === "Enter") save.click(); });
  const cancel = element("button", { type: "button", textContent: "Cancel" });
  cancel.addEventListener("click", () => { if (editing) openDetails(event); else box.hidden = true; });
  save.addEventListener("click", guard(async () => {
    if (!title.value.trim()) throw new Error("Give the event a title.");
    const times = allDayBox.checked
      ? { start: { date: startInput.value }, end: { date: isoDate(addDays(new Date(`${endInput.value || startInput.value}T00:00:00`), 1)) } }
      : { start: { dateTime: new Date(startInput.value).toISOString() }, end: { dateTime: new Date(endInput.value).toISOString() } };
    if ((times.start.dateTime && times.end.dateTime <= times.start.dateTime) || (times.start.date && times.end.date <= times.start.date)) throw new Error("The event has to end after it starts.");
    save.disabled = true;
    try {
      if (editing) {
        const sameTimes = allDayBox.checked === event.allDay && (event.allDay
          ? times.start.date === isoDate(event.start) && times.end.date === isoDate(event.end)
          : new Date(times.start.dateTime).getTime() === event.start.getTime() && new Date(times.end.dateTime).getTime() === event.end.getTime());
        const patch = sameTimes ? {} : { ...times };
        if (title.value.trim() !== (event.summary ?? "")) patch.summary = title.value.trim();
        if (location.value.trim() !== (event.location ?? "")) patch.location = location.value.trim();
        if (notes.value !== (event.description ? textOf(event.description) : "")) patch.description = notes.value;
        if (!Object.keys(patch).length) {
          openDetails(event);
          return;
        }
        await call("update_event", { calendar_id: event.calendar.id, event_id: event.id, patch });
      } else {
        await call("ui_create_event", { calendar_id: calendarSelect.value, event: { summary: title.value.trim(), ...times, ...(location.value.trim() ? { location: location.value.trim() } : {}), ...(notes.value.trim() ? { description: notes.value } : {}) } });
      }
      box.hidden = true;
      await loadEvents();
    } finally {
      save.disabled = false;
    }
  }));
  const card = element("div", { className: "card form" },
    element("header", {}, element("h2", { textContent: editing ? "Edit event" : "New event" })),
    title,
    element("label", { className: "inline" }, allDayBox, "All day"),
    element("div", { className: "times" }, element("label", {}, "Starts", startInput), element("label", {}, "Ends", endInput)),
    element("label", {}, "Calendar", calendarSelect),
    location,
    notes,
    element("div", { className: "actions" }, save, cancel));
  card.style.setProperty("--event", editing ? color(event) : (choices[0].backgroundColor || "var(--accent, #d9a548)"));
  box.replaceChildren(card);
  box.hidden = false;
  requestAnimationFrame(() => title.focus());
}

function openDetails(event) {
  const box = $("#details");
  const status = { accepted: "✓", tentative: "?", declined: "✕", needsAction: "…" };
  const summary = event.summary || "(no title)";
  const describe = `"${summary}" (${whenText(event)}${event.location ? `, ${event.location}` : ""}, event id ${event.id} in calendar ${event.calendar.id})`;
  const close = element("button", { type: "button", className: "close", textContent: "×", "aria-label": "Close" });
  close.addEventListener("click", () => { box.hidden = true; });
  const ask = element("button", { type: "button", className: "primary", textContent: "Ask about this event" });
  ask.addEventListener("click", guard(() => zorai("ask", { text: `About the calendar event ${describe}: ` })));
  const prep = element("button", { type: "button", textContent: "Prepare for this meeting" });
  prep.addEventListener("click", guard(() => zorai("ask", { text: `Prepare me for the calendar event ${describe}: attendees, agenda, related emails and open questions. ` })));
  const me = event.attendees?.find((person) => person.self);
  const rsvp = me ? element("div", { className: "rsvp" }, element("span", { className: "muted", textContent: "Going?" })) : null;
  if (me) {
    for (const [value, label] of [["accepted", "Yes"], ["tentative", "Maybe"], ["declined", "No"]]) {
      const button = element("button", { type: "button", textContent: label });
      button.setAttribute("aria-pressed", String(me.responseStatus === value));
      button.addEventListener("click", guard(async () => {
        await call("update_attendance", { calendar_id: event.calendar.id, event_id: event.id, email: me.email, response_status: value });
        me.responseStatus = value;
        openDetails(event);
      }));
      rsvp.append(button);
    }
  }
  const canEdit = writable(event.calendar);
  const editButton = canEdit ? element("button", { type: "button", textContent: "Edit" }) : null;
  editButton?.addEventListener("click", guard(() => eventForm({ event, start: event.start, end: event.end, allDay: event.allDay })));
  const deleteButton = canEdit ? element("button", { type: "button", className: "danger", textContent: "Delete" }) : null;
  deleteButton?.addEventListener("click", () => {
    const confirm = element("div", { className: "confirm" },
      element("span", { textContent: "Delete this event? This cannot be undone." }),
      element("button", { type: "button", className: "danger", textContent: "Delete" }),
      element("button", { type: "button", textContent: "Keep" }));
    const [, yes, no] = confirm.children;
    yes.addEventListener("click", guard(async () => {
      await call("delete_event", { calendar_id: event.calendar.id, event_id: event.id });
      events = events.filter((other) => !(other.id === event.id && other.calendar.id === event.calendar.id));
      box.hidden = true;
      render();
    }));
    no.addEventListener("click", () => confirm.remove());
    box.querySelector(".actions").after(confirm);
  });
  const meeting = event.hangoutLink || event.conferenceData?.entryPoints?.find((point) => point.entryPointType === "video")?.uri;
  const card = element("div", { className: "card" },
    element("header", {}, element("span", { className: "dot" }), element("h2", { textContent: summary }), close),
    element("p", { textContent: whenText(event) }),
    element("p", { className: "muted", textContent: event.calendar.summaryOverride || event.calendar.summary || event.calendar.id }),
    event.location ? element("p", { textContent: `📍 ${event.location}` }) : null,
    meeting ? element("p", { textContent: `🎥 ${meeting}` }) : null,
    event.attendees?.length ? element("ul", { className: "attendees" }, ...event.attendees.map((person) => element("li", { title: person.responseStatus }, `${status[person.responseStatus] ?? "·"} ${person.displayName || person.email}${person.organizer ? " (organizer)" : ""}${person.self ? " (you)" : ""}`))) : null,
    event.description ? element("p", { className: "description", textContent: textOf(event.description) }) : null,
    rsvp,
    element("div", { className: "actions" }, ask, prep, editButton, deleteButton));
  card.style.setProperty("--event", color(event));
  box.replaceChildren(card);
  box.hidden = false;
}

$("#details").addEventListener("click", (event) => { if (event.target === $("#details")) $("#details").hidden = true; });
addEventListener("keydown", (event) => { if (event.key === "Escape") $("#details").hidden = true; });
$("#today").addEventListener("click", () => { anchor = startOfDay(new Date()); render(); void guard(loadEvents)(); });
$("#prev").addEventListener("click", () => move(-1));
$("#next").addEventListener("click", () => move(1));
$("#jump").addEventListener("change", () => { if ($("#jump").value) { anchor = new Date(`${$("#jump").value}T00:00:00`); render(); void guard(loadEvents)(); } });
for (const button of document.querySelectorAll("[data-view]")) button.addEventListener("click", () => setView(button.dataset.view));
$("#refresh").addEventListener("click", guard(async () => { await loadCalendars(); await loadEvents(); }));
$("#ask").addEventListener("click", guard(() => zorai("ask", { text: "Look at my calendar and " })));
$("#new-event").addEventListener("click", guard(() => {
  const start = new Date(anchor);
  const now = new Date();
  start.setHours(sameDay(anchor, now) ? now.getHours() + 1 : 9, 0, 0, 0);
  eventForm({ start, end: new Date(start.getTime() + 3600000), allDay: false });
}));

render();
guard(async () => { await loadCalendars(); await loadEvents(); })();
