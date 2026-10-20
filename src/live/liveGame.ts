import type { AssistantConfig } from "../config.js";
import { requestJson } from "../lcu/http.js";
import type { StateStore } from "../state.js";
import { computeObjectives, type LiveEvent, type ObjectiveSummary, type Team } from "./objectives.js";

/** Porta fixa da Live Client Data API (só existe durante a partida). */
export const LIVE_CLIENT_PORT = 2999;

export interface LivePlayer {
  championName: string;
  riotId?: string;
  riotIdGameName?: string;
  summonerName?: string;
  team: Team;
  level: number;
  isDead: boolean;
  respawnTimer: number;
  position?: string;
  scores: { kills: number; deaths: number; assists: number; creepScore: number; wardScore: number };
  items: { itemID: number; displayName: string; price: number; count: number }[];
}

export interface AllGameData {
  activePlayer: {
    riotId?: string;
    riotIdGameName?: string;
    summonerName?: string;
    level: number;
    currentGold: number;
  };
  allPlayers: LivePlayer[];
  events: { Events: LiveEvent[] };
  gameData: { gameMode: string; gameTime: number; mapNumber: number };
}

export interface LivePlayerView {
  champion: string;
  name: string;
  level: number;
  kda: string;
  cs: number;
  csPerMin: number;
  itemGold: number;
  dead: boolean;
  respawnIn: number;
  position: string;
  isMe: boolean;
}

export interface LiveGameView {
  gameTime: number;
  gameMode: string;
  myTeam: Team;
  me?: { champion: string; gold: number; level: number };
  teams: Record<Team, { kills: number; itemGold: number; players: LivePlayerView[] }>;
  objectives: ObjectiveSummary;
  tips: string[];
}

const nameOf = (p: { riotId?: string; riotIdGameName?: string; summonerName?: string }) =>
  p.riotId ?? p.riotIdGameName ?? p.summonerName ?? "";

function matchesName(p: LivePlayer, name: string | undefined): boolean {
  if (!name) return false;
  return [p.riotId, p.riotIdGameName, p.summonerName].some((n) => n && (n === name || n.split("#")[0] === name));
}

/** Transforma o JSON de /liveclientdata/allgamedata no que o painel mostra. */
export function buildLiveGameView(data: AllGameData): LiveGameView {
  const gameTime = data.gameData.gameTime;
  const minutes = Math.max(gameTime / 60, 1 / 60);
  const myName = nameOf(data.activePlayer);
  const meRaw = data.allPlayers.find((p) => matchesName(p, myName));
  const myTeam: Team = meRaw?.team ?? "ORDER";

  const teams: LiveGameView["teams"] = {
    ORDER: { kills: 0, itemGold: 0, players: [] },
    CHAOS: { kills: 0, itemGold: 0, players: [] },
  };
  for (const p of data.allPlayers) {
    const itemGold = p.items.reduce((sum, i) => sum + i.price * Math.max(1, i.count), 0);
    const team = teams[p.team];
    team.kills += p.scores.kills;
    team.itemGold += itemGold;
    team.players.push({
      champion: p.championName,
      name: nameOf(p),
      level: p.level,
      kda: `${p.scores.kills}/${p.scores.deaths}/${p.scores.assists}`,
      cs: p.scores.creepScore,
      csPerMin: Math.round((p.scores.creepScore / minutes) * 10) / 10,
      itemGold,
      dead: p.isDead,
      respawnIn: Math.ceil(p.respawnTimer),
      position: p.position ?? "",
      isMe: p === meRaw,
    });
  }

  const teamOf = (killer: string | undefined) => data.allPlayers.find((p) => matchesName(p, killer))?.team;
  const objectives = computeObjectives(data.events.Events, gameTime, teamOf);

  return {
    gameTime,
    gameMode: data.gameData.gameMode,
    myTeam,
    me: meRaw ? { champion: meRaw.championName, gold: Math.floor(data.activePlayer.currentGold), level: data.activePlayer.level } : undefined,
    teams,
    objectives,
    tips: buildTips(teams, myTeam, objectives, meRaw && teams[myTeam].players.find((p) => p.isMe), gameTime),
  };
}

function buildTips(
  teams: LiveGameView["teams"],
  myTeam: Team,
  objectives: ObjectiveSummary,
  me: LivePlayerView | undefined,
  gameTime: number,
): string[] {
  const tips: string[] = [];
  const enemy: Team = myTeam === "ORDER" ? "CHAOS" : "ORDER";
  const deadEnemies = teams[enemy].players.filter((p) => p.dead);
  const deadAllies = teams[myTeam].players.filter((p) => p.dead);

  for (const t of objectives.timers) {
    if (!t.alive && t.remaining <= 60) tips.push(`${t.name} nasce em ${t.remaining}s — prepare visão e prioridade de rota.`);
  }
  if (deadEnemies.length >= 2 && deadEnemies.length > deadAllies.length) {
    const window = Math.min(...deadEnemies.map((p) => p.respawnIn));
    const alive = objectives.timers.filter((t) => t.alive).map((t) => t.name);
    const target = alive.length ? alive.join(" / ") : "torres";
    tips.push(`${deadEnemies.length} inimigos mortos (~${window}s): janela para ${target}.`);
  }
  if (deadAllies.length >= 3 && deadAllies.length > deadEnemies.length) {
    tips.push(`${deadAllies.length} aliados mortos: jogue seguro e evite lutas.`);
  }
  if (me && gameTime > 10 * 60 && me.csPerMin < 6 && me.position.toUpperCase() !== "UTILITY") {
    tips.push(`Seu farm está em ${me.csPerMin} CS/min — tente chegar a 7+.`);
  }
  const diff = teams[myTeam].itemGold - teams[enemy].itemGold;
  if (Math.abs(diff) >= 3000) {
    tips.push(diff > 0 ? `Vocês estão ${diff} de ouro em itens à frente: force objetivos.` : `Vocês estão ${-diff} de ouro em itens atrás: farme e defenda.`);
  }
  return tips;
}

/** Faz polling da Live Client Data API enquanto a partida estiver em andamento. */
export class LiveGameTracker {
  private interval?: NodeJS.Timeout;

  constructor(
    private readonly store: StateStore,
    private readonly config: () => AssistantConfig,
  ) {}

  start(): void {
    if (this.interval || !this.config().liveGame.enabled) return;
    this.store.log("Partida em andamento: acompanhando dados ao vivo.");
    const tick = async () => {
      try {
        const data = await requestJson<AllGameData>(LIVE_CLIENT_PORT, "/liveclientdata/allgamedata", { timeoutMs: 1500 });
        this.store.update({ liveGame: buildLiveGameView(data) });
      } catch {
        // A API demora alguns segundos para subir na tela de carregamento.
      }
    };
    void tick();
    this.interval = setInterval(tick, this.config().liveGame.pollIntervalMs);
  }

  stop(): void {
    clearInterval(this.interval);
    this.interval = undefined;
  }
}
