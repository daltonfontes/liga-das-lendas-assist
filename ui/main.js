const POSITIONS = [["default", "Qualquer rota"], ["top", "Topo"], ["jungle", "Selva"], ["middle", "Meio"], ["bottom", "Atirador"], ["utility", "Suporte"]];
const POS_PT = { top: "Topo", jungle: "Selva", middle: "Meio", bottom: "Atirador", utility: "Suporte" };
const PHASE_PT = { None: "No cliente", Lobby: "Lobby", Matchmaking: "Na fila", ReadyCheck: "Partida encontrada", ChampSelect: "Seleção de campeões",
  GameStart: "Carregando", InProgress: "Em partida", Reconnect: "Reconectar", WaitingForStats: "Aguardando estatísticas", PreEndOfGame: "Fim de jogo", EndOfGame: "Fim de jogo" };
const $ = (id) => document.getElementById(id);
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") node.className = v; else node.setAttribute(k, v);
  }
  for (const c of children) node.append(c ?? "");
  return node;
}
const fmt = (s) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;

function row(cells, cls = "", tag = "td") {
  return el("tr", { class: cls }, ...cells.map((c) => el(tag, {}, String(c))));
}

function renderLive(g) {
  $("gameTime").textContent = `${fmt(g.gameTime)} · ${g.gameMode}` + (g.me ? ` · ${g.me.champion} · ${g.me.gold} de ouro` : "");
  $("timers").replaceChildren(...g.objectives.timers.map((t) =>
    el("div", { class: "timer" + (t.alive ? " alive" : "") }, t.name, el("b", {}, t.alive ? "Vivo" : fmt(t.remaining)))));
  const d = g.objectives.dragons;
  $("timers").append(el("div", { class: "timer" }, "Dragões (azul × vermelho)", el("b", {}, `${d.ORDER.length} × ${d.CHAOS.length}`)));
  $("tips").replaceChildren(...(g.tips.length ? g.tips : ["Sem alertas no momento."]).map((t) => el("li", {}, t)));
  $("teams").replaceChildren(...["ORDER", "CHAOS"].map((team) => {
    const t = g.teams[team];
    const title = `${team === g.myTeam ? "Seu time" : "Inimigos"} · ${t.kills} abates · ${t.itemGold} em itens`;
    const table = el("table", {}, row(["Campeão", "Nv", "KDA", "CS", "CS/min", "Itens", ""], "", "th"),
      ...t.players.map((p) => row([p.champion, p.level, p.kda, p.cs, p.csPerMin, p.itemGold, p.dead ? `☠ ${p.respawnIn}s` : ""],
        (p.isMe ? "me " : "") + (p.dead ? "dead" : ""))));
    return el("div", { class: team }, el("div", { class: "team-title" }, title), el("div", { class: "scroll" }, table));
  }));
}

function renderChampSelect(cs) {
  $("csPhase").textContent = [cs.phase, cs.timeLeftMs != null ? `${Math.ceil(cs.timeLeftMs / 1000)}s` : ""].filter(Boolean).join(" · ");
  $("csMine").replaceChildren(row(["Rota", "Campeão", "Jogador"], "", "th"),
    ...cs.myTeam.map((p) => row([POS_PT[p.position] ?? (p.position || "—"), p.champion ?? (p.intent ? `(${p.intent})` : "—"), p.isMe ? "Você" : p.name ?? ""], p.isMe ? "me" : "")));
  $("csTheirs").replaceChildren(row(["Campeão"], "", "th"), ...cs.theirTeam.map((p) => row([p.champion ?? "—"])));
  $("csBans").textContent = cs.bans.length ? `Banidos: ${cs.bans.join(", ")}` : "";
}

function render(s) {
  $("conn").textContent = s.connected ? "Conectado" : "Desconectado";
  $("conn").className = "pill" + (s.connected ? " on" : "");
  $("phase").textContent = PHASE_PT[s.phase] ?? s.phase;
  $("summoner").textContent = s.summoner ? `${s.summoner.gameName}#${s.summoner.tagLine} · nível ${s.summoner.level}` : "";
  $("live").classList.toggle("hidden", !s.liveGame);
  $("champSelect").classList.toggle("hidden", !s.champSelect);
  $("idle").classList.toggle("hidden", !!(s.liveGame || s.champSelect));
  $("idleText").textContent = s.connected ? "Conectado. Entre na fila — o assistente cuida do resto." : "Abra o cliente do League of Legends. O assistente se conecta sozinho.";
  if (s.liveGame) renderLive(s.liveGame);
  if (s.champSelect) renderChampSelect(s.champSelect);
  $("log").replaceChildren(...[...s.log].reverse().map((l) => el("div", {}, `${new Date(l.time).toLocaleTimeString("pt-BR")}  ${l.message}`)));
}

// ---- Configuração ----
const form = $("cfg");
$("prefs").append(...POSITIONS.flatMap(([key, label]) => [
  el("label", { class: "stack" }, `Picks · ${label}`, el("input", { type: "text", name: `champSelect.picks.${key}` })),
  el("label", { class: "stack" }, `Bans · ${label}`, el("input", { type: "text", name: `champSelect.bans.${key}` })),
]));

const getPath = (obj, path) => path.split(".").reduce((o, k) => o?.[k], obj);
function setPath(obj, path, value) {
  const keys = path.split(".");
  keys.slice(0, -1).reduce((o, k) => (o[k] ??= {}), obj)[keys.at(-1)] = value;
}

let config = {};

async function loadConfig() {
  const cfg = (config = await invoke("get_config"));
  for (const input of form.elements) {
    if (!input.name) continue;
    const v = getPath(cfg, input.name);
    if (input.type === "checkbox") input.checked = !!v;
    else input.value = Array.isArray(v) ? v.join(", ") : v ?? "";
  }
}

form.addEventListener("submit", async (e) => {
  e.preventDefault();
  const next = structuredClone(config);
  for (const input of form.elements) {
    if (!input.name) continue;
    let v = input.type === "checkbox" ? input.checked : input.type === "number" ? Number(input.value) : input.value;
    if (input.name.startsWith("champSelect.picks") || input.name.startsWith("champSelect.bans")) {
      v = input.value.split(",").map((x) => x.trim()).filter(Boolean);
    }
    setPath(next, input.name, v);
  }
  try {
    await invoke("set_config", { config: next });
    config = next;
    $("saved").textContent = "Salvo!";
  } catch (err) {
    $("saved").textContent = `Erro ao salvar: ${err}`;
  }
  setTimeout(() => ($("saved").textContent = ""), 2000);
});

loadConfig();
listen("state", (e) => render(e.payload));
invoke("get_state").then(render);
