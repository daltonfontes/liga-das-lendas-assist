const { invoke, convertFileSrc } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const nf = new Intl.NumberFormat("pt-BR");

const ROLES = [["default", "Qualquer rota"], ["top", "Topo"], ["jungle", "Selva"], ["middle", "Meio"], ["bottom", "Atirador"], ["utility", "Suporte"]];
const ROLE_PT = Object.fromEntries(ROLES);
const SLOT_STATE_PT = { confirmed: "Confirmado", picking: "Escolhendo", intent: "Intenção", waiting: "Aguardando" };
const CS_PHASE_PT = { PLANNING: "Planejamento", BAN_PICK: "Picks e bans", FINALIZATION: "Finalização" };
const STEPS = [
  ["Fila", ["Lobby", "Matchmaking", "ReadyCheck"]],
  ["Seleção", ["ChampSelect"]],
  ["Em partida", ["GameStart", "InProgress", "Reconnect"]],
  ["Fim", ["WaitingForStats", "PreEndOfGame", "EndOfGame"]],
];

/** Cria um elemento; filhos de texto entram como texto (nunca HTML). */
function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v == null || v === false) continue;
    if (k === "class") node.className = v;
    else node.setAttribute(k, v === true ? "" : v);
  }
  for (const c of children) if (c != null) node.append(c);
  return node;
}
const fmt = (s) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
const initial = (name) => (name ? name[0].toUpperCase() : "?");

/** Avatar com o ícone do campeão (servido pelo app via champ://); sem ícone, fica a inicial. */
function avatar(name, cls = "") {
  const node = el("span", { class: `avatar ${cls}`.trim(), "aria-hidden": "true" }, initial(name));
  if (name) {
    const img = el("img", { src: convertFileSrc(name, "champ"), alt: "" });
    img.addEventListener("error", () => img.remove());
    node.append(img);
  }
  return node;
}

// ---------- Navegação ----------
function showView(next) {
  // Fecha o autocompletar/foco de uma tela que vai sumir.
  const focused = document.activeElement;
  if (focused && focused !== document.body && !$(`view-${next}`).contains(focused)) focused.blur();
  for (const btn of document.querySelectorAll(".nav-item")) {
    if (btn.dataset.view === next) btn.setAttribute("aria-current", "page");
    else btn.removeAttribute("aria-current");
  }
  for (const v of ["live", "select", "settings"]) $(`view-${v}`).classList.toggle("hidden", v !== next);
}
for (const btn of document.querySelectorAll(".nav-item")) btn.addEventListener("click", () => showView(btn.dataset.view));

// ---------- Estado ----------
let state = null;
let lastPhase = null;

function render(s) {
  state = s;
  renderStatus(s);
  renderStepper(s.phase);
  renderLive(s);
  renderSelect(s);
  renderLog(s.log);
  // Segue a fase do jogo quando ela muda.
  if (s.phase !== lastPhase) {
    if (s.phase === "ChampSelect") showView("select");
    else if (["GameStart", "InProgress", "Reconnect"].includes(s.phase)) showView("live");
    lastPhase = s.phase;
  }
}

function renderStatus(s) {
  $("conn").classList.toggle("on", s.connected);
  $("conn-text").textContent = s.connected ? "Conectado ao cliente" : "Aguardando o cliente";
  const sm = s.summoner;
  $("summoner").replaceChildren(...(sm ? [sm.gameName, el("span", {}, `#${sm.tagLine}`)] : []));
  $("summoner-level").textContent = sm ? `Nível ${sm.level}` : "";
}

function renderStepper(phase) {
  $("stepper").replaceChildren(...STEPS.map(([label, phases]) =>
    el("li", { "aria-current": phases.includes(phase) ? "step" : null }, label)));
}

