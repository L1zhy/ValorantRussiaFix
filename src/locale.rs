use std::cell::Cell;
use std::sync::OnceLock;

static LANGUAGE: OnceLock<Option<&'static str>> = OnceLock::new();
thread_local! { static WORKER_LANGUAGE: Cell<Option<bool>> = const { Cell::new(None) }; }

pub fn set_worker_language(ru: bool) {
    WORKER_LANGUAGE.set(Some(ru));
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetUserDefaultUILanguage() -> u16;
}

pub fn explicit() -> Option<&'static str> {
    *LANGUAGE.get_or_init(|| {
        let mut args = std::env::args_os().skip(1);
        args.find(|arg| arg == "--lang")?;
        match args.next()?.to_str()? {
            "ru" => Some("ru"),
            "en" => Some("en"),
            _ => None,
        }
    })
}

pub fn is_russian() -> bool {
    if let Some(ru) = WORKER_LANGUAGE.get() {
        return ru;
    }
    match explicit() {
        Some(lang) => lang == "ru",
        // The low ten bits identify the primary Windows UI language.
        None => (unsafe { GetUserDefaultUILanguage() } & 0x03ff) == 0x19,
    }
}

pub fn text(ru: &'static str, en: &'static str) -> &'static str {
    if is_russian() { ru } else { en }
}
