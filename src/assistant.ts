import { setTimeout as sleep } from "node:timers/promises";
import type { AssistantConfig } from "./config.js";
import { AutoAccept } from "./features/autoAccept.js";
import { ChampSelectAssistant } from "./features/champSelect.js";
import { ChampionIndex, type ChampionSummary } from "./features/champions.js";
import { LcuClient } from "./lcu/client.js";
import { findCredentials } from "./lcu/credentials.js";
import { LiveGameTracker } from "./live/liveGame.js";
import type { GameflowPhase, StateStore } from "./state.js";

const PHASES_PT: Record<string, string> = {
  None: "No cliente",
  Lobby: "No lobby",
  Matchmaking: "Na fila",
  ReadyCheck: "Partida encontrada",
  ChampSelect: "Seleção de campeões",
  GameStart: "Carregando partida",
  InProgress: "Em partida",
  Reconnect: "Reconectar",
  WaitingForStats: "Aguardando estatísticas",
  PreEndOfGame: "Fim de jogo",
  EndOfGame: "Fim de jogo",
};

/** Liga o assistente ao cliente do LoL e reconecta quando o cliente é reaberto. */
export class Assistant {
  private champions = new ChampionIndex();
  private readonly live: LiveGameTracker;
  private running = false;

  constructor(
    private readonly store: StateStore,
    private readonly config: () => AssistantConfig,
  ) {
    this.live = new LiveGameTracker(store, config);
  }

  async run(): Promise<void> {
    this.running = true;
    let warned = false;
    while (this.running) {
      const creds = await findCredentials();
      if (!creds) {
        if (!warned) this.store.log("Aguardando o cliente do League of Legends abrir...");
        warned = true;
        await sleep(3000);
        continue;
      }
      warned = false;
      const lcu = new LcuClient(creds);
      try {
        await lcu.connect();
      } catch {
        // O processo existe mas a API ainda não subiu.
        await sleep(2000);
        continue;
      }
      await this.session(lcu);
    }
  }

  stop(): void {
    this.running = false;
    this.live.stop();
  }

  private async session(lcu: LcuClient): Promise<void> {
    const closed = new Promise<void>((resolve) => lcu.once("close", resolve));
    lcu.on("error", (err: Error) => this.store.log(`Erro: ${err.message}`));

    const autoAccept = new AutoAccept(lcu, this.store, this.config);
    const champSelect = new ChampSelectAssistant(lcu, this.store, () => this.champions, this.config);
    lcu.on("/lol-gameflow/v1/gameflow-phase", (event) => void this.onPhase(lcu, event.data as GameflowPhase, champSelect));

    this.store.update({ connected: true });
    this.store.log(`Conectado ao cliente (porta ${lcu.credentials.port}).`);

    await this.loadSummoner(lcu);
    await this.loadChampions(lcu);
    const phase = await lcu.get<GameflowPhase>("/lol-gameflow/v1/gameflow-phase").catch(() => "None");
    await this.onPhase(lcu, phase, champSelect);

    await closed;
    autoAccept.cancel();
    this.live.stop();
    this.store.update({ connected: false, phase: "None", champSelect: undefined, liveGame: undefined, summoner: undefined });
    this.store.log("Cliente fechado.");
  }

  private async onPhase(lcu: LcuClient, phase: GameflowPhase, champSelect: ChampSelectAssistant): Promise<void> {
    if (phase === this.store.get().phase && this.store.get().connected) return;
    this.store.update({ phase });
    this.store.log(`Fase: ${PHASES_PT[phase] ?? phase}`);

    if (phase === "ChampSelect") {
      if (!this.champions.size) await this.loadChampions(lcu);
      await champSelect.refresh();
    } else {
      champSelect.reset();
    }

    if (phase === "InProgress" || phase === "GameStart" || phase === "Reconnect") {
      this.live.start();
    } else {
      this.live.stop();
      if (phase !== "WaitingForStats" && phase !== "PreEndOfGame" && phase !== "EndOfGame") {
        this.store.update({ liveGame: undefined });
      }
    }
  }

  private async loadSummoner(lcu: LcuClient): Promise<void> {
    try {
      const s = await lcu.get<{ gameName: string; tagLine: string; summonerLevel: number }>("/lol-summoner/v1/current-summoner");
      this.store.update({ summoner: { gameName: s.gameName, tagLine: s.tagLine, level: s.summonerLevel } });
    } catch {
      // Ainda na tela de login.
    }
  }

  private async loadChampions(lcu: LcuClient): Promise<void> {
    try {
      this.champions = new ChampionIndex(await lcu.get<ChampionSummary[]>("/lol-game-data/assets/v1/champion-summary.json"));
    } catch (err) {
      this.store.log(`Não foi possível carregar a lista de campeões: ${(err as Error).message}`);
    }
  }
}
