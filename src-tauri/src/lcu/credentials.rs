use std::path::{Path, PathBuf};

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub port: u16,
    pub password: String,
}

/// Formato do lockfile: `LeagueClient:<pid>:<porta>:<senha>:<protocolo>`.
pub fn parse_lockfile(content: &str) -> Option<Credentials> {
    let parts: Vec<&str> = content.trim().split(':').collect();
    if parts.len() < 5 || parts[3].is_empty() {
        return None;
    }
    let port = parts[2].parse().ok().filter(|p| *p > 0)?;
    Some(Credentials {
        port,
        password: parts[3].to_string(),
    })
}

fn arg_value<'a>(args: &[&'a str], name: &str) -> Option<&'a str> {
    args.iter()
        .find_map(|a| a.trim_matches('"').strip_prefix(name)?.strip_prefix('='))
}

/// Lê porta e token dos argumentos do processo `LeagueClientUx`.
pub fn parse_args(args: &[&str]) -> Option<Credentials> {
    let port = arg_value(args, "--app-port")?.parse().ok()?;
    let password = arg_value(args, "--remoting-auth-token")?.to_string();
    (!password.is_empty()).then_some(Credentials { port, password })
}

/// Procura o cliente: primeiro pelo lockfile (barato), depois pela lista de processos.
#[derive(Default)]
pub struct Finder {
    system: Option<System>,
    /// Pasta de instalação descoberta pelo processo, para os próximos ciclos.
    install_dir: Option<PathBuf>,
}

impl Finder {
    pub fn find(&mut self) -> Option<Credentials> {
        self.find_lockfile().or_else(|| self.scan_processes())
    }

    fn install_dirs(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Ok(dir) = std::env::var("LOL_INSTALL_DIR") {
            dirs.push(dir.into());
        }
        dirs.extend(self.install_dir.clone());
        if cfg!(windows) {
            dirs.push(r"C:\Riot Games\League of Legends".into());
            dirs.push(r"D:\Riot Games\League of Legends".into());
        } else if cfg!(target_os = "macos") {
            dirs.push("/Applications/League of Legends.app/Contents/LoL".into());
        }
        dirs
    }

    fn find_lockfile(&self) -> Option<Credentials> {
        self.install_dirs().iter().find_map(|dir| {
            std::fs::read_to_string(Path::new(dir).join("lockfile"))
                .ok()
                .and_then(|c| parse_lockfile(&c))
        })
    }

    fn scan_processes(&mut self) -> Option<Credentials> {
        let system = self.system.get_or_insert_with(System::new);
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cmd(UpdateKind::OnlyIfNotSet),
        );
        for process in system.processes().values() {
            let name = process.name().to_string_lossy();
            if !name.starts_with("LeagueClientUx") || name.contains("Render") {
                continue;
            }
            let args: Vec<String> = process
                .cmd()
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            if let Some(dir) = arg_value(&args, "--install-directory") {
                self.install_dir = Some(dir.into());
            }
            if let Some(creds) = parse_args(&args) {
                return Some(creds);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_o_lockfile() {
        assert_eq!(
            parse_lockfile("LeagueClient:12345:54321:abcDEF_123:https"),
            Some(Credentials {
                port: 54321,
                password: "abcDEF_123".into()
            })
        );
        assert_eq!(parse_lockfile(""), None);
        assert_eq!(parse_lockfile("LeagueClient:1:abc:pw:https"), None);
    }

    #[test]
    fn le_os_argumentos_do_processo() {
        let args = [
            "C:/Riot Games/League of Legends/LeagueClientUx.exe",
            "--riotclient-auth-token=xyz",
            "\"--app-port=61234\"",
            "--remoting-auth-token=Tok-en_9",
            "--install-directory=C:/Riot Games/League of Legends",
        ];
        assert_eq!(
            parse_args(&args),
            Some(Credentials {
                port: 61234,
                password: "Tok-en_9".into()
            })
        );
        assert_eq!(
            arg_value(&args, "--install-directory"),
            Some("C:/Riot Games/League of Legends")
        );
        assert_eq!(parse_args(&[]), None);
    }
}