function renderLive(s) {
  const g = s.liveGame;
  $("live-empty").classList.toggle("hidden", !!g);
  $("live-content").classList.toggle("hidden", !g);
  if (!g) {
    $("clock").replaceChildren();
    $("live-empty-text").textContent = s.connected
      ? "Entre na fila — o assistente acompanha tudo daqui."
      : "Abra o cliente do League of Legends. O assistente se conecta sozinho.";
    return;
  }
  const me = g.me;
  $("clock").replaceChildren(
    el("span", { class: "num" }, fmt(g.gameTime)),
    el("span", {}, me ? `${me.champion} · nível ${me.level} · ` : g.gameMode, me ? el("strong", {}, `${nf.format(me.gold)} de ouro`) : null),
  );

  $("timers").replaceChildren(...g.objectives.timers.map((t) => {
    const side = t.detail?.startsWith("vermelho") ? " side-red" : t.detail?.startsWith("azul") ? " side-blue" : "";
    const tag = t.alive ? "Vivo" : t.detail ?? `às ${fmt(t.spawnsAt)}`;
    const bar = el("div");
    bar.style.width = `${Math.round((t.alive ? 1 : t.progress) * 100)}%`;
    return el("div", { class: `timer${t.alive ? " alive" : ""}${side}` },
      el("div", { class: "timer-head" }, el("span", {}, t.name), el("span", { class: "timer-tag" }, tag)),
      el("div", { class: "timer-value" }, t.alive ? "Vivo" : fmt(t.remaining)),
      el("div", { class: "progress" }, bar));
  }));

  const [main, ...more] = g.tips;
  $("tips").classList.toggle("calm", !main);
  $("tip-main").textContent = main ?? "Sem alertas no momento.";
  $("tip-more").replaceChildren(...more.map((t) => el("li", {}, t)));

  const enemy = g.myTeam === "ORDER" ? "CHAOS" : "ORDER";
  const mine = g.teams[g.myTeam], theirs = g.teams[enemy];
  const total = mine.kills + theirs.kills;
  $("kills-mine").textContent = mine.kills;
  $("kills-enemy").textContent = theirs.kills;
  $("killbar-mine").style.width = `${total ? (mine.kills / total) * 100 : 50}%`;
  const drakes = g.objectives.dragons;
  const caption = (team, label) => {
    const n = drakes[team].length;
    return `${label} · ${nf.format(g.teams[team].itemGold)} em itens${n ? ` · ${n} ${n > 1 ? "dragões" : "dragão"}` : ""}`;
  };
  $("caption-mine").textContent = caption(g.myTeam, "Seu time");
  $("caption-enemy").textContent = caption(enemy, "Inimigos");

  $("teams").replaceChildren(teamTable(mine, "Seu time", "mine"), teamTable(theirs, "Inimigos", "enemy"));
}

function teamTable(team, label, cls) {
  const head = el("tr", {}, ...[label, "KDA", "CS/min", "Itens"].map((h) => el("th", { scope: "col" }, h)));
  const rows = team.players.map((p) => el("tr", { class: [p.isMe && "me", p.dead && "dead"].filter(Boolean).join(" ") || null },
    el("td", {}, el("div", { class: "who" },
      avatar(p.champion, p.isMe ? "me" : ""),
      el("span", { class: "who-text" },
        el("span", { class: "champ-name" }, p.champion),
        el("span", { class: "muted small" }, `Nv ${p.level}${p.dead ? ` · renasce em ${p.respawnIn}s` : ""}`)))),
    el("td", {}, p.kda),
    el("td", {}, nf.format(p.csPerMin)),
    el("td", { class: "gold" }, nf.format(p.itemGold))));
  return el("div", { class: `team-card ${cls}` }, el("div", { class: "stripe" }),
    el("div", { class: "table-wrap" }, el("table", {}, el("thead", {}, head), el("tbody", {}, ...rows))));
}

