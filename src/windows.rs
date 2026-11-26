use std::ffi::{OsStr, c_void};
use std::fs::File;
use std::io;
use std::net::Ipv4Addr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::path::PathBuf;
use std::{env, ptr};

use windows_sys::Win32::Foundation::{
    ERROR_INVALID_PARAMETER, HANDLE, INVALID_HANDLE_VALUE, WAIT_ABANDONED, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows_sys::Win32::System::{Console, Diagnostics::ToolHelp::*, Registry::*, Threading::*};
use windows_sys::Win32::UI::{Shell::*, WindowsAndMessaging::*};
use windows_sys::core::w;

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

pub fn attach_parent_console() {
    // Workers with redirected output already have valid standard handles.
    unsafe {
        if Console::GetStdHandle(Console::STD_OUTPUT_HANDLE).is_null()
            && Console::GetStdHandle(Console::STD_ERROR_HANDLE).is_null()
        {
            Console::AttachConsole(Console::ATTACH_PARENT_PROCESS);
        }
    }
}

pub fn run_elevated(exe: &Path, args: &[&str]) -> io::Result<u32> {
    let file = wide(exe.as_os_str());
    let directory = exe.parent().map(|dir| wide(dir.as_os_str()));
    let mut parameters = Vec::new();
    for arg in args {
        if !parameters.is_empty() {
            parameters.push(b' ' as u16);
        }
        append_quoted(&mut parameters, OsStr::new(arg));
    }
    parameters.push(0);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: w!("runas"),
        lpFile: file.as_ptr(),
        lpParameters: parameters.as_ptr(),
        lpDirectory: directory.as_ref().map_or(ptr::null(), |dir| dir.as_ptr()),
        nShow: SW_SHOWNORMAL,
        ..Default::default()
    };
    unsafe {
        if ShellExecuteExW(&mut info) == 0 || info.hProcess.is_null() {
            return Err(io::Error::last_os_error());
        }
        let handle = OwnedHandle::from_raw_handle(info.hProcess);
        let mut code = 0;
        if WaitForSingleObject(handle.as_raw_handle(), INFINITE) != WAIT_OBJECT_0
            || GetExitCodeProcess(handle.as_raw_handle(), &mut code) == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(code)
    }
}

pub struct UpdateLock(OwnedHandle);

impl Drop for UpdateLock {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.0.as_raw_handle());
        }
    }
}

pub fn vanguard_lock() -> io::Result<UpdateLock> {
    unsafe {
        let handle = CreateMutexW(ptr::null(), 0, w!("Local\\RiotFix.Vanguard.Update"));
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let handle = OwnedHandle::from_raw_handle(handle);
        match WaitForSingleObject(handle.as_raw_handle(), 0) {
            WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(UpdateLock(handle)),
            WAIT_TIMEOUT => Err(io::Error::other(crate::locale::text(
                "Восстановление Vanguard уже запущено в другом окне.",
                "Vanguard recovery is already running in another window.",
            ))),
            _ => Err(io::Error::last_os_error()),
        }
    }
}

pub fn verify_riot_signature(path: &Path, file: &File) -> io::Result<()> {
    use windows_sys::Win32::Security::{Cryptography::*, WinTrust::*};
    let path = wide(path.as_os_str());
    let mut file_info = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: path.as_ptr(),
        hFile: file.as_raw_handle(),
        ..Default::default()
    };
    let mut data = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_WHOLECHAIN,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 {
            pFile: &mut file_info,
        },
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT | WTD_DISABLE_MD2_MD4,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let mut signer_name = [0u16; 128];
    let (status, length) = unsafe {
        let status = WinVerifyTrust(
            ptr::null_mut(),
            &mut action,
            ptr::from_mut(&mut data).cast(),
        );
        let length = if status == 0 {
            let provider = WTHelperProvDataFromStateData(data.hWVTStateData);
            (!provider.is_null())
                .then(|| WTHelperGetProvSignerFromChain(provider, 0, 0, 0))
                .and_then(|signer| signer.as_ref())
                .filter(|signer| signer.csCertChain > 0)
                .and_then(|signer| signer.pasCertChain.as_ref())
                .map_or(0, |cert| {
                    CertGetNameStringW(
                        cert.pCert,
                        CERT_NAME_SIMPLE_DISPLAY_TYPE,
                        0,
                        ptr::null(),
                        signer_name.as_mut_ptr(),
                        signer_name.len() as u32,
                    )
                })
        } else {
            0
        };
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        WinVerifyTrust(
            ptr::null_mut(),
            &mut action,
            ptr::from_mut(&mut data).cast(),
        );
        (status, length)
    };
    if length > 1
        && String::from_utf16_lossy(&signer_name[..length as usize - 1]) == "Riot Games, Inc."
    {
        return Ok(());
    }
    Err(io::Error::other(localized!(
        format,
        "Подпись Riot Games не подтверждена (0x{:08X}).",
        "Riot Games signature verification failed (0x{:08X}).",
        status as u32
    )))
}

