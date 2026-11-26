use crate::{locale::text, windows};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant, SystemTime};

const GAME_NAMES: &[&str] = &[
    "VALORANT.exe",
    "VALORANT-Win64-Shipping.exe",
    "LeagueClient.exe",
    "LeagueClientUx.exe",
    "LeagueClientUxRender.exe",
    "League of Legends.exe",
    "LoR.exe",
    "2XKO.exe",
    "2XKO-Win64-Shipping.exe",
];
const CLIENT_NAMES: &[&str] = &[
    "RiotClientServices.exe",
    "RiotClientUx.exe",
    "RiotClientUxRender.exe",
    "Riot Client.exe",
];

pub fn logs() -> io::Result<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(|folder| PathBuf::from(folder).join(r"Riot Games\Riot Client\Logs"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA"))
}

pub fn executable() -> io::Result<PathBuf> {
    let base = std::env::var_os("ProgramData").unwrap_or_else(|| r"C:\ProgramData".into());
    let data = fs::read(PathBuf::from(base).join(r"Riot Games\RiotClientInstalls.json"))?;
    let config: serde_json::Value = serde_json::from_slice(&data)?;
    ["rc_default", "rc_live"]
        .into_iter()
        .filter_map(|key| config[key].as_str())
        .map(PathBuf::from)
        .find(|path| {
            path.is_absolute()
                && path.is_file()
                && path
                    .file_name()
                    .is_some_and(|name| name.eq_ignore_ascii_case("RiotClientServices.exe"))
        })
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                text(
                    "Не удалось найти Riot Client.",
                    "Could not locate Riot Client.",
                ),
            )
        })
}

pub fn ensure_no_game() -> io::Result<()> {
    if windows::session_processes()?
        .iter()
        .any(|process| process.matches(GAME_NAMES))
    {
        return Err(io::Error::other(text(
            "Закрой запущенную игру Riot и повтори попытку.",
            "Close the running Riot game and try again.",
        )));
    }
    Ok(())
}

pub fn stop(executable: &Path) -> io::Result<()> {
    ensure_no_game()?;
    let directory = executable.parent().expect("absolute client executable");
    let clients: Vec<_> = windows::session_processes()?
        .into_iter()
        .filter(|process| process.matches(CLIENT_NAMES))
        .collect();
    let mut handles = Vec::new();
    for process in clients {
        if let Some(handle) = process.open_for_restart()?
            && handle
                .image()
                .is_ok_and(|path| windows::same_path_prefix(&path, directory))
        {
            handles.push((process.id, handle));
        }
    }
    for (pid, handle) in &handles {
        handle.close_windows(*pid);
    }
    if !handles.is_empty() {
        std::thread::sleep(Duration::from_secs(2));
    }
    ensure_no_game()?;
    for (_, handle) in handles {
        handle.stop()?;
    }
    Ok(())
}

pub fn start(executable: &Path, valorant: bool) -> io::Result<Child> {
    let mut command = Command::new(executable);
    command.current_dir(executable.parent().expect("client directory"));
    if valorant {
        command.args(["--launch-product=valorant", "--launch-patchline=live"]);
    }
    command.spawn()
}

pub fn restart() -> io::Result<()> {
    let executable = executable()?;
    stop(&executable)?;
    let started = SystemTime::now();
    let mut service = start(&executable, false)?;
    let electron = executable.with_file_name(r"RiotClientElectron\Riot Client.exe");
    if !electron.is_file() {
        return Ok(());
    }
    let log_dir = logs()?.join("Riot Client Electron Logs");
    let mut deadline = Instant::now() + Duration::from_secs(110);
    let mut retried = false;
    localized!(
        println,
        "Ожидаем подключения окна Riot Client к локальной службе…",
        "Waiting for the Riot Client window to connect to its local service…"
    );
    while Instant::now() < deadline && service.try_wait()?.is_none() {
        match electron_state(&log_dir, started, service.id()) {
            Some(ElectronState::Ready) => {
                localized!(
                    println,
                    "Интерфейс Riot Client подключился к локальной службе.",
                    "The Riot Client interface connected to its local service."
                );
                return Ok(());
            }
            Some(ElectronState::Disconnected(port))
                if !retried && windows::owns_loopback_port(service.id(), port) =>
            {
                retried = true;
                if restart_electron(service.id(), &electron)? {
                    deadline = Instant::now() + Duration::from_secs(45);
                }
            }
            _ => {}
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        text(
            "Окно Riot Client не подтвердило подключение к локальной службе.",
            "The Riot Client window did not confirm its connection to the local service.",
        ),
    ))
}

enum ElectronState {
    Ready,
    Disconnected(u16),
}

fn electron_state(dir: &Path, started: SystemTime, pid: u32) -> Option<ElectronState> {
    let mut logs: Vec<_> = fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            let created = metadata.created().ok()?;
            (created >= started && metadata.is_file() && metadata.len() < 8 * 1024 * 1024)
                .then_some((created, entry.path()))
        })
        .collect();
    logs.sort_unstable_by_key(|entry| std::cmp::Reverse(entry.0));
    for (_, path) in logs {
        let Ok(log) = fs::read_to_string(path) else {
            continue;
        };
        let field = |name| {
            log.lines().find_map(|line| {
                line.split_once(name)?
                    .1
                    .trim()
                    .split(|c: char| !c.is_ascii_digit())
                    .next()
            })
        };
        if field("appPid:").and_then(|value| value.parse().ok()) != Some(pid) {
            continue;
        }
        if log.contains("(DataBinding) created") {
            return Some(ElectronState::Ready);
        }
        if log.contains("(SocketManager) onReconnectFailed") {
            return field("appPort:")
                .and_then(|value| value.parse().ok())
                .map(ElectronState::Disconnected);
        }
        break;
    }
    None
}

fn restart_electron(parent: u32, executable: &Path) -> io::Result<bool> {
    use std::os::windows::process::CommandExt;
    ensure_no_game()?;
    for process in windows::session_processes()? {
        if process.parent != parent || !process.matches(&["Riot Client.exe"]) {
            continue;
        }
        if let Some(handle) = process.open_for_restart()?
            && handle.image().is_ok_and(|path| {
                windows::same_path_prefix(&path, executable)
                    && path.components().count() == executable.components().count()
            })
            && let Some(args) = handle.command_arguments()
        {
            handle.stop()?;
            Command::new(executable)
                .raw_arg(args)
                .current_dir(executable.parent().expect("electron directory"))
                .spawn()?;
            return Ok(true);
        }
    }
    Ok(false)
}
