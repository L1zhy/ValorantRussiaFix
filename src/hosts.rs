use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::windows;

const MANAGED_TAG: &[u8] = b"# riot-fix";
const UTF8_BOM: &[u8] = b"\xef\xbb\xbf";

/// Updates managed entries and returns the number of entries in the resulting file.
pub fn apply(pins: &BTreeMap<String, Ipv4Addr>) -> io::Result<usize> {
    update_system_hosts(pins, MANAGED_TAG)
}

pub fn apply_voice(ip: Option<Ipv4Addr>) -> io::Result<()> {
    let pins = ip
        .into_iter()
        .map(|ip| (crate::network::VOICE_HOST.to_owned(), ip))
        .collect();
    let count = update_system_hosts(&pins, b"# riot-fix-voice")?;
    if ip.is_some() && count == 0 {
        return Err(io::Error::other("Existing hosts entry takes precedence"));
    }
    Ok(())
}

fn update_system_hosts(pins: &BTreeMap<String, Ipv4Addr>, tag: &[u8]) -> io::Result<usize> {
    let root = std::env::var_os("SystemRoot").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            crate::locale::text("SystemRoot отсутствует", "SystemRoot is missing"),
        )
    })?;
    apply_with_tag(
        &PathBuf::from(root).join(r"System32\drivers\etc\hosts"),
        pins,
        tag,
    )
}

