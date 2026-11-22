mod locale;

macro_rules! localized {
    ($formatter:ident, $ru:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if crate::locale::is_russian() {
            $formatter!($ru $(, $arg)*)
        } else {
            $formatter!($en $(, $arg)*)
        }
    };
}

mod gui;
mod hosts;
mod network;
mod progress;
mod windows;
mod zapret;

use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const HELP_RU: &str = "Riot Fix — проверка прямого подключения Riot

Использование:
  riot-fix.exe            Открыть графическое окно
  riot-fix.exe --apply    Применить исправления из консоли
  riot-fix.exe --dry-run  Проверить Zapret и сеть без записи файлов
  riot-fix.exe --check    Проверить HTTPS к Riot Client Config без изменений
  riot-fix.exe --voice-check  Проверить маршрут голосового чата
  riot-fix.exe --voice-fix    Временно изменить адрес голосового чата
  riot-fix.exe --voice-reset  Отменить изменение адреса голосового чата
  riot-fix.exe --restart-client  Перезапустить Riot Client
  riot-fix.exe --help     Показать справку

Добавьте --zapret-dir <path> для проверки конкретной установки Zapret.
Добавьте --no-pause для запуска без ожидания Enter.
Добавьте --lang <ru|en> для выбора языка.";

const HELP_EN: &str = "Riot Fix - direct Riot connectivity checks

Usage:
  riot-fix.exe                Open the desktop app
  riot-fix.exe --apply        Apply repairs from the console
  riot-fix.exe --dry-run      Check Zapret and the network without writing files
  riot-fix.exe --check        Check HTTPS access to Riot Client Config
  riot-fix.exe --voice-check  Check the voice chat route
  riot-fix.exe --voice-fix    Temporarily override the voice chat address
  riot-fix.exe --voice-reset  Remove the voice chat address override
  riot-fix.exe --restart-client  Restart Riot Client
  riot-fix.exe --help         Show help

Use --zapret-dir <path> to check a specific Zapret installation.
Use --no-pause to skip the Enter prompt.
Use --lang <ru|en> to choose a language.";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Gui,
    Apply,
    DryRun,
    Check,
    Help,
    VoiceCheck,
    VoiceFix,
    VoiceReset,
    RestartClient,
}

struct Options {
    mode: Mode,
    pause: bool,
    zapret_dir: Option<PathBuf>,
}

impl Options {
    fn parse() -> Result<Self, String> {
        let mut mode = None;
        let mut language_seen = false;
        let mut pause = true;
        let mut zapret_dir = None;
        let mut arguments = std::env::args_os().skip(1);
        while let Some(argument) = arguments.next() {
            let selected = match argument.to_str() {
                Some("--gui") => Mode::Gui,
                Some("--apply") => Mode::Apply,
                Some("--dry-run") => Mode::DryRun,
                Some("--voice-check") => Mode::VoiceCheck,
                Some("--voice-fix") => Mode::VoiceFix,
                Some("--voice-reset") => Mode::VoiceReset,
                Some("--restart-client") => Mode::RestartClient,
                Some("--check") => Mode::Check,
                Some("--help" | "-h") => Mode::Help,
                Some("--zapret-dir") => {
                    if zapret_dir.is_some() {
                        return Err(crate::locale::text(
                            "--zapret-dir: укажите только один путь",
                            "--zapret-dir: specify only one path",
                        )
                        .into());
                    }
                    let path = arguments.next().filter(|path| {
                        !path.is_empty() && !path.to_string_lossy().starts_with("--")
                    });
                    zapret_dir = Some(PathBuf::from(path.ok_or(crate::locale::text(
                        "Для --zapret-dir нужен путь",
                        "--zapret-dir requires a path",
                    ))?));
                    continue;
                }
                Some("--lang") => {
                    if language_seen {
                        return Err(locale::text(
                            "Укажите язык только один раз.",
                            "Specify the language only once.",
                        )
                        .into());
                    }
                    if !matches!(
                        arguments.next().as_deref().and_then(|value| value.to_str()),
                        Some("ru" | "en")
                    ) {
                        return Err(locale::text(
                            "Для --lang укажите ru или en.",
                            "--lang requires ru or en.",
                        )
                        .into());
                    }
                    language_seen = true;
                    continue;
                }
                Some("--no-pause") => {
                    pause = false;
                    continue;
                }
                _ => {
                    return Err(localized!(
                        format,
                        "Неизвестный аргумент: {}",
                        "Unknown argument: {}",
                        argument.to_string_lossy()
                    ));
                }
            };
            if mode.replace(selected).is_some() {
                return Err(crate::locale::text(
                    "Укажите только один режим.",
                    "Choose only one mode.",
                )
                .into());
            }
        }
        Ok(Self {
            mode: mode.unwrap_or(if pause && zapret_dir.is_none() {
                Mode::Gui
            } else {
                Mode::Apply
            }),
            pause,
            zapret_dir,
        })
    }
}

