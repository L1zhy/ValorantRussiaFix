$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$ru = $env:RIOT_FIX_GUI_LANG -eq 'ru'
$session = [Diagnostics.Process]::GetCurrentProcess().SessionId
$runner = if ($env:RIOT_FIX_RUNNER_PID -match '^\d+$') {
    Get-Process -Id ([int]$env:RIOT_FIX_RUNNER_PID) -ErrorAction Stop
}

function Assert-NoGame {
    $games = Get-Process -ErrorAction Stop | Where-Object {
        $_.SessionId -eq $session -and
        $_.ProcessName -match '^(VALORANT(?:-Win64-Shipping)?|LeagueClient(?:Ux(?:Render)?)?|League of Legends|LoR|2XKO(?:-Win64-Shipping)?)$'
    }
    if ($games) {
        throw $(if ($ru) { 'Закройте запущенную игру Riot перед перезапуском клиента.' }
            else { 'Close the running Riot game before restarting the client.' })
    }
}

function Get-ElectronStartupState([int]$serviceId, [datetime]$since) {
    $files = Get-ChildItem -LiteralPath $electronLogs -Filter '*.log' -File -ErrorAction SilentlyContinue |
        Where-Object { $_.CreationTimeUtc -ge $since } |
        Sort-Object CreationTimeUtc -Descending
    foreach ($file in $files) {
        $log = Get-Content -LiteralPath $file.FullName -Raw -ErrorAction SilentlyContinue
        if ($log -notmatch "(?m)\bappPid:\s*${serviceId}\b") { continue }
        if ($log -match '\(DataBinding\)\s+created\b') { return @{ Ready = $true } }
        if ($log -match '\(SocketManager\)\s+onReconnectFailed\b' -and
            $log -match '(?m)\bappPort:\s*(\d+)\b') {
            return @{ DisconnectedPort = [int]$Matches[1] }
        }
        return
    }
}

function Restart-Electron([int]$serviceId) {
    Assert-NoGame
    $ui = Get-CimInstance Win32_Process -Filter "ParentProcessId = $serviceId AND Name = 'Riot Client.exe'" |
        Where-Object { $_.ExecutablePath -eq $electron -and $_.SessionId -eq $session } |
        Select-Object -First 1
    if (!$ui -or $ui.CommandLine -notmatch '^"[^"]+"\s+(?<arguments>.+)$') { return $false }
    # Reuse the launch arguments from this instance of Riot Client. Keep them in memory.
    $arguments = $Matches.arguments
    $process = Get-Process -Id $ui.ProcessId -ErrorAction SilentlyContinue
    if (!$process -or $process.Path -ne $electron) { return $false }
    $process.Kill()
    if (!$process.WaitForExit(5000)) { return $false }
    Start-Process -FilePath $electron -ArgumentList $arguments -WorkingDirectory (Split-Path -LiteralPath $electron)
    return $true
}

try {
    Assert-NoGame
    $installs = Get-Content -LiteralPath (Join-Path $env:ProgramData 'Riot Games\RiotClientInstalls.json') -Raw | ConvertFrom-Json
    $exe = @($installs.rc_default, $installs.rc_live) | Where-Object {
        $_ -and [IO.Path]::IsPathRooted($_) -and
        [IO.Path]::GetFileName($_) -eq 'RiotClientServices.exe' -and
        (Test-Path -LiteralPath $_ -PathType Leaf)
    } | Select-Object -First 1
    if (-not $exe) {
        throw $(if ($ru) { 'Не удалось найти установленный Riot Client.' }
            else { 'Could not locate the installed Riot Client.' })
    }
    $electron = Join-Path (Split-Path -LiteralPath $exe) 'RiotClientElectron\Riot Client.exe'
    $clients = @(Get-Process -Name RiotClientServices,RiotClientUx,RiotClientUxRender,'Riot Client' -ErrorAction SilentlyContinue |
        Where-Object {
            $_.SessionId -eq $session -and
            ($_.ProcessName -ne 'Riot Client' -or $_.Path -eq $electron)
        })

    foreach ($client in $clients) {
        if (-not $client.HasExited) { [void]$client.CloseMainWindow() }
    }
    if ($clients.Count) { Start-Sleep -Seconds 2 }
    Assert-NoGame
    foreach ($client in $clients) {
        if (-not $client.HasExited) {
            try { $client.Kill() }
            catch { if (-not $client.HasExited) { throw } }
            if (-not $client.WaitForExit(5000)) { throw [TimeoutException]::new($client.ProcessName) }
        }
    }
    $electronLogs = Join-Path $env:LOCALAPPDATA 'Riot Games\Riot Client\Logs\Riot Client Electron Logs'
    $started = [datetime]::UtcNow
    $service = Start-Process -FilePath $exe -WorkingDirectory (Split-Path -LiteralPath $exe) -PassThru
    if (!(Test-Path -LiteralPath $electron -PathType Leaf)) {
        if ($ru) { 'Riot Client запущен. Проверьте окно клиента.' }
        else { 'Riot Client was launched. Check the client window.' }
        exit 0
    }

    $deadline = $started.AddSeconds(110)
    $retried = $false
    if ($ru) { 'Ожидаем подключения окна Riot Client к локальной службе…' }
    else { 'Waiting for the Riot Client window to connect to the local service…' }
    while ([datetime]::UtcNow -lt $deadline -and !$service.HasExited) {
        if ($runner -and $runner.HasExited) { exit 1 }
        $state = Get-ElectronStartupState $service.Id $started
        if ($state.Ready) {
            if ($ru) { 'Интерфейс Riot Client подключился к локальной службе.' }
            else { 'The Riot Client interface connected to the local service.' }
            exit 0
        }
        if (!$retried -and $state.DisconnectedPort -and
            (Get-NetTCPConnection -State Listen -LocalAddress '127.0.0.1' -LocalPort $state.DisconnectedPort -OwningProcess $service.Id -ErrorAction SilentlyContinue)) {
            $retried = $true
            if (Restart-Electron $service.Id) {
                $deadline = [datetime]::UtcNow.AddSeconds(45)
            }
        }
        Start-Sleep -Seconds 2
    }
    throw $(if ($ru) { 'Интерфейс Riot Client не подтвердил подключение к локальной службе.' }
        else { 'The Riot Client interface did not confirm a connection to the local service.' })
} catch {
    if ($ru) { [Console]::Error.WriteLine("Не удалось перезапустить Riot Client: $_") }
    else { [Console]::Error.WriteLine("Could not restart Riot Client: $_") }
    exit 1
}