fn apply_with_tag(path: &Path, pins: &BTreeMap<String, Ipv4Addr>, tag: &[u8]) -> io::Result<usize> {
    let original = match fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let (updated, pinned) = render(original.as_deref().unwrap_or_default(), pins, tag);
    if original.as_deref().unwrap_or_default() == updated {
        return Ok(pinned);
    }

    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let stem = format!("hosts.riotfix-{}-{suffix}", std::process::id());
    let temporary = path.with_file_name(format!("{stem}.tmp"));
    let backup = path.with_file_name(format!("{stem}.bak"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let written = file.write_all(&updated).and_then(|()| file.sync_all());
    drop(file);
    let result = written.and_then(|()| {
        if let Some(original) = original {
            if fs::read(path)? != original {
                return Err(io::Error::other(crate::locale::text(
                    "hosts изменился во время проверки. Повторите запуск.",
                    "The hosts file changed during the check. Run Riot Fix again.",
                )));
            }
            windows::replace_file(path, &temporary, &backup)?;
            localized!(
                println,
                "Резервная копия hosts: {}",
                "Hosts backup: {}",
                backup.display()
            );
        } else {
            // A hard link publishes the whole file and fails if another process created hosts.
            fs::hard_link(&temporary, path)?;
        }
        Ok(pinned)
    });
    let _ = fs::remove_file(&temporary);
    result
}

fn render(original: &[u8], pins: &BTreeMap<String, Ipv4Addr>, tag: &[u8]) -> (Vec<u8>, usize) {
    let mut output = Vec::with_capacity(original.len());
    let original = if let Some(rest) = original.strip_prefix(UTF8_BOM) {
        output.extend_from_slice(UTF8_BOM);
        rest
    } else {
        original
    };
    let mut manual = BTreeSet::new();

    for line in original.split_inclusive(|&byte| byte == b'\n') {
        let value = line.trim_ascii();
        let comment_start = value.iter().position(|&byte| byte == b'#');
        let entry = comment_start.map_or(value, |position| &value[..position]);
        let mut fields = entry
            .split(|byte| byte.is_ascii_whitespace())
            .filter(|v| !v.is_empty());
        let is_address = fields
            .next()
            .and_then(|part| std::str::from_utf8(part).ok())
            .is_some_and(|part| part.parse::<IpAddr>().is_ok());
        if is_address {
            let names: Vec<_> = fields.collect();
            if names.len() == 1 && comment_start.is_some_and(|start| &value[start..] == tag) {
                continue;
            }
            if tag == MANAGED_TAG
                && pins.contains_key(crate::network::PERMISSIONS)
                && names.len() == 1
                && names[0].eq_ignore_ascii_case(crate::network::PERMISSIONS.as_bytes())
                && comment_start
                    .is_some_and(|start| &value[start..] == b"# riot-fix-permissions-test")
            {
                continue;
            }
            manual.extend(names.into_iter().map(<[u8]>::to_ascii_lowercase));
        }
        output.extend_from_slice(line);
    }

    let line_ending: &[u8] =
        if original.contains(&b'\n') && !original.windows(2).any(|pair| pair == b"\r\n") {
            b"\n"
        } else {
            b"\r\n"
        };
    let mut count = 0;
    for (domain, ip) in pins {
        if manual.contains(&domain.as_bytes().to_ascii_lowercase()) {
            localized!(
                println,
                "Существующая пользовательская запись сохранена: {domain}",
                "Kept the existing user entry: {domain}"
            );
            continue;
        }
        if !output.is_empty() && output != UTF8_BOM && !output.ends_with(b"\n") {
            output.extend_from_slice(line_ending);
        }
        output.extend_from_slice(format!("{ip} {domain} ").as_bytes());
        output.extend_from_slice(tag);
        output.extend_from_slice(line_ending);
        count += 1;
    }
    (output, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply_to(path: &Path, pins: &BTreeMap<String, Ipv4Addr>) -> io::Result<usize> {
        apply_with_tag(path, pins, MANAGED_TAG)
    }

    #[test]
    fn voice_entry_can_be_removed_without_touching_other_entries() {
        let original = b"192.0.2.1 auth.example # riot-fix\r\n";
        let pins = BTreeMap::from([(
            crate::network::VOICE_HOST.into(),
            Ipv4Addr::new(192, 0, 2, 2),
        )]);
        let (updated, count) = render(original, &pins, b"# riot-fix-voice");
        assert_eq!(count, 1);
        assert!(updated.starts_with(original));
        assert_eq!(
            render(&updated, &BTreeMap::new(), b"# riot-fix-voice").0,
            original
        );
        assert_eq!(render(&updated, &pins, b"# riot-fix-voice"), (updated, 1));
    }

    #[test]
    fn updates_only_managed_entries_and_keeps_existing_aliases() {
        let original = b"\xef\xbb\xbf# \xff\n127.0.0.1 CUSTOM.example second.example\n192.0.2.1 stale.example # riot-fix\n# riot-fix marker\n";
        let pins = BTreeMap::from([
            ("custom.example".into(), Ipv4Addr::new(192, 0, 2, 2)),
            ("new.example".into(), Ipv4Addr::new(192, 0, 2, 3)),
        ]);
        let (updated, count) = render(original, &pins, MANAGED_TAG);
        assert_eq!(count, 1);
        assert_eq!(updated, b"\xef\xbb\xbf# \xff\n127.0.0.1 CUSTOM.example second.example\n# riot-fix marker\n192.0.2.3 new.example # riot-fix\n");
        assert_eq!(render(&updated, &pins, MANAGED_TAG), (updated, count));
    }

    #[test]
    fn replacement_preserves_original_backup_and_repeated_run_is_a_noop() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("riot-fix-test-{}-{suffix}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("hosts");
        let original = b"# original\r\n";
        fs::write(&path, original).unwrap();
        let pins = BTreeMap::from([("new.example".into(), Ipv4Addr::new(192, 0, 2, 3))]);
        assert_eq!(apply_to(&path, &pins).unwrap(), 1);
        assert_eq!(apply_to(&path, &pins).unwrap(), 1);
        assert_eq!(
            fs::read(&path).unwrap(),
            b"# original\r\n192.0.2.3 new.example # riot-fix\r\n"
        );
        let alternatives: Vec<_> = fs::read_dir(&directory)
            .unwrap()
            .map(|item| item.unwrap().path())
            .filter(|entry| entry != &path)
            .collect();
        assert_eq!(alternatives.len(), 1);
        assert_eq!(fs::read(&alternatives[0]).unwrap(), original);
        fs::remove_file(&alternatives[0]).unwrap();
        fs::remove_file(&path).unwrap();
        fs::remove_dir(&directory).unwrap();
    }

    #[test]
    fn preserves_unterminated_file_without_modifications() {
        let original = b"127.0.0.1 localhost";
        assert_eq!(
            render(original, &BTreeMap::new(), MANAGED_TAG),
            (original.to_vec(), 0)
        );
    }
}
