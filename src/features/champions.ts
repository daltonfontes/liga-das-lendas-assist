export interface ChampionSummary {
  id: number;
  name: string;
  alias: string;
}

const normalize = (s: string) => s.toLowerCase().replace(/[^a-z0-9]/g, "");

/** Índice de campeões vindo de `/lol-game-data/assets/v1/champion-summary.json`. */
export class ChampionIndex {
  private readonly byId = new Map<number, ChampionSummary>();
  private readonly byKey = new Map<string, ChampionSummary>();

  constructor(champions: ChampionSummary[] = []) {
    for (const c of champions) {
      if (c.id <= 0) continue;
      this.byId.set(c.id, c);
      this.byKey.set(normalize(c.name), c);
      this.byKey.set(normalize(c.alias), c);
    }
  }

  get size(): number {
    return this.byId.size;
  }

  name(id: number | undefined): string | undefined {
    return id ? this.byId.get(id)?.name : undefined;
  }

  /** Aceita nome ("Lee Sin"), alias ("LeeSin") ou id numérico. */
  resolve(nameOrId: string | number): number | undefined {
    if (typeof nameOrId === "number") return this.byId.has(nameOrId) ? nameOrId : undefined;
    if (/^\d+$/.test(nameOrId)) return this.resolve(Number(nameOrId));
    return this.byKey.get(normalize(nameOrId))?.id;
  }
}
