use std::env;
use std::io::{self, Write};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub fn run(language: Option<&str>) -> io::Result<()> {
    let status = run_script(include_bytes!("gui.ps1"), language)?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("PowerShell {status}")))
    }
}

pub fn run_script(script: &[u8], language: Option<&str>) -> io::Result<std::process::ExitStatus> {
    let windows = PathBuf::from(env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into()));
    let mut child = Command::new(windows.join(r"System32\WindowsPowerShell\v1.0\powershell.exe"))
        .args([
            "-NoProfile", "-NonInteractive", "-STA", "-Command",
            "[Console]::InputEncoding = [Text.Encoding]::UTF8; & ([ScriptBlock]::Create([Console]::In.ReadToEnd()))",
        ])
        .env("RIOT_FIX_GUI_EXE", env::current_exe()?)
        .env("RIOT_FIX_GUI_LANG", language.unwrap_or_default())
        .stdin(Stdio::piped())
        .creation_flags(0x08000000)
        .spawn()?;
    child.stdin.take().expect("piped stdin").write_all(script)?;
    child.wait()
}
