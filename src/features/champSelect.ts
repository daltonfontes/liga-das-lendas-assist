import type { AssistantConfig, Position } from "../config.js";
import type { LcuClient } from "../lcu/client.js";
import type { StateStore } from "../state.js";
import type { ChampionIndex } from "./champions.js";

export interface ChampSelectAction {
  id: number;
  actorCellId: number;
  championId: number;
  completed: boolean;
  isInProgress: boolean;
  type: "pick" | "ban" | string;
}

export interface ChampSelectPlayer {
  cellId: number;
  championId: number;
  championPickIntent: number;
  assignedPosition: string;
  gameName?: string;
  tagLine?: string;
}

export interface ChampSelectSession {
  localPlayerCellId: number;
  myTeam: ChampSelectPlayer[];
  theirTeam: ChampSelectPlayer[];
  actions: ChampSelectAction[][];
  bans?: { myTeamBans: number[]; theirTeamBans: number[] };
  timer?: { phase: string; adjustedTimeLeftInPhase?: number };
}

export interface ChampSelectDecision {
  actionId: number;
  championId: number;
  type: "pick" | "ban";
  /** true: confirma (completed); false: apenas seleciona/declara intenção. */
  lock: boolean;
}

export interface DecisionInput {
  session: ChampSelectSession;
  /** Preferências já convertidas em ids, em ordem. */
  picks: number[];
  bans: number[];
  autoLock: boolean;
  autoBan: boolean;
  /** Se informado, só considera estes campeões (campeões que você possui/pode jogar). */
  pickable?: Set<number>;
  bannable?: Set<number>;
}

export function localPosition(session: ChampSelectSession): Position | undefined {
  const me = session.myTeam.find((p) => p.cellId === session.localPlayerCellId);
  const pos = me?.assignedPosition?.toLowerCase();
  return pos ? (pos as Position) : undefined;
}

function unavailableChampions(session: ChampSelectSession, forType: "pick" | "ban"): Set<number> {
  const taken = new Set<number>();
  for (const action of session.actions.flat()) {
    if (action.completed && action.championId > 0) taken.add(action.championId);
  }
  for (const id of [...(session.bans?.myTeamBans ?? []), ...(session.bans?.theirTeamBans ?? [])]) {
    if (id > 0) taken.add(id);
  }
  for (const p of [...session.myTeam, ...session.theirTeam]) {
    if (p.cellId === session.localPlayerCellId) continue;
    if (p.championId > 0) taken.add(p.championId);
    // Não banir o que um aliado declarou que vai jogar.
    if (forType === "ban" && session.myTeam.includes(p) && p.championPickIntent > 0) taken.add(p.championPickIntent);
  }
  return taken;
}

/**
 * Decide o que fazer na seleção de campeões a partir da sessão atual.
 * Função pura: não chama o cliente, apenas retorna a próxima ação (ou null).
 */
export function decideChampSelect(input: DecisionInput): ChampSelectDecision | null {
  const { session } = input;
  const mine = session.actions
    .flat()
    .filter((a) => a.actorCellId === session.localPlayerCellId && !a.completed && (a.type === "pick" || a.type === "ban"));

  const current = mine.find((a) => a.isInProgress);
  const planning = session.timer?.phase === "PLANNING";
  // Na fase de planejamento, declara a intenção de pick (sem confirmar).
  const action = current ?? (planning ? mine.find((a) => a.type === "pick") : undefined);
  if (!action) return null;

  const type = action.type as "pick" | "ban";
  if (type === "ban" && !input.autoBan) return null;

  const allowed = type === "pick" ? input.pickable : input.bannable;
  const taken = unavailableChampions(session, type);
  const prefs = type === "pick" ? input.picks : input.bans;
  const available = (id: number) => id > 0 && !taken.has(id) && (!allowed || allowed.has(id));
  // Uma seleção que ainda está disponível (feita por você ou por nós antes) é respeitada.
  const championId = available(action.championId) ? action.championId : prefs.find(available);
  if (!championId) return null;

  const lock = !!current && (type === "ban" || input.autoLock);
  if (action.championId === championId && !lock) return null;
  return { actionId: action.id, championId, type, lock };
}

export interface ChampSelectView {
  phase?: string;
  timeLeftMs?: number;
  position?: string;
  myTeam: { position: string; champion?: string; intent?: string; name?: string; isMe: boolean }[];
  theirTeam: { champion?: string }[];
  bans: string[];
}