pub fn vanguard_version() -> Option<[u16; 4]> {
    use windows_sys::Win32::Storage::FileSystem::*;
    let mut path = [0u16; 2048];
    let mut bytes = size_of_val(&path) as u32;
    unsafe {
        if RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!(r"SYSTEM\CurrentControlSet\Services\vgc"),
            w!("ImagePath"),
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
            ptr::null_mut(),
            path.as_mut_ptr().cast(),
            &mut bytes,
        ) != 0
        {
            return None;
        }
        let path = std::ffi::OsString::from_wide(until_nul(&path));
        let path = path.to_string_lossy();
        let path = wide(OsStr::new(path.trim_matches('"')));
        let size = GetFileVersionInfoSizeW(path.as_ptr(), ptr::null_mut());
        if size == 0 || size > 1024 * 1024 {
            return None;
        }
        let mut buffer = vec![0u32; (size as usize).div_ceil(4)];
        if GetFileVersionInfoW(path.as_ptr(), 0, size, buffer.as_mut_ptr().cast()) == 0 {
            return None;
        }
        let mut version = ptr::null_mut();
        let mut length = 0;
        if VerQueryValueW(buffer.as_ptr().cast(), w!("\\"), &mut version, &mut length) == 0
            || (length as usize) < size_of::<VS_FIXEDFILEINFO>()
        {
            return None;
        }
        let version = version.cast::<VS_FIXEDFILEINFO>().read_unaligned();
        Some([
            (version.dwFileVersionMS >> 16) as u16,
            version.dwFileVersionMS as u16,
            (version.dwFileVersionLS >> 16) as u16,
            version.dwFileVersionLS as u16,
        ])
    }
}

pub fn dns_a(domain: &str, server: Option<Ipv4Addr>) -> Vec<Ipv4Addr> {
    use windows_sys::Win32::NetworkManagement::Dns::*;
    use windows_sys::Win32::Networking::WinSock::{AF_INET, IN_ADDR, IN_ADDR_0, SOCKADDR_IN};
    let domain = wide(OsStr::new(domain));
    let mut request = DNS_QUERY_REQUEST {
        Version: DNS_QUERY_REQUEST_VERSION1,
        QueryName: domain.as_ptr(),
        QueryType: DNS_TYPE_A,
        QueryOptions: (DNS_QUERY_NO_HOSTS_FILE | DNS_QUERY_TREAT_AS_FQDN) as u64,
        ..Default::default()
    };
    let mut servers = DNS_ADDR_ARRAY::default();
    if let Some(ip) = server {
        let address = SOCKADDR_IN {
            sin_family: AF_INET,
            sin_port: 53u16.to_be(),
            sin_addr: IN_ADDR {
                S_un: IN_ADDR_0 {
                    S_addr: u32::from_ne_bytes(ip.octets()),
                },
            },
            sin_zero: [0; 8],
        };
        unsafe {
            ptr::copy_nonoverlapping(
                ptr::from_ref(&address).cast::<i8>(),
                servers.AddrArray[0].MaxSa.as_mut_ptr(),
                size_of::<SOCKADDR_IN>(),
            );
        }
        servers.AddrCount = 1;
        servers.MaxCount = 1;
        request.pDnsServerList = &mut servers;
        request.QueryOptions |= DNS_QUERY_WIRE_ONLY as u64;
    }
    let mut result = DNS_QUERY_RESULT {
        Version: DNS_QUERY_RESULTS_VERSION1,
        ..Default::default()
    };
    let mut addresses = Vec::new();
    unsafe {
        let status = DnsQueryEx(&request, &mut result, ptr::null_mut());
        if status == 0 {
            let mut next = result.pQueryRecords;
            while let Some(record) = next.as_ref() {
                if record.wType == DNS_TYPE_A {
                    addresses.push(Ipv4Addr::from(record.Data.A.IpAddress.to_ne_bytes()));
                }
                next = record.pNext;
            }
        }
        if !result.pQueryRecords.is_null() {
            DnsFree(result.pQueryRecords.cast(), DnsFreeRecordList);
        }
    }
    addresses
}

