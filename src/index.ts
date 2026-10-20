import path from "node:path";
import { Assistant } from "./assistant.js";
import { loadConfig, saveConfig } from "./config.js";
import { startDashboard } from "./server/dashboard.js";
import { StateStore } from "./state.js";

const CONFIG_FILE = process.env.LOL_ASSIST_CONFIG ?? path.resolve("config.json");

let config = await loadConfig(CONFIG_FILE);
const store = new StateStore();
const assistant = new Assistant(store, () => config);

const port = Number(process.env.LOL_ASSIST_PORT ?? config.dashboard.port);
startDashboard({
  port,
  store,
  getConfig: () => config,
  setConfig: async (next) => {
    config = next;
    await saveConfig(CONFIG_FILE, next);
    store.log("Configuração salva.");
  },
});
console.log(`[assist] Painel: http://127.0.0.1:${port}`);
console.log(`[assist] Configuração: ${CONFIG_FILE}`);

process.on("SIGINT", () => {
  assistant.stop();
  process.exit(0);
});

await assistant.run();
