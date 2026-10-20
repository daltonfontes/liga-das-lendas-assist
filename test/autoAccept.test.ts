import { afterEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_CONFIG } from "../src/config.js";
import { AutoAccept, shouldAccept } from "../src/features/autoAccept.js";
import { LcuClient } from "../src/lcu/client.js";
import { StateStore } from "../src/state.js";

describe("auto aceitar", () => {
  afterEach(() => vi.useRealTimers());

  it("só aceita quando a checagem está em andamento e sem resposta", () => {
    expect(shouldAccept({ state: "InProgress", playerResponse: "None" })).toBe(true);
    expect(shouldAccept({ state: "InProgress", playerResponse: "Declined" })).toBe(false);
    expect(shouldAccept({ state: "Invalid", playerResponse: "None" })).toBe(false);
    expect(shouldAccept(undefined)).toBe(false);
  });

  it("aceita uma vez após o atraso, a partir de um evento do WebSocket", async () => {
    vi.useFakeTimers();
    vi.spyOn(console, "log").mockImplementation(() => {});
    const lcu = new LcuClient({ port: 1, password: "x", protocol: "https" });
    const post = vi.spyOn(lcu, "post").mockResolvedValue(undefined);
    new AutoAccept(lcu, new StateStore(), () => DEFAULT_CONFIG);

    const event = JSON.stringify([8, "OnJsonApiEvent", { uri: "/lol-matchmaking/v1/ready-check", eventType: "Update", data: { state: "InProgress", playerResponse: "None" } }]);
    lcu.dispatch(event);
    lcu.dispatch(event);
    expect(post).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(DEFAULT_CONFIG.autoAccept.delayMs);
    expect(post).toHaveBeenCalledTimes(1);
    expect(post).toHaveBeenCalledWith("/lol-matchmaking/v1/ready-check/accept");
  });

  it("cancela se o jogador recusar antes", async () => {
    vi.useFakeTimers();
    vi.spyOn(console, "log").mockImplementation(() => {});
    const lcu = new LcuClient({ port: 1, password: "x", protocol: "https" });
    const post = vi.spyOn(lcu, "post").mockResolvedValue(undefined);
    const accept = new AutoAccept(lcu, new StateStore(), () => DEFAULT_CONFIG);
    accept.handle({ state: "InProgress", playerResponse: "None" });
    accept.handle({ state: "InProgress", playerResponse: "Declined" });
    await vi.advanceTimersByTimeAsync(10_000);
    expect(post).not.toHaveBeenCalled();
  });
});