function renderSelect(s) {
  const cs = s.champSelect;
  $("select-empty").classList.toggle("hidden", !!cs);
  $("select-content").classList.toggle("hidden", !cs);
  $("ring").classList.toggle("hidden", !cs);
  if (!cs) return;

  $("cs-phase").textContent = CS_PHASE_PT[cs.phase] ?? "Seleção";
  const left = Math.max(0, Math.ceil((cs.timeLeftMs ?? 0) / 1000));
  const pct = cs.totalMs ? Math.min(100, ((cs.timeLeftMs ?? 0) / cs.totalMs) * 100) : 0;
  $("ring").style.setProperty("--p", pct.toFixed(1));
  $("ring").setAttribute("aria-label", `${left} segundos restantes`);
  $("ring-value").textContent = left;

  const paused = s.automationPaused;
  const a = cs.assistant;
  $("select-content").querySelector(".assistant-card").classList.toggle("paused", paused);
  const assistantAvatar = avatar(a?.champion, "avatar-lg me");
  assistantAvatar.id = "assistant-avatar";
  $("assistant-avatar").replaceWith(assistantAvatar);
  $("assistant-title").textContent = paused ? "Automação pausada nesta seleção" : a?.title ?? "";
  $("assistant-detail").textContent = paused ? "Você escolhe manualmente. Retome quando quiser." : a?.detail ?? "";
  $("assistant-runes").textContent = cs.runes ?? "";
  $("assistant-runes").classList.toggle("hidden", !cs.runes);
  $("pause").classList.toggle("hidden", !config.champSelect?.enabled);
  $("pause").classList.toggle("active", paused);
  $("pause").textContent = paused ? "Retomar automação" : "Pausar automação";

  const slot = (champion, intent, sub, slotState, isMe) => {
    const shown = champion ?? intent;
    const champClass = champion ? "slot-champ" : intent ? "slot-champ intent" : "slot-champ empty-champ";
    return el("div", { class: `slot${isMe ? " me" : ""}` },
      avatar(shown, `avatar-md${isMe ? " me" : ""}${!champion && intent ? " intent" : ""}`),
      el("div", { class: "slot-text" }, el("span", { class: champClass }, shown ?? "A escolher"), el("span", { class: "muted small" }, sub)),
      el("span", { class: `slot-state ${slotState}` }, SLOT_STATE_PT[slotState] ?? ""));
  };
  const ally = cs.myTeam.map((p) =>
    slot(p.champion, p.intent, [ROLE_PT[p.position], p.isMe ? "Você" : p.name].filter(Boolean).join(" · "), p.state, p.isMe));
  const foes = cs.theirTeam.map((p, i) => slot(p.champion, null, `Jogador ${i + 1}`, p.state, false));
  $("cs-teams").replaceChildren(
    el("section", { class: "team-col", "aria-label": "Seu time" }, el("div", { class: "team-label blue" }, "Seu time"), ...ally),
    el("section", { class: "team-col", "aria-label": "Inimigos" }, el("div", { class: "team-label red" }, "Inimigos"), ...foes));
  $("bans").replaceChildren(...(cs.bans.length ? cs.bans.map((b) => el("span", { class: "ban-chip" }, avatar(b, "avatar-xs"), el("span", { class: "strike" }, b))) : [el("span", { class: "muted" }, "Nenhum ainda")]));
}

$("pause").addEventListener("click", () => invoke("set_automation_paused", { paused: !state?.automationPaused }));

function renderLog(log) {
  const kind = (m) => /Erro|Não foi possível|inválid/.test(m) ? "err"
    : /Confirmado|Banido|aceita\.|Conectado|Runas importadas/.test(m) ? "ok"
    : /Partida encontrada|pausada/.test(m) ? "warn" : "info";
  $("log").replaceChildren(...[...log].reverse().map((l) => el("div", { class: `log-entry ${kind(l.message)}` },
    el("time", {}, new Date(l.time).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })),
    el("span", {}, l.message))));
}

// ---------- Configurações ----------
const form = $("cfg");
let config = {};
let role = "default";
let champions = [];
let knownChampions = new Set();
const norm = (s) => s.toLowerCase().normalize("NFD").replace(/[^a-z0-9]/g, "");

const getPath = (obj, path) => path.split(".").reduce((o, k) => o?.[k], obj);
function setPath(obj, path, value) {
  const keys = path.split(".");
  keys.slice(0, -1).reduce((o, k) => (o[k] ??= {}), obj)[keys.at(-1)] = value;
}
const delayText = (ms) => `${(ms / 1000).toLocaleString("pt-BR")}s`;

function fillForm() {
  for (const input of form.querySelectorAll("input[name]")) {
    const v = getPath(config, input.name);
    if (input.type === "checkbox") input.checked = !!v;
    else input.value = v ?? "";
  }
  $("delay-out").textContent = delayText(config.autoAccept.delayMs);
  renderRoles();
}

