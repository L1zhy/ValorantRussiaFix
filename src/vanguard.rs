use crate::{client, http, locale::text, windows};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;
use std::time::{Duration, SystemTime};

type Version = [u16; 4];
const STALL_SECONDS: f64 = 120.0;
const MAX_LOG_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const CDN: &str = "https://riot-client.secure.dyn.riotcdn.net/channels/public/rccontent/vanguard";

#[derive(Clone, Copy)]
pub enum Diagnosis {
    Current(Version),
    NoEvidence,
    Updating(Version),
    Stalled(Version),
}

pub fn display_version(version: Version) -> String {
    let [major, minor, build, revision] = version;
    format!("{major}.{minor}.{build}.{revision}")
}

fn version(value: &str) -> Option<Version> {
    let mut parts = value.split('.');
    let mut parsed = [0; 4];
    for part in &mut parsed {
        *part = parts.next()?.parse().ok()?;
    }
    parts.next().is_none().then_some(parsed)
}

#[derive(Default)]
struct History {
    target: Option<Version>,
    installed: Option<Version>,
    last_progress: f64,
    progress: Option<(String, u8)>,
    failed: bool,
}

impl History {
    fn read_line(&mut self, line: &str) {
        let Some((time, body)) = line.split_once('|') else {
            return;
        };
        let Ok(time) = time.trim().parse::<f64>() else {
            return;
        };
        if !time.is_finite() || time < 0.0 {
            return;
        }
        if let Some(value) = body
            .split_once("vanguard: Discovered vanguard version ")
            .and_then(|(_, value)| value.split_whitespace().next())
        {
            self.installed = version(value);
        }
        if let Some(value) = body
            .split_once("patch-proxy: Vanguard update started (installed: ")
            .and_then(|(_, value)| value.split_once(", available: "))
            .and_then(|(_, value)| value.strip_suffix(')'))
        {
            self.target = version(value);
            self.last_progress = time;
            self.progress = None;
            self.failed = false;
        }
        if self.target.is_none() {
            return;
        }
        if body.contains("Successfully installed third-party dependency 'vanguard'") {
            self.installed = self.target.take();
        } else if body.contains("agent: Dependency vanguard failed to install:") {
            self.failed = true;
        }
        if let Some((phase, progress)) = body
            .rsplit_once("Vanguard phase transition: ")
            .and_then(|(_, value)| value.rsplit_once(" -> "))
            .and_then(|(_, value)| value.split_once(", progress="))
            .and_then(|(phase, value)| Some((phase, value.parse::<u8>().ok()?)))
        {
            match phase {
                "Succeeded" => self.installed = self.target.take(),
                "Failed" | "Cancelled" | "Canceled" => self.failed = true,
                _ => self.advance(time, phase, progress),
            }
        }
        // Product progress is shared with Vanguard only in DependencyUpdate.
        if body.contains("productId: valorant, patchlineId: live, status: DependencyUpdate")
            && let Some(progress) = body
                .rsplit_once("progress: ")
                .and_then(|(_, value)| value.parse::<u8>().ok())
        {
            let phase = self
                .progress
                .as_ref()
                .map_or("", |(phase, _)| phase.as_str())
                .to_owned();
            if progress > self.progress.as_ref().map_or(0, |(_, amount)| *amount) {
                self.advance(time, &phase, progress);
            }
        }
    }

    fn advance(&mut self, time: f64, phase: &str, amount: u8) {
        if !self
            .progress
            .as_ref()
            .is_some_and(|(previous, value)| previous == phase && *value >= amount)
        {
            self.last_progress = time;
            self.progress = Some((phase.to_owned(), amount));
        }
    }

    fn diagnosis(&self, elapsed: f64, installed: Option<Version>) -> Diagnosis {
        if let Some(current) = installed
            && (self.target.is_some_and(|target| current >= target)
                || self.target.is_none() && self.installed.is_some())
        {
            return Diagnosis::Current(current);
        }
        match self.target {
            Some(target)
                if self.failed
                    || (elapsed - self.last_progress >= STALL_SECONDS
                        && self
                            .progress
                            .as_ref()
                            .is_some_and(|(phase, _)| phase == "Download")) =>
            {
                Diagnosis::Stalled(target)
            }
            Some(target) => Diagnosis::Updating(target),
            None => Diagnosis::NoEvidence,
        }
    }
}

pub fn diagnose() -> io::Result<Diagnosis> {
    let latest = match fs::read_dir(client::logs()?.join("Riot Client Logs")) {
        Ok(logs) => logs
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let metadata = entry.metadata().ok()?;
                (entry.path().extension()?.eq_ignore_ascii_case("log") && metadata.is_file())
                    .then_some((metadata.created().ok()?, entry.path()))
            })
            .max_by_key(|entry| entry.0),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Diagnosis::NoEvidence),
        Err(error) => return Err(error),
    };
    let Some((started, latest)) = latest else {
        return Ok(Diagnosis::NoEvidence);
    };
    let elapsed = SystemTime::now()
        .duration_since(started)
        .unwrap_or_default();
    if elapsed > MAX_LOG_AGE {
        return Ok(Diagnosis::NoEvidence);
    }
    const MAX_LOG_BYTES: u64 = 64 * 1024 * 1024;
    let file = File::open(latest)?;
    // Windows directory entries can report a stale size while Riot holds the log open.
    if file.metadata()?.len() > MAX_LOG_BYTES {
        return Ok(Diagnosis::NoEvidence);
    }
    let mut history = History::default();
    let mut reader = BufReader::new(file.take(MAX_LOG_BYTES));
    let mut line = Vec::new();
    while reader.read_until(b'\n', &mut line)? != 0 {
        history.read_line(String::from_utf8_lossy(&line).trim_end());
        line.clear();
    }
    let state = history.diagnosis(elapsed.as_secs_f64(), windows::vanguard_version());
    if let Diagnosis::Stalled(target) = state
        && !history.failed
        && riot_download_is_recent()
    {
        return Ok(Diagnosis::Updating(target));
    }
    Ok(state)
}

