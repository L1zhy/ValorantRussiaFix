use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const REPARSE_POINT: u32 = 0x400;
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetLogicalDrives() -> u32;
    fn GetDriveTypeW(root: *const u16) -> u32;
}

pub fn run(apply: bool) -> bool {
    let mut candidates = Vec::new();
    for root in drive_roots() {
        println!("Поиск Zapret: {}", root.display());
        visit_lists(&root, |path| candidates.push(path.to_owned()));
    }
    candidates.sort_by_key(|path| path.to_string_lossy().to_lowercase());
    candidates.dedup_by(|a, b| {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    });
    candidates.sort_by_key(|path| version_key(path));
    let Some(path) = candidates.pop() else {
        println!("Установки Zapret не найдены. Обновите или установите Zapret.");
        return false;
    };
    println!(
        "Выбрана самая новая копия из {}: {}",
        candidates.len() + 1,
        path.display()
    );
    warn_old_zapret(&path);
    if let Err(error) = update_list(&path, apply) {
        eprintln!("Не удалось обработать {}: {error}", path.display());
        return false;
    }
    if apply {
        apply_settings(&path, &mut None)
    } else {
        println!("Можно включить Game Filter, Auto-Update Check и обновить IPSet.");
        true
    }
}

fn version_key(list: &Path) -> (Vec<u64>, String) {
    let service = list
        .parent()
        .and_then(Path::parent)
        .and_then(|root| fs::read_to_string(root.join("service.bat")).ok())
        .unwrap_or_default();
    let version = service
        .lines()
        .find_map(|line| {
            line.split_once("LOCAL_VERSION=")
                .map(|(_, value)| value.trim().trim_matches('"').to_owned())
        })
        .unwrap_or_default();
    let numbers = version
        .split('.')
        .map(|part| {
            part.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .unwrap_or(0)
        })
        .collect();
    (numbers, version)
}

fn skip_directory(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "windows"
            | "$recycle.bin"
            | "system volume information"
            | "appdata"
            | "node_modules"
            | ".git"
            | "target"
            | "cache"
            | "caches"
            | ".cache"
            | "winsxs"
            | "programdata"
            | "go"
            | ".cargo"
            | ".rustup"
            | ".venv"
            | "venv"
            | "__pycache__"
            | "work"
    )
}
fn warn_old_zapret(list: &Path) {
    let Some(root) = list.parent().and_then(Path::parent) else {
        return;
    };
    let service = fs::read_to_string(root.join("service.bat")).unwrap_or_default();
    let supports_settings = service.contains("GameFilterTCP")
        && service.contains("check_updates.enabled")
        && service.contains("ipset-service.txt");
    let version = service.lines().find_map(|line| {
        let (_, rest) = line.split_once("LOCAL_VERSION=")?;
        Some(rest.trim().trim_matches('"').to_owned())
    });
    if let Some(version) = version {
        println!("  Версия Zapret: {version} ({})", root.display());
        if version.starts_with("1.9.") {
            eprintln!("  Обновите Zapret. Найдена старая версия {version}.");
            eprintln!("  https://github.com/Flowseal/zapret-discord-youtube/releases/latest");
        }
    }
    if !supports_settings {
        eprintln!("  Обновите Zapret: эта копия не поддерживает все необходимые настройки.");
        eprintln!("  https://github.com/Flowseal/zapret-discord-youtube/releases/latest");
    }
}

fn drive_roots() -> impl Iterator<Item = PathBuf> {
    // GetLogicalDrives returns a bit mask from A: through Z:.
    let drives = unsafe { GetLogicalDrives() };
    (0..26).filter_map(move |offset| {
        if drives & (1 << offset) == 0 {
            return None;
        }
        let letter = b'A' + offset;
        let root = [u16::from(letter), b':' as u16, b'\\' as u16, 0];
        // The buffer is a NUL-terminated drive root. Search removable and fixed disks.
        if !matches!(unsafe { GetDriveTypeW(root.as_ptr()) }, 2 | 3) {
            return None;
        }
        let path = PathBuf::from(format!("{}:\\", char::from(letter)));
        path.is_dir().then_some(path)
    })
}

