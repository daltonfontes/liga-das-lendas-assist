import https from "node:https";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

/** Certificado raiz público da Riot que assina o LCU e a Live Client Data API. */
export const RIOT_CA = readFileSync(fileURLToPath(new URL("../../certs/riotgames.pem", import.meta.url)));

export class HttpError extends Error {
  constructor(
    readonly status: number,
    readonly body: string,
    message: string,
  ) {
    super(message);
  }
}

export interface RequestOptions {
  method?: string;
  body?: unknown;
  auth?: string;
  timeoutMs?: number;
}

/**
 * Requisição JSON para 127.0.0.1. O certificado é validado contra a CA da Riot,
 * mas o hostname não (o certificado foi emitido para "rclient", não para o IP).
 */
export function requestJson<T>(port: number, path: string, opts: RequestOptions = {}): Promise<T> {
  const payload = opts.body === undefined ? undefined : JSON.stringify(opts.body);
  return new Promise((resolve, reject) => {
    const req = https.request(
      {
        host: "127.0.0.1",
        port,
        path,
        method: opts.method ?? "GET",
        ca: RIOT_CA,
        checkServerIdentity: () => undefined,
        timeout: opts.timeoutMs ?? 5000,
        headers: {
          Accept: "application/json",
          ...(payload ? { "Content-Type": "application/json", "Content-Length": Buffer.byteLength(payload) } : {}),
          ...(opts.auth ? { Authorization: opts.auth } : {}),
        },
      },
      (res) => {
        const chunks: Buffer[] = [];
        res.on("data", (c: Buffer) => chunks.push(c));
        res.on("end", () => {
          const text = Buffer.concat(chunks).toString("utf8");
          const status = res.statusCode ?? 0;
          if (status < 200 || status >= 300) {
            reject(new HttpError(status, text, `${opts.method ?? "GET"} ${path} -> ${status}`));
            return;
          }
          if (!text) return resolve(undefined as T);
          try {
            resolve(JSON.parse(text) as T);
          } catch {
            resolve(text as T);
          }
        });
      },
    );
    req.on("timeout", () => req.destroy(new Error(`timeout em ${path}`)));
    req.on("error", reject);
    if (payload) req.write(payload);
    req.end();
  });
}