fn until_nul(units: &[u16]) -> &[u16] {
    &units[..units
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(units.len())]
}

pub struct Process {
    pub id: u32,
    pub parent: u32,
    name: String,
}
pub struct ProcessHandle(OwnedHandle);

pub fn session_processes() -> io::Result<Vec<Process>> {
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    let mut session = 0;
    let mut processes = Vec::new();
    unsafe {
        if ProcessIdToSessionId(std::process::id(), &mut session) == 0 {
            return Err(io::Error::last_os_error());
        }
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let snapshot = OwnedHandle::from_raw_handle(snapshot);
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snapshot.as_raw_handle(), &mut entry) == 0 {
            return Err(io::Error::last_os_error());
        }
        loop {
            let mut other = 0;
            if ProcessIdToSessionId(entry.th32ProcessID, &mut other) != 0 && other == session {
                processes.push(Process {
                    id: entry.th32ProcessID,
                    parent: entry.th32ParentProcessID,
                    name: String::from_utf16_lossy(until_nul(&entry.szExeFile)),
                });
            }
            if Process32NextW(snapshot.as_raw_handle(), &mut entry) == 0 {
                break;
            }
        }
    }
    Ok(processes)
}

pub fn same_path_prefix(path: &Path, prefix: &Path) -> bool {
    let mut components = path.components();
    prefix.components().all(|expected| {
        components.next().is_some_and(|component| {
            expected
                .as_os_str()
                .eq_ignore_ascii_case(component.as_os_str())
        })
    })
}

impl Process {
    pub fn matches(&self, names: &[&str]) -> bool {
        names
            .iter()
            .any(|name| self.name.eq_ignore_ascii_case(name))
    }

    pub fn open_for_restart(&self) -> io::Result<Option<ProcessHandle>> {
        unsafe {
            let handle = OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | SYNCHRONIZATION_SYNCHRONIZE,
                0,
                self.id,
            );
            if !handle.is_null() {
                return Ok(Some(ProcessHandle(OwnedHandle::from_raw_handle(handle))));
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
                Ok(None)
            } else {
                Err(error)
            }
        }
    }
}

