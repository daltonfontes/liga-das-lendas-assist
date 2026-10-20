import { execFile } from "node:child_process";
import { readFile } from "node:fs/promises";
import { promisify } from "node:util";
import os from "node:os";
import path from "node:path";

const execFileAsync = promisify(execFile);

export interface LcuCredentials {
  port: number;
  password: string;
  pid?: number;
  protocol: string;
}

/** Formato do lockfile: `LeagueClient:<pid>:<porta>:<senha>:<protocolo>` */
export function parseLockfile(content: string): LcuCredentials | null {
  const parts = content.trim().split(":");
  if (parts.length < 5) return null;
  const [, pid, port, password, protocol] = parts;
  const portNum = Number(port);
  if (!Number.isInteger(portNum) || portNum <= 0 || !password) return null;
  return { pid: Number(pid), port: portNum, password, protocol: protocol ?? "https" };
}

/** Extrai porta e token da linha de comando do processo `LeagueClientUx`. */
export function parseCommandLine(commandLine: string): LcuCredentials | null {
  const port = /--app-port=["']?(\d+)/.exec(commandLine)?.[1];
  const password = /--remoting-auth-token=["']?([\w-]+)/.exec(commandLine)?.[1];
  if (!port || !password) return null;
  return { port: Number(port), password, protocol: "https" };
}

export function defaultInstallDirs(): string[] {
  const dirs: string[] = [];
  if (process.env.LOL_INSTALL_DIR) dirs.push(process.env.LOL_INSTALL_DIR);
  if (process.platform === "win32") {
    dirs.push("C:\\Riot Games\\League of Legends", "D:\\Riot Games\\League of Legends");
  } else if (process.platform === "darwin") {
    dirs.push("/Applications/League of Legends.app/Contents/LoL");
  } else {
    // Linux via Wine/Lutris — caminho comum, ajuste com LOL_INSTALL_DIR.
    dirs.push(path.join(os.homedir(), "Games/league-of-legends/drive_c/Riot Games/League of Legends"));
  }
  return dirs;
}

async function fromLockfile(dirs: string[]): Promise<LcuCredentials | null> {
  for (const dir of dirs) {
    try {
      const creds = parseLockfile(await readFile(path.join(dir, "lockfile"), "utf8"));
      if (creds) return creds;
    } catch {
      // lockfile ausente: cliente fechado ou instalado em outro lugar
    }
  }
  return null;
}

async function fromProcessList(): Promise<LcuCredentials | null> {
  try {
    let output: string;
    if (process.platform === "win32") {
      ({ stdout: output } = await execFileAsync(
        "powershell.exe",
        [
          "-NoProfile",
          "-Command",
          "Get-CimInstance Win32_Process -Filter \"name='LeagueClientUx.exe'\" | Select-Object -ExpandProperty CommandLine",
        ],
        { windowsHide: true },
      ));
    } else {
      ({ stdout: output } = await execFileAsync("ps", ["-A", "-o", "args="]));
      output = output
        .split("\n")
        .filter((line) => line.includes("LeagueClientUx"))
        .join("\n");
    }
    return parseCommandLine(output);
  } catch {
    return null;
  }
}

/** Procura as credenciais do cliente: primeiro pelo processo, depois pelo lockfile. */
export async function findCredentials(dirs = defaultInstallDirs()): Promise<LcuCredentials | null> {
  return (await fromProcessList()) ?? (await fromLockfile(dirs));
}
