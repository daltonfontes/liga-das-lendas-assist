import http from "node:http";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { mergeConfig, type AssistantConfig } from "../config.js";
import type { StateStore } from "../state.js";

const INDEX_HTML = fileURLToPath(new URL("../../public/index.html", import.meta.url));

export interface DashboardOptions {
  port: number;
  store: StateStore;
  getConfig: () => AssistantConfig;
  setConfig: (config: AssistantConfig) => Promise<void>;
}

/** Painel local em http://127.0.0.1:<porta> com atualização via Server-Sent Events. */
export function startDashboard(opts: DashboardOptions): http.Server {
  const allowedHosts = new Set([`127.0.0.1:${opts.port}`, `localhost:${opts.port}`]);

  const server = http.createServer(async (req, res) => {
    // Bloqueia DNS rebinding: só aceita requisições endereçadas ao localhost.
    if (!allowedHosts.has(req.headers.host ?? "")) {
      res.writeHead(403).end();
      return;
    }
    const url = new URL(req.url ?? "/", "http://localhost");
    try {
      if (req.method === "GET" && url.pathname === "/") {
        res.writeHead(200, { "Content-Type": "text/html; charset=utf-8" });
        res.end(await readFile(INDEX_HTML));
      } else if (req.method === "GET" && url.pathname === "/api/state") {
        sendJson(res, opts.store.get());
      } else if (req.method === "GET" && url.pathname === "/api/events") {
        res.writeHead(200, { "Content-Type": "text/event-stream", "Cache-Control": "no-cache", Connection: "keep-alive" });
        const push = (state: unknown) => res.write(`data: ${JSON.stringify(state)}\n\n`);
        push(opts.store.get());
        opts.store.on("change", push);
        req.on("close", () => opts.store.off("change", push));
      } else if (req.method === "GET" && url.pathname === "/api/config") {
        sendJson(res, opts.getConfig());
      } else if (req.method === "PUT" && url.pathname === "/api/config") {
        // Exigir JSON força um preflight CORS, que nunca respondemos: sites externos não conseguem alterar a config.
        if (!req.headers["content-type"]?.startsWith("application/json")) {
          res.writeHead(415).end();
          return;
        }
        const patch = JSON.parse(await readBody(req)) as Partial<AssistantConfig>;
        const next = mergeConfig(opts.getConfig(), patch);
        await opts.setConfig(next);
        sendJson(res, next);
      } else {
        res.writeHead(404).end();
      }
    } catch (err) {
      res.writeHead(400, { "Content-Type": "text/plain; charset=utf-8" }).end((err as Error).message);
    }
  });
  server.listen(opts.port, "127.0.0.1");
  return server;
}

function sendJson(res: http.ServerResponse, body: unknown): void {
  res.writeHead(200, { "Content-Type": "application/json; charset=utf-8" });
  res.end(JSON.stringify(body));
}

function readBody(req: http.IncomingMessage): Promise<string> {
  return new Promise((resolve, reject) => {
    let body = "";
    req.on("data", (chunk: Buffer) => {
      body += chunk.toString("utf8");
      if (body.length > 100_000) req.destroy(new Error("corpo muito grande"));
    });
    req.on("end", () => resolve(body));
    req.on("error", reject);
  });
}
