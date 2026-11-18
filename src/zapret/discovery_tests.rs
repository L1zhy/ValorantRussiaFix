use super::*;

fn installation(root: &Path, relative: &str, version: &str) -> io::Result<PathBuf> {
    let directory = root.join(relative);
    fs::create_dir_all(directory.join("utils"))?;
    fs::create_dir_all(directory.join("lists"))?;
    fs::write(directory.join("winws.exe"), b"MZ-fixture")?;
    fs::write(
        directory.join("service.bat"),
        format!("set \"LOCAL_VERSION={version}\"\r\nrem game_filter.enabled"),
    )?;
    let list = directory.join("lists/list-exclude.txt");
    fs::write(&list, b"riotgames.com\r\nriotcdn.net\r\nkeep.example\r\n")?;
    Ok(list)
}

#[test]
fn discovers_newest_installation_and_only_changes_selected_copy() -> io::Result<()> {
    let root = std::env::temp_dir().join(format!("riot-discovery-newest-{}", std::process::id()));
    let old = installation(&root, "old", "1.9.9c")?;
    let new = installation(&root, "nested/new", "1.10.3")?;
    let original = fs::read(&old)?;
    let mut found = Vec::new();
    visit_lists(&root, |list| found.push(list.to_owned()));
    assert_eq!(found.len(), 2);
    found.sort_by_key(|path| version_key(path));
    let selected = found.pop().unwrap();
    assert_eq!(selected, new);
    update_list(&selected, true)?;
    assert_eq!(fs::read(old)?, original);
    assert_eq!(fs::read(new)?, b"keep.example\r\n");
    fs::remove_dir_all(root)
}

#[test]
fn ignores_fake_installations_and_prunes_system_and_cache_trees() -> io::Result<()> {
    let root = std::env::temp_dir().join(format!("riot-discovery-pruning-{}", std::process::id()));
    let expected = installation(&root, "real", "1.10.3")?;
    let empty = installation(&root, "fake-empty", "9.9.9")?;
    fs::write(
        empty.parent().unwrap().parent().unwrap().join("winws.exe"),
        b"",
    )?;
    let missing = installation(&root, "fake-no-service", "9.9.9")?;
    fs::remove_file(
        missing
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("service.bat"),
    )?;
    for name in [
        "Windows",
        "AppData",
        "cache",
        "work",
        "node_modules",
        "target",
    ] {
        installation(&root, &format!("{name}/hidden"), "99.99.99")?;
    }
    let mut found = Vec::new();
    visit_lists(&root, |list| found.push(list.to_owned()));
    assert_eq!(found, vec![expected]);
    fs::remove_dir_all(root)
}

#[test]
fn sorts_numeric_versions_before_suffixes() -> io::Result<()> {
    let root = std::env::temp_dir().join(format!("riot-discovery-version-{}", std::process::id()));
    let a = installation(&root, "a", "1.9.9c")?;
    let b = installation(&root, "b", "1.10.3")?;
    let c = installation(&root, "c", "1.10.10")?;
    assert!(version_key(&a) < version_key(&b));
    assert!(version_key(&b) < version_key(&c));
    fs::remove_dir_all(root)
}