fn visit_lists(root: &Path, mut handle: impl FnMut(&Path)) {
    let mut pending = std::collections::VecDeque::from([(root.to_path_buf(), 0usize)]);
    if let Ok(users) = fs::read_dir(root.join("Users")) {
        for user in users.flatten() {
            for name in ["Downloads", "Desktop", "Documents", "OneDrive"] {
                let common = user.path().join(name);
                if common.is_dir() {
                    pending.push_back((common, 0));
                }
            }
        }
    }
    let mut visited = 0;
    let started = std::time::Instant::now();
    while let Some((directory, depth)) = pending.pop_front() {
        visited += 1;
        if visited > 15000 || started.elapsed().as_secs() >= 4 {
            println!("Достигнут предел поиска в {}.", root.display());
            break;
        }
        let list = directory.join("lists").join("list-exclude.txt");
        let service = directory.join("service.bat");
        if is_plain_file(&list)
            && is_plain_file(&service)
            && directory.join("utils").is_dir()
            && has_winws(&directory)
        {
            let contents = fs::read_to_string(service).unwrap_or_default();
            if contents.contains("LOCAL_VERSION=") && contents.contains("game_filter.enabled") {
                handle(&list);
                continue;
            }
        }
        if depth >= 4 {
            continue;
        }
        if let Ok(entries) = fs::read_dir(&directory) {
            for entry in entries.flatten() {
                if skip_directory(&entry.file_name().to_string_lossy()) {
                    continue;
                }
                if entry.file_type().is_ok_and(|kind| kind.is_dir())
                    && entry
                        .metadata()
                        .is_ok_and(|info| info.file_attributes() & REPARSE_POINT == 0)
                {
                    pending.push_back((entry.path(), depth + 1));
                }
            }
        }
    }
}
fn has_winws(installation: &Path) -> bool {
    ["winws.exe", "winws2.exe"].into_iter().any(|name| {
        is_executable(&installation.join(name))
            || (fs::symlink_metadata(installation.join("bin"))
                .is_ok_and(|info| info.is_dir() && info.file_attributes() & REPARSE_POINT == 0)
                && is_executable(&installation.join("bin").join(name)))
    })
}

fn is_executable(path: &Path) -> bool {
    is_plain_file(path) && fs::metadata(path).is_ok_and(|info| info.len() > 0)
}

fn is_plain_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|info| info.is_file() && info.file_attributes() & REPARSE_POINT == 0)
}

fn without_riot_entries(original: &[u8]) -> Vec<u8> {
    let mut filtered = Vec::with_capacity(original.len());
    let body = if let Some(body) = original.strip_prefix(UTF8_BOM) {
        filtered.extend_from_slice(UTF8_BOM);
        body
    } else {
        original
    };
    for line in body.split_inclusive(|&byte| byte == b'\n') {
        let entry = line.trim_ascii();
        if !entry.eq_ignore_ascii_case(b"riotgames.com")
            && !entry.eq_ignore_ascii_case(b"riotcdn.net")
        {
            filtered.extend_from_slice(line);
        }
    }
    filtered
}

fn update_list(path: &Path, apply: bool) -> io::Result<()> {
    let original = fs::read(path)?;
    let updated = without_riot_entries(&original);
    if original == updated {
        println!("  Уже настроен: {}", path.display());
        return Ok(());
    }
    if !apply {
        println!("  Можно удалить исключения Riot: {}", path.display());
        return Ok(());
    }

    replace_with_backup(path, &original, &updated)?;
    println!("  Исключения Riot удалены: {}", path.display());
    Ok(())
}

fn replace_with_backup(path: &Path, original: &[u8], updated: &[u8]) -> io::Result<()> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let suffix = format!("riotfix-{}-{stamp}", std::process::id());
    let mut stem = path
        .file_name()
        .expect("settings file has a name")
        .to_os_string();
    stem.push(format!(".{suffix}"));
    let mut temporary_name = stem.clone();
    temporary_name.push(".tmp");
    stem.push(".bak");
    let temp = path.with_file_name(temporary_name);
    let backup = path.with_file_name(stem);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let write_result = file.write_all(updated).and_then(|()| file.sync_all());
    drop(file);
    let result = write_result.and_then(|()| {
        if fs::read(path)? != original {
            return Err(io::Error::other("список изменился во время обработки"));
        }
        crate::windows::replace_file(path, &temp, &backup)
    });
    if let Err(error) = result {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    println!("  Резервная копия: {}", backup.display());
    Ok(())
}

