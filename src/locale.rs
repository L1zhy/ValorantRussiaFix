use std::sync::OnceLock;

static LANGUAGE: OnceLock<Option<&'static str>> = OnceLock::new();

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
    match explicit() {
        Some(lang) => lang == "ru",
        // The low ten bits identify the primary Windows UI language.
        None => (unsafe { GetUserDefaultUILanguage() } & 0x03ff) == 0x19,
    }
}

pub fn text(ru: &'static str, en: &'static str) -> &'static str {
    if is_russian() { ru } else { en }
}
