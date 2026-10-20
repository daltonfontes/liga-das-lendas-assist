import { EventEmitter } from "node:events";
import type { ChampSelectView } from "./features/champSelect.js";
import type { LiveGameView } from "./live/liveGame.js";

export type GameflowPhase =
  | "None"
  | "Lobby"
  | "Matchmaking"
  | "ReadyCheck"
  | "ChampSelect"
  | "GameStart"
  | "InProgress"
  | "Reconnect"
  | "WaitingForStats"
  | "PreEndOfGame"
  | "EndOfGame"
  | string;

export interface LogEntry {
  time: number;
  message: string;
}

export interface AssistantState {
  connected: boolean;
  summoner?: { gameName: string; tagLine: string; level: number };
  phase: GameflowPhase;
  champSelect?: ChampSelectView;
  liveGame?: LiveGameView;
  log: LogEntry[];
}

/** Estado observável que alimenta o painel. */
export class StateStore extends EventEmitter {
  private state: AssistantState = { connected: false, phase: "None", log: [] };

  get(): AssistantState {
    return this.state;
  }

  update(patch: Partial<AssistantState>): void {
    this.state = { ...this.state, ...patch };
    this.emit("change", this.state);
  }

  log(message: string): void {
    console.log(`[assist] ${message}`);
    const log = [...this.state.log, { time: Date.now(), message }].slice(-50);
    this.update({ log });
  }
}
