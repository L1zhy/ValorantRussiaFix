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
    let mut found = false;
    let mut ok = true;
    for root in drive_roots() {
        println!("Поиск Zapret: {}", root.display());
        visit_lists(&root, |path| {
            found = true;
            if let Err(error) = update_list(path, apply) {
                eprintln!("Не удалось обработать {}: {error}", path.display());
                ok = false;
            }
        });
    }
    if !found {
        println!("Доступные установки Zapret со списком исключений не найдены.");
    }
    ok
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
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        if directory
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("lists"))
            && let Some(installation) = directory.parent()
        {
            let list = directory.join("list-exclude.txt");
            if is_plain_file(&list) && has_winws(installation) {
                handle(&list);
            }
        }
        if let Ok(entries) = fs::read_dir(&directory) {
            for entry in entries.flatten() {
                // Use metadata without following symlinks, junctions and other reparse points.
                if entry.file_type().is_ok_and(|kind| kind.is_dir())
                    && entry
                        .metadata()
                        .is_ok_and(|info| info.file_attributes() & REPARSE_POINT == 0)
                {
                    pending.push(entry.path());
                }
            }
        }
    }
}

fn has_winws(installation: &Path) -> bool {
    ["winws.exe", "winws2.exe"].into_iter().any(|name| {
        is_plain_file(&installation.join(name))
            || (fs::symlink_metadata(installation.join("bin"))
                .is_ok_and(|info| info.is_dir() && info.file_attributes() & REPARSE_POINT == 0)
                && is_plain_file(&installation.join("bin").join(name)))
    })
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

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let suffix = format!("riotfix-{}-{stamp}", std::process::id());
    let temp = path.with_extension(format!("txt.{suffix}.tmp"));
    let backup = path.with_extension(format!("txt.{suffix}.bak"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let write_result = file.write_all(&updated).and_then(|()| file.sync_all());
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
    println!("  Исключения Riot удалены: {}", path.display());
    println!("  Резервная копия: {}", backup.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
                fs::write(directory.join("winws.exe"), b"")?;
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
