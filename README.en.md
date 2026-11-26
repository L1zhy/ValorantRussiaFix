# Riot Fix

Created by Lizhy. [Discord](https://discord.gg/pXTuzhABVx) · [Download for Windows x64](https://github.com/L1zhy/ValorantRussiaFix/releases/latest)

[Русский](README.md)

Riot Fix recovers Vanguard updates and helps diagnose VALORANT connectivity on Windows. Double-click `riot-fix.exe` to open a native Windows window without a console or PowerShell. Use the menu in the upper-right corner to change languages.

Riot Fix checks the Riot Client log at startup. If the Vanguard download stops progressing for two minutes or Riot Client reports an installation failure, Riot Fix downloads the required version from Riot, verifies the installer's signature, and requests administrator approval to install it. Close any running Riot game before recovery. Riot Fix verifies the new Vanguard version and opens Riot Client after installation.

Start with the quick check for access to the Riot Client Config API. Verify sign-in and game launch in Riot Client. Voice chat has its own check.

If Riot Client opens to a blank gray window, close the game and click **Restart Riot Client** in Riot Fix. If the window can't connect to the local service, Riot Fix waits for the service to start and opens the window again.

Applying changes requires administrator rights. The tool can change Zapret settings and the Windows `hosts` file. When it replaces an existing file, it saves a backup beside it. To undo a change, restore the corresponding backup. Restoring `hosts` requires administrator rights.

## Command line

- `riot-fix.exe --check` checks access to the Riot Client Config API.
- `riot-fix.exe --dry-run` finds Zapret installations and checks the network without writing files.
- `riot-fix.exe --help` lists all commands.
- `riot-fix.exe --restart-client` runs the same restart and connection check from the command line.

Add `--lang en` or `--lang ru` to choose the console language.

## Build

You need Rust with edition 2024 support and the MSVC build tools. The app opens as a native Windows executable.

```powershell
cargo test --locked
cargo build --release --locked
```

The executable is written to `target\release\riot-fix.exe`.