const IPSET_URL: &str = "https://raw.githubusercontent.com/Flowseal/zapret-discord-youtube/refs/heads/main/.service/ipset-service.txt";

fn download_ipset() -> Result<Vec<u8>, String> {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    let response = std::process::Command::new(PathBuf::from(root).join(r"System32\curl.exe"))
        .args([
            "--disable",
            "--noproxy",
            "*",
            "--fail",
            "--silent",
            "--show-error",
            "--connect-timeout",
            "8",
            "--max-time",
            "30",
            "--max-filesize",
            "4194304",
            IPSET_URL,
        ])
        .output()
        .map_err(|error| error.to_string())?;
    if !response.status.success() {
        return Err(String::from_utf8_lossy(&response.stderr).trim().to_owned());
    }
    if !valid_ipset(&response.stdout) {
        return Err("источник вернул некорректный список IPSet".into());
    }
    Ok(response.stdout)
}

fn valid_ipset(data: &[u8]) -> bool {
    use std::net::IpAddr;
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let mut found = false;
    for entry in text.trim_start_matches('\u{feff}').lines().map(str::trim) {
        if entry.is_empty() || entry.starts_with('#') {
            continue;
        }
        if entry == "203.0.113.113/32" {
            return false;
        }
        let valid = if let Some((address, prefix)) = entry.split_once('/') {
            match (address.parse::<IpAddr>(), prefix.parse::<u8>()) {
                (Ok(IpAddr::V4(_)), Ok(bits)) => bits <= 32,
                (Ok(IpAddr::V6(_)), Ok(bits)) => bits <= 128,
                _ => false,
            }
        } else {
            entry.parse::<IpAddr>().is_ok()
        };
        if !valid {
            return false;
        }
        found = true;
    }
    found
}

fn configure_game_filter(path: &Path) -> io::Result<()> {
    let original = read_existing(path)?;
    // Both legacy and current menus accept "all". Current menus also read tcp=/udp=.
    let mut settings = b"all\r\n".to_vec();
    if let Some(data) = &original {
        for line in data.split(|byte| *byte == b'\n').map(<[u8]>::trim_ascii) {
            if line.get(..4).is_some_and(|prefix| {
                prefix.eq_ignore_ascii_case(b"tcp=") || prefix.eq_ignore_ascii_case(b"udp=")
            }) {
                settings.extend_from_slice(line);
                settings.extend_from_slice(b"\r\n");
            }
        }
    }
    write_settings(path, original.as_deref(), &settings)
}

fn enable_update_check(path: &Path) -> io::Result<()> {
    if read_existing(path)?.is_none() {
        write_settings(path, None, b"ENABLED\r\n")?;
    }
    Ok(())
}

