$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$ru = $env:RIOT_FIX_GUI_LANG -eq 'ru'
$session = [Diagnostics.Process]::GetCurrentProcess().SessionId

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
    Start-Process -FilePath $exe -WorkingDirectory (Split-Path -LiteralPath $exe)
    if ($ru) { 'Riot Client запущен. Проверьте вход в VALORANT.' }
    else { 'Riot Client was launched. Try signing in to VALORANT.' }
    exit 0
} catch {
    if ($ru) { [Console]::Error.WriteLine("Не удалось перезапустить Riot Client: $_") }
    else { [Console]::Error.WriteLine("Could not restart Riot Client: $_") }
    exit 1
}
