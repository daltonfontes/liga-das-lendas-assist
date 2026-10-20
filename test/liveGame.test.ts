import { describe, expect, it } from "vitest";
import { buildLiveGameView, type AllGameData, type LivePlayer } from "../src/live/liveGame.js";
import { computeObjectives, describeInhib, OBJECTIVE_TIMES } from "../src/live/objectives.js";

function player(name: string, team: "ORDER" | "CHAOS", extra: Partial<LivePlayer> = {}): LivePlayer {
  return {
    championName: name,
    riotId: `${name}#BR1`,
    riotIdGameName: name,
    team,
    level: 10,
    isDead: false,
    respawnTimer: 0,
    position: "MIDDLE",
    scores: { kills: 1, deaths: 1, assists: 1, creepScore: 100, wardScore: 5 },
    items: [{ itemID: 1, displayName: "Item", price: 1000, count: 1 }],
    ...extra,
  };
}

describe("objetivos", () => {
  const teamOf = (n?: string) => (n?.startsWith("Blue") ? "ORDER" : n?.startsWith("Red") ? "CHAOS" : undefined);

  it("timer inicial do dragão e barão", () => {
    const o = computeObjectives([], 120, teamOf);
    expect(o.timers.find((t) => t.name === "Dragão")).toMatchObject({ remaining: OBJECTIVE_TIMES.dragonFirstSpawn - 120, alive: false });
    expect(o.timers.find((t) => t.name === "Arauto")).toBeDefined();
  });

  it("calcula respawn do dragão e conta por time", () => {
    const o = computeObjectives([{ EventID: 1, EventName: "DragonKill", EventTime: 400, KillerName: "BlueJg", DragonType: "Fire" }], 500, teamOf);
    expect(o.dragons.ORDER).toEqual(["Fire"]);
    expect(o.timers[0]).toMatchObject({ name: "Dragão", spawnsAt: 400 + OBJECTIVE_TIMES.dragonRespawn });
  });

  it("vira Dragão Ancião após a alma", () => {
    const events = [1, 2, 3, 4].map((i) => ({ EventID: i, EventName: "DragonKill", EventTime: i * 400, KillerName: "RedJg", DragonType: "Water" }));
    const o = computeObjectives(events, 1700, teamOf);
    expect(o.timers[0]).toMatchObject({ name: "Dragão Ancião", spawnsAt: 1600 + OBJECTIVE_TIMES.elderRespawn });
  });

  it("acompanha inibidores", () => {
    const o = computeObjectives([{ EventID: 1, EventName: "InhibKilled", EventTime: 1800, InhibKilled: "Barracks_T2_C1" }], 1900, teamOf);
    expect(o.timers.find((t) => t.name.startsWith("Inibidor"))).toMatchObject({ name: "Inibidor vermelho (meio)", remaining: 200 });
    expect(describeInhib("Barracks_T1_L1")).toBe("azul (topo)");
  });
});

describe("buildLiveGameView", () => {
  it("identifica o jogador local, soma ouro e gera dicas", () => {
    const data: AllGameData = {
      activePlayer: { riotId: "Ahri#BR1", level: 11, currentGold: 1234.7 },
      allPlayers: [
        player("Ahri", "ORDER", { items: [{ itemID: 1, displayName: "X", price: 5000, count: 1 }] }),
        player("Zed", "CHAOS", { isDead: true, respawnTimer: 22.3 }),
        player("Lux", "CHAOS", { isDead: true, respawnTimer: 30 }),
      ],
      events: { Events: [] },
      gameData: { gameMode: "CLASSIC", gameTime: 600, mapNumber: 11 },
    };
    const view = buildLiveGameView(data);
    expect(view.myTeam).toBe("ORDER");
    expect(view.me).toEqual({ champion: "Ahri", gold: 1234, level: 11 });
    expect(view.teams.ORDER.itemGold).toBe(5000);
    expect(view.teams.CHAOS.players[0]).toMatchObject({ dead: true, respawnIn: 23, csPerMin: 10 });
    expect(view.tips.some((t) => t.includes("2 inimigos mortos"))).toBe(true);
    expect(view.tips.some((t) => t.includes("3000 de ouro em itens à frente"))).toBe(true);
  });
});
