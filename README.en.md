# ValorantRussiaFix

[Русский](README.md) · [English](README.en.md)

[Download riot-fix.exe for Windows x64](https://github.com/Kopat14ch/ValorantRussiaFix/releases/latest)

Riot Fix checks direct connections to Riot servers on Windows. Use it if Riot Client or VALORANT stalls while connecting on a PC running Flowseal's Zapret.

## Run it

1. Run `riot-fix.exe` and accept the administrator prompt.
2. Wait for the checks to finish. Searching your drives for Zapret can take a few minutes.
3. Close Riot Client completely, then open it again.

| Command | What it does |
| --- | --- |
| `riot-fix.exe` | Applies the Zapret settings and checks Riot server addresses. |
| `riot-fix.exe --check` | Checks HTTPS access to the Riot Client Config API without administrator rights. |
| `riot-fix.exe --dry-run` | Finds Zapret installations and checks the network without writing any files. |
| `riot-fix.exe --no-pause` | Applies the changes and exits without waiting for Enter. |

A successful `--check` confirms access to the config API. Check sign-in and game launch in the client.

## What it changes

- Removes the `riotgames.com` and `riotcdn.net` entries from `lists/list-exclude.txt` in every Zapret installation it finds.
- Enables Game Filter for TCP and UDP and turns on Auto-Update Check. Downloads [Flowseal's IPSet list](https://github.com/Flowseal/zapret-discord-youtube/blob/main/.service/ipset-service.txt) and loads it into IPSet Filter. If the download fails, it keeps the current list.
- Checks Riot addresses over HTTPS without using the system proxy. If it needs a fallback address, it adds the address to `hosts` after three successful checks. It preserves manual entries.

Riot Fix keeps a backup next to each existing file it replaces. Restore the matching backup to undo a change. Restoring the Windows `hosts` file requires administrator rights.

## Build from source

Run these commands in the directory containing `Cargo.toml`. You need Rust with edition 2024 support and the MSVC build tools. The program uses the Windows copies of `curl.exe` and Windows PowerShell.

```powershell
cargo test --locked
cargo build --release --locked
.\target\release\riot-fix.exe --check
```

Exit code `0` means success, `1` means a system error, and `2` means some network checks failed. When UAC starts a new window, the original process reports whether the launch succeeded. The new window shows the result of the run.
