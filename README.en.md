# Riot Fix

Created by Lizhy. [Discord](https://discord.gg/pXTuzhABVx) · [Download for Windows x64](https://github.com/L1zhy/ValorantRussiaFix/releases/latest)

[Русский](README.md)

Riot Fix helps diagnose connection problems with Riot Client and VALORANT on Windows. Run `riot-fix.exe` and choose your language from the list in the upper-right corner.

Start with the quick check for access to the Riot Client Config API. Verify sign-in and game launch in Riot Client. Voice chat has its own check.

Applying changes requires administrator rights. The tool can change Zapret settings and the Windows `hosts` file. When it replaces an existing file, it saves a backup beside it. To undo a change, restore the corresponding backup. Restoring `hosts` requires administrator rights.

## Command line

- `riot-fix.exe --check` checks access to the Riot Client Config API.
- `riot-fix.exe --dry-run` finds Zapret installations and checks the network without writing files.
- `riot-fix.exe --help` lists all commands.
- `riot-fix.exe --restart-client` restarts an unresponsive Riot Client. The desktop app has the same option. Close the game before restarting.

Add `--lang en` or `--lang ru` to choose the console language.

## Build

You need Rust with edition 2024 support and the MSVC build tools. The app uses Windows PowerShell and `curl.exe`.

```powershell
cargo test --locked
cargo build --release --locked
```

The executable is written to `target\release\riot-fix.exe`.