fn read_existing(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs::symlink_metadata(path) {
        Ok(info) if info.is_file() && info.file_attributes() & REPARSE_POINT == 0 => {
            fs::read(path).map(Some)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
        Ok(_) => Err(io::Error::other("ожидался обычный файл")),
    }
}

fn write_settings(path: &Path, original: Option<&[u8]>, contents: &[u8]) -> io::Result<()> {
    if original == Some(contents) {
        return Ok(());
    }
    if let Some(previous) = original {
        replace_with_backup(path, previous, contents)
    } else {
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        let result = file.write_all(contents).and_then(|()| file.sync_all());
        drop(file);
        if result.is_err() {
            let _ = fs::remove_file(path);
        }
        result
    }
}

fn apply_settings(list: &Path, download: &mut Option<Result<Vec<u8>, String>>) -> bool {
    let lists = list.parent().expect("discovered list has a parent");
    let root = lists
        .parent()
        .expect("discovered lists directory has a parent");
    let utils = root.join("utils");
    let mut ok = true;
    let mut report = |name: &str, result: io::Result<()>| match result {
        Ok(()) => println!("  {name}"),
        Err(error) => {
            ok = false;
            eprintln!("  {name}: {error}");
        }
    };
    let usable_utils = fs::symlink_metadata(&utils)
        .is_ok_and(|info| info.is_dir() && info.file_attributes() & REPARSE_POINT == 0);
    if usable_utils {
        report(
            "Game Filter: TCP и UDP",
            configure_game_filter(&utils.join("game_filter.enabled")),
        );
        report(
            "Auto-Update Check: enabled",
            enable_update_check(&utils.join("check_updates.enabled")),
        );
    } else {
        report(
            "Game Filter / Auto-Update Check",
            Err(io::Error::other("нет доступного каталога utils")),
        );
    }
    let ipset = lists.join("ipset-all.txt");
    let result = match download.get_or_insert_with(download_ipset) {
        Ok(data) => read_existing(&ipset)
            .and_then(|original| write_settings(&ipset, original.as_deref(), data)),
        Err(error) => Err(io::Error::other(error.clone())),
    };
    report("IPSet Filter: loaded / Update IPSet List", result);
    ok
}

#[cfg(test)]
mod discovery_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_all_settings_and_rejects_invalid_ipset() -> io::Result<()> {
        let root =
            std::env::temp_dir().join(format!("riot-fix-settings-test-{}", std::process::id()));
        let utils = root.join("utils");
        let lists = root.join("lists");
        fs::create_dir_all(&utils)?;
        fs::create_dir_all(&lists)?;
        let game = utils.join("game_filter.enabled");
        let ipset = lists.join("ipset-all.txt");
        let list = lists.join("list-exclude.txt");
        fs::write(&game, b"mode=tcp\r\ntcp=27000-27100\r\n")?;
        fs::write(&ipset, b"203.0.113.113/32\r\n")?;
        let fresh = b"1.0.0.0/24\n2001:db8::/32\n";
        assert!(valid_ipset(fresh));
        assert!(!valid_ipset(b"203.0.113.113/32\n"));
        assert!(!valid_ipset(b"<!doctype html>"));
        assert!(apply_settings(&list, &mut Some(Ok(fresh.to_vec()))));
        assert_eq!(fs::read(&game)?, b"all\r\ntcp=27000-27100\r\n");
        assert!(utils.join("check_updates.enabled").is_file());
        assert_eq!(fs::read(&ipset)?, fresh);
        assert!(apply_settings(&list, &mut Some(Ok(fresh.to_vec()))));
        assert!(!apply_settings(&list, &mut Some(Err("offline".into()))));
        assert_eq!(fs::read(&ipset)?, fresh);
        fs::remove_dir_all(root)
    }

    #[test]
    fn updates_only_valid_installations_and_keeps_exact_backups() -> io::Result<()> {
        let root =
            std::env::temp_dir().join(format!("riot-fix-zapret-test-{}", std::process::id()));
        let original = b"\xef\xbb\xbfRIOTGAMES.COM\r\nkeep.example\r\n# riotcdn.net\nriotcdn.net";
        let filtered = b"\xef\xbb\xbfkeep.example\r\n# riotcdn.net\n";
        for name in ["Zapret", "nested\\another folder", "unrelated"] {
            let directory = root.join(name);
            fs::create_dir_all(directory.join("lists"))?;
            if name != "unrelated" {
                fs::write(directory.join("winws.exe"), b"MZ")?;
                fs::create_dir_all(directory.join("utils"))?;
                fs::write(
                    directory.join("service.bat"),
                    b"LOCAL_VERSION=1.10.3\ngame_filter.enabled",
                )?;
            }
            fs::write(directory.join("lists\\list-exclude.txt"), original)?;
        }
        let mut found = Vec::new();
        visit_lists(&root, |path| found.push(path.to_owned()));
        assert_eq!(found.len(), 2);
        for path in found {
            update_list(&path, false)?;
            assert_eq!(fs::read(&path)?, original);
            update_list(&path, true)?;
            update_list(&path, true)?;
            assert_eq!(fs::read(&path)?, filtered);
            let entries: Vec<_> =
                fs::read_dir(path.parent().unwrap())?.collect::<Result<_, _>>()?;
            assert_eq!(entries.len(), 2);
            let backup = entries
                .into_iter()
                .find(|entry| entry.path().extension().is_some_and(|ext| ext == "bak"))
                .unwrap();
            assert_eq!(fs::read(backup.path())?, original);
        }
        assert_eq!(
            fs::read(root.join("unrelated\\lists\\list-exclude.txt"))?,
            original
        );
        fs::remove_dir_all(root)
    }
}
