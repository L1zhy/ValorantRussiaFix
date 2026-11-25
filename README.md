# Riot Fix

Создано Lizhy. [Discord](https://discord.gg/pXTuzhABVx) · [Скачать для Windows x64](https://github.com/L1zhy/ValorantRussiaFix/releases/latest)

[English](README.en.md)

Riot Fix помогает разобраться с подключением Riot Client и VALORANT на Windows. Запусти `riot-fix.exe` и выбери язык в списке в правом верхнем углу.

Начни с быстрой проверки доступа к API конфигурации Riot Client. Вход в аккаунт и запуск игры проверь в самом клиенте. Отдельная проверка предназначена для голосового чата.

Если вместо интерфейса Riot Client видишь серый экран, закрой игру и нажми **«Перезапустить Riot Client»** в Riot Fix. Если окно не подключится к локальной службе, Riot Fix дождётся её запуска и откроет окно ещё раз.

Применение изменений требует прав администратора. Утилита может менять настройки Zapret и файл Windows `hosts`. При замене существующего файла она сохраняет резервную копию рядом с ним. Для отката восстанови нужный файл из копии. Для восстановления `hosts` нужны права администратора.

## Командная строка

- `riot-fix.exe --check` проверяет доступ к API конфигурации Riot Client.
- `riot-fix.exe --dry-run` ищет установки Zapret и проверяет сеть без записи файлов.
- `riot-fix.exe --help` показывает все команды.
- `riot-fix.exe --restart-client` запускает ту же проверку и перезапуск из командной строки.

Добавь `--lang ru` или `--lang en`, чтобы выбрать язык консоли.

## Сборка

Нужны Rust с поддержкой edition 2024 и инструменты MSVC. Для запуска используются Windows PowerShell и `curl.exe`.

```powershell
cargo test --locked
cargo build --release --locked
```

Готовый файл появится в `target\release\riot-fix.exe`.