fn riot_download_is_recent() -> bool {
    let Some(data) = std::env::var_os("ProgramData") else {
        return false;
    };
    let setup = std::path::PathBuf::from(data).join(r"Riot Games\Metadata\vanguard\setup.exe");
    fs::metadata(setup)
        .and_then(|file| file.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age.as_secs_f64() < STALL_SECONDS)
}

pub enum Stage {
    Download {
        fraction: f32,
        bytes: u64,
        total: u64,
    },
    Verify,
    Install,
    Restart,
}

pub fn recover(target: Version, mut report: impl FnMut(Stage) -> bool) -> io::Result<bool> {
    let _lock = windows::vanguard_lock()?;
    let expected = |state| matches!(state, Diagnosis::Stalled(version) if version == target);
    let ensure_pending = || {
        if expected(diagnose()?) {
            Ok(())
        } else {
            Err(io::Error::other(text(
                "Состояние обновления изменилось. Повтори проверку.",
                "The update state changed. Run the check again.",
            )))
        }
    };
    client::ensure_no_game()?;
    ensure_pending()?;
    let executable = client::executable()?;
    let directory = tempfile::Builder::new()
        .prefix("riot-fix-vanguard-")
        .tempdir()?;
    let setup = directory.path().join("setup.exe");
    download(target, &setup, &mut report)?;

    if !report(Stage::Verify) {
        return Err(io::ErrorKind::Interrupted.into());
    }
    // Keep the verified file open without write or delete sharing until the installer exits.
    let installer = OpenOptions::new().read(true).share_mode(1).open(&setup)?;
    windows::verify_riot_signature(&setup, &installer)?;
    ensure_pending()?;
    if !report(Stage::Install) {
        return Err(io::ErrorKind::Interrupted.into());
    }
    client::stop(&executable)?;
    let result = windows::run_elevated(&setup, &[]).and_then(|code| {
        if matches!(code, 0 | 3010 | 1641)
            && windows::vanguard_version().is_some_and(|current| current >= target)
        {
            Ok(code != 0)
        } else {
            Err(io::Error::other(localized!(
                format,
                "Vanguard не подтвердил установку. Код установщика: {code}.",
                "Vanguard installation could not be verified. Installer exit code: {code}."
            )))
        }
    });
    drop(installer);
    report(Stage::Restart);
    let restart = client::start(&executable, true);
    let reboot_required = result?;
    restart.map(|_| reboot_required)
}

fn download(
    target: Version,
    path: &Path,
    report: &mut impl FnMut(Stage) -> bool,
) -> io::Result<()> {
    let url = format!("{CDN}/{}/setup.exe", display_version(target));
    let mut writer = File::create_new(path)?;
    http::download(&url, &mut writer, 600 * 1024 * 1024, 900, |total, bytes| {
        let fraction = if total > 0.0 {
            (bytes / total).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        report(Stage::Download {
            fraction,
            bytes: bytes as u64,
            total: total as u64,
        })
    })?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_busy_log_is_not_download_progress_and_an_installed_version_wins() {
        let mut state = History::default();
        state.read_line("000001.521| OKAY| patch-proxy: Vanguard update started (installed: 1.19.3.23, available: 1.19.3.24)");
        state.read_line("000001.522| OKAY| patch-proxy: Vanguard phase transition: Unknown -> Download, progress=0");
        state.read_line(
            "000119.500| WARN| patch-proxy: Dependency update does not include ProductPatchline",
        );
        assert!(matches!(
            state.diagnosis(120.0, None),
            Diagnosis::Updating(_)
        ));
        assert!(matches!(
            state.diagnosis(122.0, None),
            Diagnosis::Stalled([1, 19, 3, 24])
        ));
        assert!(matches!(
            state.diagnosis(122.0, Some([1, 19, 3, 25])),
            Diagnosis::Current([1, 19, 3, 25])
        ));
        state.read_line("000122.100| OKAY| patch-proxy: HandleProductPatchlineStatusUpdate: productId: valorant, patchlineId: live, status: DependencyUpdate, progress: 10");
        assert!(matches!(
            state.diagnosis(123.0, None),
            Diagnosis::Updating(_)
        ));
        state.read_line("000123.000| OKAY| patch-proxy: Vanguard phase transition: Download -> Install, progress=90");
        assert!(matches!(
            state.diagnosis(300.0, None),
            Diagnosis::Updating(_)
        ));
        state.read_line("000124.200| WARN| agent: Dependency vanguard failed to install: installation cancelled");
        assert!(matches!(
            state.diagnosis(125.0, None),
            Diagnosis::Stalled(_)
        ));
    }
}
