import { describe, expect, it } from "vitest";
import { parseCommandLine, parseLockfile } from "../src/lcu/credentials.js";

describe("credenciais do LCU", () => {
  it("lê o lockfile", () => {
    expect(parseLockfile("LeagueClient:12345:54321:abcDEF_123:https")).toEqual({
      pid: 12345,
      port: 54321,
      password: "abcDEF_123",
      protocol: "https",
    });
  });

  it("rejeita lockfile inválido", () => {
    expect(parseLockfile("")).toBeNull();
    expect(parseLockfile("LeagueClient:1:abc:pw:https")).toBeNull();
  });

  it("lê a linha de comando do LeagueClientUx", () => {
    const cmd = `"C:/Riot Games/League of Legends/LeagueClientUx.exe" "--riotclient-auth-token=xyz" "--app-port=61234" "--remoting-auth-token=Tok-en_9" "--app-pid=999"`;
    expect(parseCommandLine(cmd)).toEqual({ port: 61234, password: "Tok-en_9", protocol: "https" });
  });

  it("retorna null sem o processo", () => {
    expect(parseCommandLine("")).toBeNull();
  });
});