function renderRoles() {
  $("roles").replaceChildren(...ROLES.map(([key, label]) => {
    const n = (config.champSelect.picks[key]?.length ?? 0) + (config.champSelect.bans[key]?.length ?? 0);
    const tab = el("button", { type: "button", class: "role", role: "tab", "aria-selected": String(key === role) }, label,
      n ? el("span", { class: "count", "aria-label": `${n} campeões` }, String(n)) : null);
    tab.addEventListener("click", () => { role = key; renderRoles(); });
    return tab;
  }));
  $("picks-label").textContent = `Picks · ${ROLE_PT[role]}`;
  $("bans-label").textContent = `Bans · ${ROLE_PT[role]}`;
  for (const box of form.querySelectorAll(".chip-input")) renderChips(box);
}

const REMOVE_ICON = '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18"/></svg>';

function renderChips(box) {
  const items = config.champSelect[box.dataset.list][role] ?? [];
  box.querySelector(".chips").replaceChildren(...items.map((name, i) => {
    const unknown = knownChampions.size > 0 && !knownChampions.has(norm(name));
    const remove = el("button", { type: "button", "aria-label": `Remover ${name}` });
    remove.innerHTML = REMOVE_ICON; // ícone fixo, sem dados do usuário
    remove.addEventListener("click", () => { items.splice(i, 1); save(); renderRoles(); });
    return el("span", { class: `chip${unknown ? " unknown" : ""}`, title: unknown ? "Campeão não encontrado" : null },
      el("span", { class: "n" }, String(i + 1)), avatar(name, "avatar-xs"), name, remove);
  }));
}

for (const box of form.querySelectorAll(".chip-input")) {
  const input = box.querySelector("input");
  box.addEventListener("click", (e) => { if (e.target === box) input.focus(); });
  const add = () => {
    const name = input.value.replace(/,/g, "").trim();
    if (!name) return;
    const match = champions.find((c) => norm(c) === norm(name)) ?? name;
    const items = (config.champSelect[box.dataset.list][role] ??= []);
    if (!items.some((n) => norm(n) === norm(match))) items.push(match);
    input.value = "";
    save();
    renderRoles();
    input.focus();
  };
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === ",") { e.preventDefault(); add(); }
    else if (e.key === "Backspace" && !input.value) {
      const items = config.champSelect[box.dataset.list][role];
      if (items?.length) { items.pop(); save(); renderRoles(); input.focus(); }
    }
  });
  // Escolher uma sugestão do autocompletar adiciona direto.
  input.addEventListener("change", () => { if (champions.some((c) => norm(c) === norm(input.value))) add(); });
}

form.addEventListener("input", (e) => {
  const input = e.target;
  if (!input.name) return;
  const value = input.type === "checkbox" ? input.checked : Number(input.value);
  setPath(config, input.name, value);
  if (input.name === "autoAccept.delayMs") $("delay-out").textContent = delayText(value);
  save();
  if (state) renderSelect(state);
});
form.addEventListener("submit", (e) => e.preventDefault());

let saveTimer;
function save() {
  clearTimeout(saveTimer);
  $("saved").textContent = "Salvando…";
  saveTimer = setTimeout(async () => {
    try {
      await invoke("set_config", { config });
      $("saved").textContent = "Salvo";
    } catch (err) {
      $("saved").textContent = `Erro ao salvar: ${err}`;
    }
  }, 300);
}

// A lista de campeões chega do cliente pouco depois de conectar.
let champTimer = null;
function scheduleChampions() {
  if (champions.length || champTimer) return;
  champTimer = setTimeout(async () => {
    champTimer = null;
    champions = await invoke("get_champions");
    knownChampions = new Set(champions.map(norm));
    $("champ-list").replaceChildren(...champions.map((c) => el("option", { value: c })));
    renderRoles();
  }, 1000);
}

// ---------- Início ----------
showView("live");
invoke("get_config").then((c) => {
  config = c;
  config.champSelect.picks ??= {};
  config.champSelect.bans ??= {};
  fillForm();
  return invoke("get_state");
}).then((s) => {
  render(s);
  listen("state", (e) => {
    render(e.payload);
    if (e.payload.connected) scheduleChampions();
  });
  if (s.connected) scheduleChampions();
});