export function buildChampSelectView(session: ChampSelectSession, champions: ChampionIndex): ChampSelectView {
  const bans = [...(session.bans?.myTeamBans ?? []), ...(session.bans?.theirTeamBans ?? [])];
  for (const a of session.actions.flat()) {
    if (a.type === "ban" && a.completed && a.championId > 0 && !bans.includes(a.championId)) bans.push(a.championId);
  }
  return {
    phase: session.timer?.phase,
    timeLeftMs: session.timer?.adjustedTimeLeftInPhase,
    position: localPosition(session),
    myTeam: session.myTeam.map((p) => ({
      position: p.assignedPosition,
      champion: champions.name(p.championId),
      intent: champions.name(p.championPickIntent),
      name: p.gameName ? `${p.gameName}#${p.tagLine ?? ""}` : undefined,
      isMe: p.cellId === session.localPlayerCellId,
    })),
    theirTeam: session.theirTeam.map((p) => ({ champion: champions.name(p.championId) })),
    bans: bans.filter((id) => id > 0).map((id) => champions.name(id) ?? String(id)),
  };
}

function resolveList(names: string[] | undefined, champions: ChampionIndex): number[] {
  return (names ?? []).map((n) => champions.resolve(n)).filter((id): id is number => id !== undefined);
}

export class ChampSelectAssistant {
  private lastApplied = "";
  private busy = false;
  private pickable?: Set<number>;
  private bannable?: Set<number>;

  constructor(
    private readonly lcu: LcuClient,
    private readonly store: StateStore,
    private readonly champions: () => ChampionIndex,
    private readonly config: () => AssistantConfig,
  ) {
    lcu.on("/lol-champ-select/v1/session", (event) => {
      if (event.eventType === "Delete") return this.reset();
      void this.handle(event.data as ChampSelectSession);
    });
  }

  reset(): void {
    this.lastApplied = "";
    this.pickable = this.bannable = undefined;
    this.store.update({ champSelect: undefined });
  }

  /** Busca a sessão atual (útil ao iniciar o assistente no meio de uma seleção). */
  async refresh(): Promise<void> {
    try {
      await this.handle(await this.lcu.get<ChampSelectSession>("/lol-champ-select/v1/session"));
    } catch {
      // Fora da seleção de campeões o endpoint responde 404.
    }
  }

  async handle(session: ChampSelectSession): Promise<void> {
    const champions = this.champions();
    this.store.update({ champSelect: buildChampSelectView(session, champions) });

    const cfg = this.config().champSelect;
    if (!cfg.enabled || this.busy) return;

    this.busy = true;
    try {
      await this.loadAvailability();
      const pos = localPosition(session);
      const decision = decideChampSelect({
        session,
        picks: resolveList([...(pos ? cfg.picks[pos] ?? [] : []), ...(cfg.picks.default ?? [])], champions),
        bans: resolveList([...(pos ? cfg.bans[pos] ?? [] : []), ...(cfg.bans.default ?? [])], champions),
        autoLock: cfg.autoLock,
        autoBan: cfg.autoBan,
        pickable: this.pickable,
        bannable: this.bannable,
      });
      if (!decision) return;

      const key = `${decision.actionId}:${decision.championId}:${decision.lock}`;
      if (key === this.lastApplied) return;
      this.lastApplied = key;

      await this.lcu.patch(`/lol-champ-select/v1/session/actions/${decision.actionId}`, {
        championId: decision.championId,
        ...(decision.lock ? { completed: true } : {}),
      });
      const verb = decision.type === "ban" ? "Banido" : decision.lock ? "Confirmado" : "Selecionado";
      this.store.log(`${verb}: ${champions.name(decision.championId) ?? decision.championId}`);
    } catch (err) {
      this.lastApplied = "";
      this.store.log(`Erro na seleção de campeões: ${(err as Error).message}`);
    } finally {
      this.busy = false;
    }
  }

  private async loadAvailability(): Promise<void> {
    if (this.pickable && this.bannable) return;
    const [pickable, bannable] = await Promise.all([
      this.lcu.get<number[]>("/lol-champ-select/v1/pickable-champion-ids").catch(() => undefined),
      this.lcu.get<number[]>("/lol-champ-select/v1/bannable-champion-ids").catch(() => undefined),
    ]);
    if (pickable?.length) this.pickable = new Set(pickable);
    if (bannable?.length) this.bannable = new Set(bannable);
  }
}
