import { readFile, writeFile } from "node:fs/promises";

export type Position = "top" | "jungle" | "middle" | "bottom" | "utility";

export interface AssistantConfig {
  autoAccept: {
    enabled: boolean;
    /** Espera antes de aceitar, para parecer humano e dar tempo de recusar. */
    delayMs: number;
  };
  champSelect: {
    enabled: boolean;
    /** Confirma o campeão automaticamente depois de selecioná-lo. */
    autoLock: boolean;
    /** Banir automaticamente quando for a sua vez. */
    autoBan: boolean;
    /** Campeões (nome ou alias, ex.: "Lee Sin" ou "LeeSin") em ordem de preferência por rota. */
    picks: Partial<Record<Position | "default", string[]>>;
    bans: Partial<Record<Position | "default", string[]>>;
  };
  liveGame: {
    enabled: boolean;
    pollIntervalMs: number;
  };
  dashboard: {
    port: number;
  };
}

export const DEFAULT_CONFIG: AssistantConfig = {
  autoAccept: { enabled: true, delayMs: 2000 },
  champSelect: {
    enabled: false,
    autoLock: false,
    autoBan: false,
    picks: { default: [] },
    bans: { default: [] },
  },
  liveGame: { enabled: true, pollIntervalMs: 2000 },
  dashboard: { port: 3737 },
};

type DeepPartial<T> = { [K in keyof T]?: T[K] extends object ? DeepPartial<T[K]> : T[K] };

function isPlainObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

export function mergeConfig<T>(base: T, patch: DeepPartial<T> | undefined): T {
  if (!isPlainObject(base) || !isPlainObject(patch)) return (patch ?? base) as T;
  const out: Record<string, unknown> = { ...base };
  for (const [key, value] of Object.entries(patch)) {
    if (value === undefined) continue;
    const current = (base as Record<string, unknown>)[key];
    out[key] = isPlainObject(current) && isPlainObject(value) ? mergeConfig(current, value) : value;
  }
  return out as T;
}

export async function loadConfig(file: string): Promise<AssistantConfig> {
  try {
    return mergeConfig(DEFAULT_CONFIG, JSON.parse(await readFile(file, "utf8")));
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code !== "ENOENT") {
      console.warn(`[config] não foi possível ler ${file}: ${(err as Error).message}. Usando padrões.`);
    }
    return structuredClone(DEFAULT_CONFIG);
  }
}

export async function saveConfig(file: string, config: AssistantConfig): Promise<void> {
  await writeFile(file, JSON.stringify(config, null, 2) + "\n", "utf8");
}
