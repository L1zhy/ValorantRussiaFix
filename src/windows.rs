use std::ffi::{OsStr, c_void};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::{env, ptr};

type Handle = *mut c_void;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> Handle;
    fn CloseHandle(handle: Handle) -> i32;
    fn ReplaceFileW(
        path: *const u16,
        replacement: *const u16,
        backup: *const u16,
        flags: u32,
        exclude: *mut c_void,
        reserved: *mut c_void,
    ) -> i32;
}

#[link(name = "advapi32")]
unsafe extern "system" {
    fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
    fn GetTokenInformation(
        token: Handle,
        class: u32,
        value: *mut u32,
        size: u32,
        returned: *mut u32,
    ) -> i32;
}

#[link(name = "shell32")]
unsafe extern "system" {
    fn ShellExecuteW(
        window: Handle,
        operation: *const u16,
        file: *const u16,
        parameters: *const u16,
        directory: *const u16,
        show: i32,
    ) -> isize;
}

#[link(name = "dnsapi")]
unsafe extern "system" {
    fn DnsFlushResolverCache() -> i32;
}

pub fn is_elevated() -> io::Result<bool> {
    const TOKEN_QUERY: u32 = 0x0008;
    const TOKEN_ELEVATION: u32 = 20;

    let mut token = ptr::null_mut();
    let mut elevated = 0;
    let mut returned = 0;
    // The process pseudo-handle needs no cleanup. The queried token does.
    unsafe {
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        let result = GetTokenInformation(token, TOKEN_ELEVATION, &mut elevated, 4, &mut returned);
        let result = if result != 0 {
            Ok(elevated != 0)
        } else {
            Err(io::Error::last_os_error())
        };
        CloseHandle(token);
        result
    }
}

pub fn relaunch_elevated() -> io::Result<()> {
    let verb = wide(OsStr::new("runas"));
    let exe = wide(env::current_exe()?.as_os_str());
    let mut args = Vec::new();
    for arg in env::args_os().skip(1) {
        if !args.is_empty() {
            args.push(b' ' as u16);
        }
        append_quoted(&mut args, &arg);
    }
    args.push(0);
    // All strings are NUL-terminated and remain valid for the call.
    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            verb.as_ptr(),
            exe.as_ptr(),
            args.as_ptr(),
            ptr::null(),
            1,
        )
    };
    if result > 32 {
        Ok(())
    } else {
        let error = io::Error::last_os_error();
        if error.raw_os_error().is_some_and(|code| code != 0) {
            Err(error)
        } else {
            Err(io::Error::other(format!("ShellExecuteW: {result}")))
        }
    }
}

pub fn flush_dns() -> io::Result<()> {
    if unsafe { DnsFlushResolverCache() } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub(crate) fn replace_file(path: &Path, replacement: &Path, backup: &Path) -> io::Result<()> {
    let path = wide(path.as_os_str());
    let replacement = wide(replacement.as_os_str());
    let backup = wide(backup.as_os_str());
    // Windows preserves target metadata and creates the backup during replacement.
    if unsafe {
        ReplaceFileW(
            path.as_ptr(),
            replacement.as_ptr(),
            backup.as_ptr(),
            0,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    } == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain([0]).collect()
}

fn append_quoted(line: &mut Vec<u16>, arg: &OsStr) {
    const QUOTE: u16 = b'"' as u16;
    const BACKSLASH: u16 = b'\\' as u16;

    line.push(QUOTE);
    let mut slashes = 0;
    for unit in arg.encode_wide() {
        if unit == BACKSLASH {
            slashes += 1;
            continue;
        }
        if unit == QUOTE {
            slashes = slashes * 2 + 1;
        }
        line.extend(std::iter::repeat_n(BACKSLASH, slashes));
        line.push(unit);
        slashes = 0;
    }
    line.extend(std::iter::repeat_n(BACKSLASH, slashes * 2));
    line.push(QUOTE);
}
