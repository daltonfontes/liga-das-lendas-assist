/**
 * Tempos de objetivos em segundos de jogo. Os valores mudam entre patches —
 * ajuste aqui se a Riot alterar o mapa.
 */
export const OBJECTIVE_TIMES = {
  dragonFirstSpawn: 5 * 60,
  dragonRespawn: 5 * 60,
  elderRespawn: 6 * 60,
  heraldFirstSpawn: 15 * 60,
  baronFirstSpawn: 25 * 60,
  baronRespawn: 6 * 60,
  inhibitorRespawn: 5 * 60,
  /** Dragões necessários para a alma (o próximo vira Ancião). */
  dragonsForSoul: 4,
};

export type Team = "ORDER" | "CHAOS";

export interface LiveEvent {
  EventID: number;
  EventName: string;
  EventTime: number;
  KillerName?: string;
  DragonType?: string;
  InhibKilled?: string;
  Stolen?: string;
}

export interface ObjectiveTimer {
  name: string;
  /** Tempo de jogo (s) em que nasce. */
  spawnsAt: number;
  /** Segundos restantes; 0 quando já está vivo. */
  remaining: number;
  alive: boolean;
}

export interface ObjectiveSummary {
  timers: ObjectiveTimer[];
  dragons: Record<Team, string[]>;
  barons: Record<Team, number>;
}

function timer(name: string, spawnsAt: number, gameTime: number): ObjectiveTimer {
  const remaining = Math.max(0, spawnsAt - gameTime);
  return { name, spawnsAt, remaining, alive: remaining === 0 };
}

/**
 * Calcula os timers de objetivos a partir dos eventos da Live Client Data API.
 * `teamOf` mapeia o nome de quem abateu para o time (minions/torres não têm time).
 */
export function computeObjectives(
  events: LiveEvent[],
  gameTime: number,
  teamOf: (killerName: string | undefined) => Team | undefined,
): ObjectiveSummary {
  const T = OBJECTIVE_TIMES;
  const dragons: Record<Team, string[]> = { ORDER: [], CHAOS: [] };
  const barons: Record<Team, number> = { ORDER: 0, CHAOS: 0 };
  let nextDragon = T.dragonFirstSpawn;
  let nextBaron = T.baronFirstSpawn;
  let heraldTaken = false;
  const inhibs = new Map<string, number>();

  for (const ev of [...events].sort((a, b) => a.EventTime - b.EventTime)) {
    switch (ev.EventName) {
      case "DragonKill": {
        const team = teamOf(ev.KillerName);
        if (team && ev.DragonType !== "Elder") dragons[team].push(ev.DragonType ?? "?");
        const soul = dragons.ORDER.length >= T.dragonsForSoul || dragons.CHAOS.length >= T.dragonsForSoul;
        nextDragon = ev.EventTime + (soul ? T.elderRespawn : T.dragonRespawn);
        break;
      }
      case "BaronKill": {
        const team = teamOf(ev.KillerName);
        if (team) barons[team]++;
        nextBaron = ev.EventTime + T.baronRespawn;
        break;
      }
      case "HeraldKill":
        heraldTaken = true;
        break;
      case "InhibKilled":
        if (ev.InhibKilled) inhibs.set(ev.InhibKilled, ev.EventTime + T.inhibitorRespawn);
        break;
      case "InhibRespawned":
        if (ev.InhibKilled) inhibs.delete(ev.InhibKilled);
        break;
    }
  }

  const soul = dragons.ORDER.length >= T.dragonsForSoul || dragons.CHAOS.length >= T.dragonsForSoul;
  const timers = [timer(soul ? "Dragão Ancião" : "Dragão", nextDragon, gameTime)];
  if (!heraldTaken && gameTime < T.baronFirstSpawn) timers.push(timer("Arauto", T.heraldFirstSpawn, gameTime));
  timers.push(timer("Barão", nextBaron, gameTime));
  for (const [inhib, respawn] of inhibs) {
    if (respawn > gameTime) timers.push(timer(`Inibidor ${describeInhib(inhib)}`, respawn, gameTime));
  }
  return { timers, dragons, barons };
}

/** "Barracks_T1_L1" -> "azul (topo)". T1 = time azul (ORDER), T2 = vermelho (CHAOS). */
export function describeInhib(id: string): string {
  const m = /T(\d)_([LCR])/.exec(id);
  if (!m) return id;
  const side = m[1] === "1" ? "azul" : "vermelho";
  const lane = { L: "topo", C: "meio", R: "bot" }[m[2] as "L" | "C" | "R"];
  return `${side} (${lane})`;
}
