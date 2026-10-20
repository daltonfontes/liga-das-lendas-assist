import type { AssistantConfig } from "../config.js";
import type { LcuClient } from "../lcu/client.js";
import type { StateStore } from "../state.js";

export interface ReadyCheck {
  state: "Invalid" | "InProgress" | "EveryoneReady" | "StrangerNotReady" | "PartyNotReady" | string;
  playerResponse: "None" | "Accepted" | "Declined" | string;
  timer?: number;
}

export function shouldAccept(check: ReadyCheck | undefined): boolean {
  return check?.state === "InProgress" && check.playerResponse === "None";
}

/** Aceita a fila automaticamente quando a partida é encontrada. */
export class AutoAccept {
  private timer?: NodeJS.Timeout;

  constructor(
    private readonly lcu: LcuClient,
    private readonly store: StateStore,
    private readonly config: () => AssistantConfig,
  ) {
    lcu.on("/lol-matchmaking/v1/ready-check", (event) => this.handle(event.data as ReadyCheck | undefined));
  }

  handle(check: ReadyCheck | undefined): void {
    const cfg = this.config().autoAccept;
    if (!cfg.enabled || !shouldAccept(check)) {
      // Fila cancelada ou já respondida (por você, manualmente): não aceita mais.
      if (!shouldAccept(check)) this.cancel();
      return;
    }
    if (this.timer) return;
    this.store.log(`Partida encontrada! Aceitando em ${(cfg.delayMs / 1000).toFixed(1)}s...`);
    this.timer = setTimeout(async () => {
      this.timer = undefined;
      try {
        await this.lcu.post("/lol-matchmaking/v1/ready-check/accept");
        this.store.log("Partida aceita.");
      } catch (err) {
        this.store.log(`Não foi possível aceitar: ${(err as Error).message}`);
      }
    }, cfg.delayMs);
  }

  cancel(): void {
    clearTimeout(this.timer);
    this.timer = undefined;
  }
}