fn main() -> ExitCode {
    let help = locale::text(HELP_RU, HELP_EN);
    let options = match Options::parse() {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}\n\n{help}");
            return ExitCode::FAILURE;
        }
    };

    if options.mode == Mode::Help {
        println!("{help}");
        return ExitCode::SUCCESS;
    }

    let writes = matches!(
        options.mode,
        Mode::Apply | Mode::VoiceFix | Mode::VoiceReset
    );
    if writes {
        match windows::is_elevated() {
            Ok(true) => {}
            Ok(false) => match windows::relaunch_elevated() {
                Ok(()) => return ExitCode::SUCCESS,
                Err(error) => {
                    localized!(
                        eprintln,
                        "Запуск от имени администратора: {error}",
                        "Could not launch as administrator: {error}"
                    );
                    pause(options.pause);
                    return ExitCode::FAILURE;
                }
            },
            Err(error) => {
                localized!(
                    eprintln,
                    "Не удалось проверить права администратора: {error}",
                    "Could not check administrator privileges: {error}"
                );
                pause(options.pause);
                return ExitCode::FAILURE;
            }
        }
    }

    let code = match options.mode {
        Mode::Gui => match gui::run(locale::explicit()) {
            Ok(()) => 0,
            Err(error) => {
                localized!(
                    eprintln,
                    "Не удалось открыть Riot Fix: {error}",
                    "Could not open Riot Fix: {error}"
                );
                1
            }
        },
        Mode::Apply => repair(true, options.zapret_dir.as_deref()),
        Mode::DryRun => repair(false, options.zapret_dir.as_deref()),
        Mode::Check => {
            let _activity = progress::Activity::start(locale::text(
                "Ожидание HTTPS-ответа Riot",
                "Waiting for Riot HTTPS response",
            ));
            u8::from(!network::check_config()) * 2
        }
        Mode::VoiceCheck | Mode::VoiceFix | Mode::VoiceReset => voice(options.mode),
        Mode::RestartClient => match gui::run_script(
            include_bytes!("restart-client.ps1"),
            Some(locale::text("ru", "en")),
        ) {
            Ok(status) => u8::from(!status.success()),
            Err(error) => {
                localized!(
                    eprintln,
                    "Не удалось запустить перезапуск клиента: {error}",
                    "Could not start the client restart: {error}"
                );
                1
            }
        },
        Mode::Help => unreachable!(),
    };
    pause(options.pause && writes);
    ExitCode::from(code)
}

fn voice(mode: Mode) -> u8 {
    let ip = if mode == Mode::VoiceReset {
        None
    } else {
        match network::check_voice() {
            network::VoiceRoute::Healthy => {
                localized!(
                    println,
                    "Все DNS-адреса прошли проверку HTTPS. Если задано переопределение, попробуйте --voice-reset.",
                    "All DNS addresses passed HTTPS. Try --voice-reset if an override is installed."
                );
                return 0;
            }
            network::VoiceRoute::Candidate(ip) => {
                localized!(
                    println,
                    "Возможный адрес для голосового чата: {ip}. Проверьте командный чат в VALORANT.",
                    "Voice route candidate: {ip}. Confirm by joining team voice in VALORANT."
                );
                if mode == Mode::VoiceCheck {
                    return 0;
                }
                Some(ip)
            }
            network::VoiceRoute::Uncertain => {
                localized!(
                    eprintln,
                    "Стабильно доступный запасной адрес не найден.",
                    "No consistently reachable alternative was established."
                );
                return 2;
            }
        }
    };

    if let Err(error) = hosts::apply_voice(ip).and_then(|()| windows::flush_dns()) {
        localized!(
            eprintln,
            "Не удалось обновить маршрут голосового чата: {error}",
            "Could not update voice routing: {error}"
        );
        return 1;
    }
    if ip.is_some() {
        localized!(
            println,
            "Адрес голосового чата изменён. После проверки отмените изменение через --voice-reset.",
            "Voice override applied. Remove it with --voice-reset after testing."
        );
    } else {
        localized!(
            println,
            "Переопределение адреса голосового чата удалено.",
            "Voice override removed."
        );
    }
    0
}

fn repair(apply: bool, zapret_dir: Option<&Path>) -> u8 {
    println!("=== Riot Fix ===");
    let activity = progress::Activity::start(locale::text(
        "Проверка Zapret, DNS и HTTPS",
        "Checking Zapret, DNS, and HTTPS",
    ));
    let (zapret_ok, report) = if apply {
        let zapret_ok = zapret::run(true, zapret_dir);
        if !zapret_ok && zapret_dir.is_some() {
            return 1;
        }
        (zapret_ok, network::run())
    } else {
        std::thread::scope(|scope| {
            let zapret = scope.spawn(|| zapret::run(false, zapret_dir));
            let report = network::run();
            (zapret.join().expect("Zapret scan"), report)
        })
    };

    drop(activity);
    let mut config_ok = true;
    if apply {
        match hosts::apply(&report.pins) {
            Ok(count) => localized!(
                println,
                "Записей Riot Fix в hosts: {count}.",
                "Riot Fix entries in hosts: {count}."
            ),
            Err(error) => {
                localized!(
                    eprintln,
                    "Не удалось обновить hosts: {error}",
                    "Could not update hosts: {error}"
                );
                return 1;
            }
        }
        if let Err(error) = windows::flush_dns() {
            localized!(
                eprintln,
                "Не удалось очистить кеш DNS: {error}",
                "Could not flush the DNS cache: {error}"
            );
            return 1;
        }
        config_ok = network::check_config();
        localized!(
            println,
            "Перезапустите Riot Client, если он открыт.",
            "Restart Riot Client if it is open."
        );
    } else {
        localized!(
            println,
            "Предлагаемых записей hosts: {}.",
            "Suggested hosts entries: {}.",
            report.pins.len()
        );
    }

    if report.unresolved > 0 {
        localized!(
            eprintln,
            "Доменов без стабильного ответа: {}.",
            "Domains without a stable response: {}.",
            report.unresolved
        );
    }
    if !zapret_ok {
        1
    } else if !config_ok || report.unresolved > 0 {
        2
    } else {
        0
    }
}

fn pause(enabled: bool) {
    if enabled && io::stdin().is_terminal() {
        localized!(
            print,
            "Нажмите Enter для выхода...",
            "Press Enter to exit..."
        );
        let _ = io::stdout().flush();
        let _ = io::stdin().read_line(&mut String::new());
    }
}
