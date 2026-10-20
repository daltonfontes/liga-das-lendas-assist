import { EventEmitter } from "node:events";
import WebSocket from "ws";
import type { LcuCredentials } from "./credentials.js";
import { RIOT_CA, requestJson } from "./http.js";

export interface LcuEvent<T = unknown> {
  uri: string;
  eventType: "Create" | "Update" | "Delete";
  data: T;
}

type Handler = (event: LcuEvent<any>) => void;

// Códigos de mensagem WAMP 1.0 usados pelo LCU
const WAMP_SUBSCRIBE = 5;
const WAMP_EVENT = 8;

/**
 * Cliente da League Client Update API: REST + WebSocket (WAMP).
 * Emite "close" quando o WebSocket cai (cliente fechado).
 */
export class LcuClient extends EventEmitter {
  private ws?: WebSocket;
  private readonly handlers = new Map<string, Set<Handler>>();
  private readonly auth: string;

  constructor(readonly credentials: LcuCredentials) {
    super();
    this.auth = "Basic " + Buffer.from(`riot:${credentials.password}`).toString("base64");
  }

  request<T = unknown>(method: string, path: string, body?: unknown): Promise<T> {
    return requestJson<T>(this.credentials.port, path, { method, body, auth: this.auth });
  }

  get<T = unknown>(path: string): Promise<T> {
    return this.request<T>("GET", path);
  }

  post<T = unknown>(path: string, body?: unknown): Promise<T> {
    return this.request<T>("POST", path, body);
  }

  patch<T = unknown>(path: string, body?: unknown): Promise<T> {
    return this.request<T>("PATCH", path, body);
  }

  /** Registra um handler para eventos de um endpoint (URI exata). */
  on(event: string | symbol, listener: (...args: any[]) => void): this {
    if (typeof event === "string" && event.startsWith("/")) {
      let set = this.handlers.get(event);
      if (!set) this.handlers.set(event, (set = new Set()));
      set.add(listener as Handler);
      return this;
    }
    return super.on(event, listener);
  }

  connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      const ws = new WebSocket(`wss://127.0.0.1:${this.credentials.port}/`, "wamp", {
        headers: { Authorization: this.auth },
        ca: RIOT_CA,
        // Repassado ao tls.connect, que espera `undefined` para "ok" (os tipos do ws pedem boolean).
        checkServerIdentity: (() => undefined) as never,
      });
      this.ws = ws;
      ws.once("open", () => {
        ws.send(JSON.stringify([WAMP_SUBSCRIBE, "OnJsonApiEvent"]));
        resolve();
      });
      ws.once("error", reject);
      ws.on("message", (raw) => this.dispatch(raw.toString()));
      ws.on("close", () => this.emit("close"));
    });
  }

  close(): void {
    this.ws?.removeAllListeners("close");
    this.ws?.close();
  }

  /** Visível para testes. */
  dispatch(raw: string): void {
    let msg: unknown;
    try {
      msg = JSON.parse(raw);
    } catch {
      return;
    }
    if (!Array.isArray(msg) || msg[0] !== WAMP_EVENT) return;
    const event = msg[2] as LcuEvent;
    if (!event?.uri) return;
    for (const handler of this.handlers.get(event.uri) ?? []) {
      try {
        handler(event);
      } catch (err) {
        this.emit("error", err);
      }
    }
  }
}
