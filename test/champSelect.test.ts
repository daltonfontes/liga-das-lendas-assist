import { describe, expect, it } from "vitest";
import { ChampionIndex } from "../src/features/champions.js";
import { buildChampSelectView, decideChampSelect, type ChampSelectSession } from "../src/features/champSelect.js";

const AHRI = 103, ZED = 238, YASUO = 157, LEE = 64, ORIANNA = 61;

function session(overrides: Partial<ChampSelectSession> = {}): ChampSelectSession {
  return {
    localPlayerCellId: 2,
    myTeam: [
      { cellId: 0, championId: 0, championPickIntent: 0, assignedPosition: "top" },
      { cellId: 1, championId: 0, championPickIntent: 0, assignedPosition: "jungle" },
      { cellId: 2, championId: 0, championPickIntent: 0, assignedPosition: "middle" },
    ],
    theirTeam: [{ cellId: 5, championId: 0, championPickIntent: 0, assignedPosition: "" }],
    actions: [],
    bans: { myTeamBans: [], theirTeamBans: [] },
    timer: { phase: "BAN_PICK" },
    ...overrides,
  };
}

const pick = (id: number, actor: number, extra: object = {}) => ({ id, actorCellId: actor, championId: 0, completed: false, isInProgress: false, type: "pick", ...extra });
const ban = (id: number, actor: number, extra: object = {}) => ({ id, actorCellId: actor, championId: 0, completed: false, isInProgress: false, type: "ban", ...extra });

const base = { picks: [AHRI, ORIANNA], bans: [ZED, YASUO], autoLock: false, autoBan: true };

describe("decideChampSelect", () => {
  it("seleciona o primeiro pick disponível sem confirmar", () => {
    const s = session({ actions: [[pick(10, 2, { isInProgress: true })]] });
    expect(decideChampSelect({ ...base, session: s })).toEqual({ actionId: 10, championId: AHRI, type: "pick", lock: false });
  });

  it("confirma quando autoLock está ativo", () => {
    const s = session({ actions: [[pick(10, 2, { isInProgress: true })]] });
    expect(decideChampSelect({ ...base, autoLock: true, session: s })?.lock).toBe(true);
  });

  it("pula campeões banidos ou escolhidos", () => {
    const s = session({
      actions: [[ban(1, 5, { championId: AHRI, completed: true })], [pick(10, 2, { isInProgress: true })]],
    });
    expect(decideChampSelect({ ...base, session: s })?.championId).toBe(ORIANNA);
  });

  it("respeita a lista de campeões disponíveis da conta", () => {
    const s = session({ actions: [[pick(10, 2, { isInProgress: true })]] });
    expect(decideChampSelect({ ...base, pickable: new Set([ORIANNA]), session: s })?.championId).toBe(ORIANNA);
  });

  it("não sobrescreve um campeão que o jogador já selecionou", () => {
    const s = session({ actions: [[pick(10, 2, { isInProgress: true, championId: LEE })]] });
    expect(decideChampSelect({ ...base, session: s })).toBeNull();
    expect(decideChampSelect({ ...base, autoLock: true, session: s })).toEqual({ actionId: 10, championId: LEE, type: "pick", lock: true });
  });

  it("não bane o campeão que um aliado pretende jogar", () => {
    const s = session({ actions: [[ban(3, 2, { isInProgress: true })]] });
    s.myTeam[0]!.championPickIntent = ZED;
    expect(decideChampSelect({ ...base, session: s })).toEqual({ actionId: 3, championId: YASUO, type: "ban", lock: true });
  });

  it("não bane com autoBan desligado", () => {
    const s = session({ actions: [[ban(3, 2, { isInProgress: true })]] });
    expect(decideChampSelect({ ...base, autoBan: false, session: s })).toBeNull();
  });

  it("declara intenção na fase de planejamento", () => {
    const s = session({ timer: { phase: "PLANNING" }, actions: [[pick(10, 2)]] });
    expect(decideChampSelect({ ...base, autoLock: true, session: s })).toEqual({ actionId: 10, championId: AHRI, type: "pick", lock: false });
  });

  it("não faz nada quando não é a sua vez", () => {
    const s = session({ actions: [[pick(10, 1, { isInProgress: true }), pick(11, 2)]] });
    expect(decideChampSelect({ ...base, session: s })).toBeNull();
  });
});

describe("ChampionIndex e visão", () => {
  const index = new ChampionIndex([
    { id: -1, name: "None", alias: "None" },
    { id: LEE, name: "Lee Sin", alias: "LeeSin" },
    { id: AHRI, name: "Ahri", alias: "Ahri" },
  ]);

  it("resolve nomes, aliases e ids", () => {
    expect(index.resolve("lee sin")).toBe(LEE);
    expect(index.resolve("LeeSin")).toBe(LEE);
    expect(index.resolve("103")).toBe(AHRI);
    expect(index.resolve("Teemo")).toBeUndefined();
  });

  it("monta a visão do painel", () => {
    const s = session({ actions: [[ban(1, 5, { championId: LEE, completed: true })]] });
    s.myTeam[2]!.championId = AHRI;
    const view = buildChampSelectView(s, index);
    expect(view.position).toBe("middle");
    expect(view.myTeam[2]).toMatchObject({ champion: "Ahri", isMe: true });
    expect(view.bans).toEqual(["Lee Sin"]);
  });
});