impl ProcessHandle {
    pub fn image(&self) -> io::Result<PathBuf> {
        let mut buffer = vec![0u16; 32768];
        let mut length = buffer.len() as u32;
        if unsafe {
            QueryFullProcessImageNameW(self.0.as_raw_handle(), 0, buffer.as_mut_ptr(), &mut length)
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(PathBuf::from(std::ffi::OsString::from_wide(
            &buffer[..length as usize],
        )))
    }

    pub fn close_windows(&self, pid: u32) {
        unsafe extern "system" fn close_if_owned(
            hwnd: windows_sys::Win32::Foundation::HWND,
            pid: isize,
        ) -> i32 {
            unsafe {
                let mut owner = 0;
                GetWindowThreadProcessId(hwnd, &mut owner);
                if owner == pid as u32 && IsWindowVisible(hwnd) != 0 {
                    PostMessageW(hwnd, WM_CLOSE, 0, 0);
                }
            }
            1
        }
        unsafe {
            EnumWindows(Some(close_if_owned), pid as isize);
        }
    }

    pub fn stop(&self) -> io::Result<()> {
        let handle = self.0.as_raw_handle();
        unsafe {
            if WaitForSingleObject(handle, 0) == WAIT_OBJECT_0 {
                return Ok(());
            }
            if TerminateProcess(handle, 1) == 0 && WaitForSingleObject(handle, 0) != WAIT_OBJECT_0 {
                return Err(io::Error::last_os_error());
            }
            if WaitForSingleObject(handle, 5000) != WAIT_OBJECT_0 {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "Riot Client"));
            }
        }
        Ok(())
    }

    pub fn command_arguments(&self) -> Option<std::ffi::OsString> {
        #[repr(C)]
        struct UnicodeString {
            length: u16,
            _capacity: u16,
            buffer: *const u16,
        }
        #[link(name = "ntdll")]
        unsafe extern "system" {
            fn NtQueryInformationProcess(
                process: HANDLE,
                class: u32,
                output: *mut c_void,
                capacity: u32,
                returned: *mut u32,
            ) -> i32;
        }
        const PROCESS_COMMAND_LINE_INFORMATION: u32 = 60;
        let mut buffer = vec![0usize; 128 * 1024 / size_of::<usize>()];
        let handle = self.0.as_raw_handle();
        let size = size_of_val(buffer.as_slice());
        unsafe {
            if NtQueryInformationProcess(
                handle,
                PROCESS_COMMAND_LINE_INFORMATION,
                buffer.as_mut_ptr().cast(),
                size as u32,
                ptr::null_mut(),
            ) < 0
            {
                return None;
            }
            let line = &*buffer.as_ptr().cast::<UnicodeString>();
            let offset = (line.buffer as usize).checked_sub(buffer.as_ptr() as usize)?;
            if offset + usize::from(line.length) > size || offset % 2 != 0 {
                return None;
            }
            let line = std::slice::from_raw_parts(line.buffer, usize::from(line.length) / 2);
            let rest = line.strip_prefix(&[b'"' as u16])?;
            let end = rest.iter().position(|unit| *unit == b'"' as u16)?;
            let args = &rest[end + 1..];
            let first = args.iter().position(|unit| !matches!(*unit, 0x20 | 0x09))?;
            let args = &args[first..];
            (!args.is_empty()).then(|| std::ffi::OsString::from_wide(args))
        }
    }
}

pub fn error_dialog(error: &str) {
    let error = wide(OsStr::new(error));
    unsafe {
        MessageBoxW(
            ptr::null_mut(),
            error.as_ptr(),
            w!("Riot Fix"),
            MB_OK | MB_ICONERROR,
        );
    }
}

pub fn owns_loopback_port(pid: u32, port: u16) -> bool {
    use windows_sys::Win32::NetworkManagement::IpHelper::*;
    use windows_sys::Win32::Networking::WinSock::AF_INET;
    let mut bytes = 0;
    unsafe {
        GetExtendedTcpTable(
            ptr::null_mut(),
            &mut bytes,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        );
        if !(4..=8 * 1024 * 1024).contains(&bytes) {
            return false;
        }
        let mut table = vec![0u32; (bytes as usize).div_ceil(4)];
        if GetExtendedTcpTable(
            table.as_mut_ptr().cast(),
            &mut bytes,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        ) != 0
        {
            return false;
        }
        let count = table[0] as usize;
        if count > (bytes as usize - 4) / size_of::<MIB_TCPROW_OWNER_PID>() {
            return false;
        }
        std::slice::from_raw_parts(table.as_ptr().add(1).cast::<MIB_TCPROW_OWNER_PID>(), count)
            .iter()
            .any(|row| {
                row.dwOwningPid == pid
                    && row.dwLocalAddr.to_ne_bytes() == [127, 0, 0, 1]
                    && u16::from_be(row.dwLocalPort as u16) == port
            })
    }
}
