mod hosts;
mod network;
mod progress;
mod windows;
mod zapret;

use std::io::{self, Write};
use std::process::ExitCode;

const HELP: &str = "Riot Fix — проверка прямого подключения Riot

Использование:
  riot-fix.exe            Применить исправления и проверить подключение
  riot-fix.exe --dry-run  Проверить Zapret и сеть без записи файлов
  riot-fix.exe --check    Проверить HTTPS к Riot Client Config без изменений
  riot-fix.exe --help     Показать справку

Добавьте --no-pause для запуска без ожидания Enter.";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Apply,
    DryRun,
    Check,
    Help,
}

struct Options {
    mode: Mode,
    pause: bool,
}

impl Options {
    fn parse() -> Result<Self, String> {
        let mut mode = None;
        let mut pause = true;
        for argument in std::env::args_os().skip(1) {
            let selected = match argument.to_str() {
                Some("--dry-run") => Mode::DryRun,
                Some("--check") => Mode::Check,
                Some("--help" | "-h") => Mode::Help,
                Some("--no-pause") => {
                    pause = false;
                    continue;
                }
                _ => {
                    return Err(format!(
                        "Неизвестный аргумент: {}",
                        argument.to_string_lossy()
                    ));
                }
            };
            if mode.replace(selected).is_some() {
                return Err("Укажите только один режим.".into());
            }
        }
        Ok(Self {
            mode: mode.unwrap_or(Mode::Apply),
            pause,
        })
    }
}

fn main() -> ExitCode {
    let options = match Options::parse() {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}\n\n{HELP}");
            return ExitCode::FAILURE;
        }
    };

    if options.mode == Mode::Help {
        println!("{HELP}");
        return ExitCode::SUCCESS;
    }

    println!("Riot Fix v{}", env!("CARGO_PKG_VERSION"));
    println!("Создано Lizhy");
    println!("Discord-сообщество: https://discord.gg/pXTuzhABVx");
    println!();

    if options.mode == Mode::Apply {
        match windows::is_elevated() {
            Ok(true) => {}
            Ok(false) => match windows::relaunch_elevated() {
                Ok(()) => return ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("Запуск от имени администратора: {error}");
                    pause(options.pause);
                    return ExitCode::FAILURE;
                }
            },
            Err(error) => {
                eprintln!("Не удалось проверить права администратора: {error}");
                pause(options.pause);
                return ExitCode::FAILURE;
            }
        }
    }

    let outcome = std::panic::catch_unwind(|| match options.mode {
        Mode::Apply => repair(true),
        Mode::DryRun => repair(false),
        Mode::Check => {
            println!("Проверка подключения к API конфигурации Riot...");
            let checking = progress::Activity::start("Ожидание HTTPS-ответа Riot");
            let code = u8::from(!network::check_config()) * 2;
            drop(checking);
            code
        }
        Mode::Help => unreachable!(),
    });
    let code = outcome.unwrap_or_else(|_| {
        eprintln!("Ошибка выполнения. Проверьте сообщения выше. Для старой версии обновите Zapret: https://github.com/Flowseal/zapret-discord-youtube/releases/latest");
        1
    });
    println!();
    println!("==================================================");
    match code {
        0 if options.mode == Mode::Apply => {
            println!("ГОТОВО. Настройки применены, проверки завершены.");
            println!("Теперь полностью закройте Riot Client и запустите его снова.");
        }
        0 if options.mode == Mode::Check => {
            println!("ГОТОВО. API конфигурации Riot доступен.");
            println!("Вход в аккаунт и запуск игры проверьте в Riot Client.");
        }
        0 => println!("ГОТОВО. Диагностика завершена. Для применения запустите riot-fix.exe."),
        2 => {
            println!("ПРОВЕРКА ЗАВЕРШЕНА. Есть предупреждения по доступности отдельных адресов.");
            println!(
                "Полностью закройте Riot Client и запустите его снова. Проверьте вход и запуск игры."
            );
        }
        _ => {
            println!("ОБРАБОТКА ЗАВЕРШЕНА. Некоторые настройки требуют внимания.");
            println!(
                "Подробности указаны выше. Перезапустите Riot Client и проверьте результат. Если проблема осталась, сохраните вывод для помощи."
            );
        }
    }
    println!("==================================================");
    if code == 0 && options.mode == Mode::Apply {
        println!(
            "Если фикс помог, поставь звезду на GitHub. Это поможет продвижению проекта, и другие игроки смогут его найти."
        );
        println!("https://github.com/Kopat14ch/ValorantRussiaFix");
    }
    pause(options.pause && options.mode == Mode::Apply);
    ExitCode::from(code)
}

fn repair(apply: bool) -> u8 {
    if !apply {
        println!("Режим диагностики. Изменения файлов отключены.");
    }
    println!("\n[1/3] Поиск и выбор Zapret");
    let searching = progress::Activity::start("Поиск и настройка Zapret");
    let zapret_ok = zapret::run(apply);
    drop(searching);
    println!("\n[2/3] Проверка подключения к Riot");
    println!(
        "Каждый подходящий IP проходит 3 HTTPS-проверки. Ожидание ответов может занять несколько минут."
    );
    let checking = progress::Activity::start("Проверка DNS и HTTPS к Riot");
    let report = network::run();
    drop(checking);

    println!("\n[3/3] Применение сетевых настроек");
    if apply {
        match hosts::apply(&report.pins) {
            Ok(count) => println!("  Записей Riot Fix в hosts: {count}."),
            Err(error) => {
                eprintln!("  Ошибка обновления hosts: {error}");
                return 1;
            }
        }
        if let Err(error) = windows::flush_dns() {
            eprintln!("  Ошибка очистки кеша DNS: {error}");
            return 1;
        }
        println!("  Кеш DNS очищен.");
    } else {
        println!("  Предлагаемых записей hosts: {}.", report.pins.len());
        println!("  Чтобы применить настройки, запустите программу без --dry-run.");
    }

    if report.unresolved > 0 {
        println!("  Доменов без стабильного ответа: {}.", report.unresolved);
    }
    if !zapret_ok {
        1
    } else if report.unresolved > 0 {
        2
    } else {
        0
    }
}

fn pause(enabled: bool) {
    if enabled {
        print!("Нажмите Enter для выхода...");
        let _ = io::stdout().flush();
        let _ = io::stdin().read_line(&mut String::new());
    }
}
