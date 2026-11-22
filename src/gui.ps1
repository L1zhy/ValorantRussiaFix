$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName PresentationFramework, PresentationCore, WindowsBase

[xml]$layout = @'
<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        Title="Riot Fix" Width="940" Height="685" MinWidth="810" MinHeight="640"
        WindowStartupLocation="CenterScreen" Background="#101723"
        Foreground="#F2F5FA" FontFamily="Segoe UI" FontSize="14">
  <Window.Resources>
    <Style TargetType="Button">
      <Setter Property="Foreground" Value="#EDF4FF"/>
      <Setter Property="Background" Value="#202E40"/>
      <Setter Property="BorderBrush" Value="#40536E"/>
      <Setter Property="Height" Value="38"/>
      <Setter Property="Margin" Value="0,0,0,8"/>
      <Setter Property="Padding" Value="11,0"/>
      <Setter Property="HorizontalContentAlignment" Value="Left"/>
      <Setter Property="FontWeight" Value="SemiBold"/>
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="Button">
            <Border x:Name="Surface" Background="{TemplateBinding Background}"
                    BorderBrush="{TemplateBinding BorderBrush}" BorderThickness="1" CornerRadius="5">
              <ContentPresenter Margin="{TemplateBinding Padding}"
                                HorizontalAlignment="{TemplateBinding HorizontalContentAlignment}"
                                VerticalAlignment="Center"/>
            </Border>
            <ControlTemplate.Triggers>
              <Trigger Property="IsMouseOver" Value="True">
                <Setter TargetName="Surface" Property="BorderBrush" Value="#91D2FF"/>
              </Trigger>
              <Trigger Property="IsKeyboardFocused" Value="True">
                <Setter TargetName="Surface" Property="BorderBrush" Value="White"/>
                <Setter TargetName="Surface" Property="BorderThickness" Value="2"/>
              </Trigger>
              <Trigger Property="IsPressed" Value="True">
                <Setter TargetName="Surface" Property="Opacity" Value="0.75"/>
              </Trigger>
              <Trigger Property="IsEnabled" Value="False">
                <Setter TargetName="Surface" Property="Opacity" Value="0.46"/>
              </Trigger>
            </ControlTemplate.Triggers>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
    </Style>
  </Window.Resources>
  <DockPanel Margin="27,21,27,24">
    <Border DockPanel.Dock="Top" BorderBrush="#33445A" BorderThickness="0,0,0,1"
            Padding="0,0,0,16" Margin="0,0,0,17">
      <DockPanel>
        <Border DockPanel.Dock="Left" Width="5" Background="#68BAF7" CornerRadius="2" Margin="0,4,13,3"/>
        <ComboBox DockPanel.Dock="Right" x:Name="Language" Width="112" Margin="10,0,0,0" VerticalAlignment="Center" AutomationProperties.Name="Language">
          <ComboBoxItem Tag="ru" Content="Русский"/>
          <ComboBoxItem Tag="en" Content="English"/>
        </ComboBox>
        <StackPanel>
          <TextBlock Text="Riot Fix" FontSize="35" FontWeight="SemiBold"/>
          <TextBlock x:Name="Subtitle" Foreground="#B5C5DA"/>
        </StackPanel>
      </DockPanel>
    </Border>
    <StackPanel DockPanel.Dock="Left" x:Name="Actions" Width="235" Margin="0,0,19,0">
      <TextBlock x:Name="Connection" FontSize="17" FontWeight="SemiBold" Margin="0,0,0,9"/>
      <Button x:Name="Quick" Foreground="#10263B" Background="#8BD0FF" BorderBrush="#8BD0FF"/>
      <Button x:Name="Scan"/>
      <Button x:Name="Repair"/>
      <Button x:Name="RestartClient"/>
      <Border BorderBrush="#33445A" BorderThickness="0,0,0,1" Margin="0,9,0,14"/>
      <TextBlock x:Name="Voice" FontSize="17" FontWeight="SemiBold" Margin="0,0,0,9"/>
      <Button x:Name="VoiceCheck"/>
      <Button x:Name="VoiceFix"/>
      <Button x:Name="VoiceReset"/>
      <TextBlock x:Name="Admin"
                 TextWrapping="Wrap" Foreground="#B5C5DA" Margin="0,8,8,0" FontSize="12"/>
    </StackPanel>
    <Border Background="#151F2E" BorderBrush="#33445A" BorderThickness="1" CornerRadius="8" Padding="17">
      <DockPanel>
        <TextBlock DockPanel.Dock="Top" x:Name="Status" FontSize="21" FontWeight="SemiBold"/>
        <ProgressBar DockPanel.Dock="Top" x:Name="Progress" Height="4" Margin="0,10,0,12"
                     Foreground="#68BAF7" Background="#28384D"/>
        <Button DockPanel.Dock="Bottom" x:Name="Copy"
                HorizontalAlignment="Right" Margin="0,10,0,0" Height="33" IsEnabled="False"/>
        <Button DockPanel.Dock="Bottom" x:Name="Cancel"
                HorizontalAlignment="Left" Margin="0,7,0,0" Height="33" Visibility="Collapsed"/>
        <TextBox x:Name="Log" IsReadOnly="True" AcceptsReturn="True" TextWrapping="Wrap"
                 VerticalScrollBarVisibility="Auto"
                 FontFamily="Consolas" FontSize="13" Background="#0F1724" Foreground="#D3DFF0"
                 BorderBrush="#304157" Padding="10"/>
      </DockPanel>
    </Border>
  </DockPanel>
</Window>
'@

$window = [Windows.Markup.XamlReader]::Load([System.Xml.XmlNodeReader]::new($layout))
$actions = $window.FindName('Actions')
$status = $window.FindName('Status')
$log = $window.FindName('Log')
$progress = $window.FindName('Progress')
$copy = $window.FindName('Copy')
$cancel = $window.FindName('Cancel')
$language = $window.FindName('Language')
$state = @{ Operation = $null; StatusKey = 'Ready'; LogKey = 'Intro'; LogArgument = $null }
$strings = @{
    ru = @{
        Subtitle = 'Подключение и голосовой чат VALORANT'
        Connection = 'Подключение'
        RestartClient = 'Перезапустить Riot Client'
        RestartRunning = 'Перезапускаем Riot Client…'
        Voice = 'Голосовой чат'
        Admin = 'Для применения и отката нужны права администратора.'
        Quick = 'Проверить Riot'
        Scan = 'Полная диагностика'
        Repair = 'Применить общий фикс'
        VoiceCheck = 'Проверить маршрут войса'
        VoiceFix = 'Применить фикс войса'
        VoiceReset = 'Отменить фикс войса'
        Copy = 'Скопировать журнал'
        Cancel = 'Остановить проверку'
        Ready = 'Готово к проверке'
        Intro = 'Выбери действие слева. Здесь появятся результаты.'
        LanguageName = 'Язык'
        LogName = 'Журнал диагностики'
        LaunchFailed = 'Не удалось запустить действие'
        AdminWindow = 'Ход выполнения показан в окне администратора.'
        Checking = 'Выполняется проверка…'
        VoiceApplied = 'Команда завершилась. Проверь вход в голосовой канал VALORANT.'
        VoiceRemoved = 'Запись для голосового чата удалена.'
        RepairDone = 'Команда завершилась. Перезапусти Riot Client и проверь игру.'
        ExitCode = 'Код завершения {0}. Запусти диагностику подключения.'
        Stopped = 'Проверка остановлена'
        Done = 'Готово'
        Warnings = 'Есть сетевые предупреждения'
        Failed = 'Действие завершилось с ошибкой'
        StopFailed = 'Не удалось остановить проверку'
        QuickRunning = 'Проверяем подключение к Riot…'
        ScanRunning = 'Проверяем сеть и Zapret…'
        RepairRunning = 'Применяем исправление подключения…'
        VoiceCheckRunning = 'Проверяем адреса голосового сервиса…'
        VoiceFixRunning = 'Проверяем и применяем маршрут…'
        VoiceResetRunning = 'Удаляем запись для голосового чата…'
        ClipboardBusy = 'Буфер обмена занят'
    }
    en = @{
        Subtitle = 'VALORANT connection and voice chat'
        Connection = 'Connection'
        RestartClient = 'Restart Riot Client'
        RestartRunning = 'Restarting Riot Client…'
        Voice = 'Voice chat'
        Admin = 'Applying or undoing a fix requires administrator rights.'
        Quick = 'Check Riot connection'
        Scan = 'Run full diagnostics'
        Repair = 'Apply connection fix'
        VoiceCheck = 'Check voice chat route'
        VoiceFix = 'Apply voice chat fix'
        VoiceReset = 'Undo voice chat fix'
        Copy = 'Copy log'
        Cancel = 'Stop check'
        Ready = 'Ready to check'
        Intro = 'Choose an action on the left. Results appear here.'
        LanguageName = 'Language'
        LogName = 'Diagnostic log'
        LaunchFailed = 'Could not start the action'
        AdminWindow = 'Progress appears in the administrator window.'
        Checking = 'Running check…'
        VoiceApplied = 'Command finished. Try joining a VALORANT voice channel.'
        VoiceRemoved = 'Voice chat entry removed.'
        RepairDone = 'Command finished. Restart Riot Client and check the game.'
        ExitCode = 'Exit code {0}. Run diagnostics to check the connection.'
        Stopped = 'Check stopped'
        Done = 'Done'
        Warnings = 'Network warnings found'
        Failed = 'Action failed'
        StopFailed = 'Could not stop the check'
        QuickRunning = 'Checking the Riot connection…'
        ScanRunning = 'Checking the network and Zapret…'
        RepairRunning = 'Applying the connection fix…'
        VoiceCheckRunning = 'Checking voice service addresses…'
        VoiceFixRunning = 'Checking and applying the route…'
        VoiceResetRunning = 'Removing the voice chat entry…'
        ClipboardBusy = 'Clipboard is busy'
    }
}

function Set-Status([string]$key) {
    $state.StatusKey = $key
    $status.Text = $strings[$state.Language][$key]
}

function Set-LogMessage([string]$key, $argument = $null) {
    $state.LogKey = $key
    $state.LogArgument = $argument
    $log.Text = $strings[$state.Language][$key] -f $argument
}

$language.Add_SelectionChanged({
    $state.Language = $language.SelectedItem.Tag
    $current = $strings[$state.Language]
    foreach ($name in 'Subtitle', 'Connection', 'Voice', 'Admin') {
        $window.FindName($name).Text = $current[$name]
    }
    foreach ($name in 'Quick', 'Scan', 'Repair', 'RestartClient', 'VoiceCheck', 'VoiceFix', 'VoiceReset', 'Copy', 'Cancel') {
        $window.FindName($name).Content = $current[$name]
    }
    [Windows.Automation.AutomationProperties]::SetName($language, $current.LanguageName)
    [Windows.Automation.AutomationProperties]::SetName($log, $current.LogName)
    Set-Status $state.StatusKey
    if ($state.LogKey) { Set-LogMessage $state.LogKey $state.LogArgument }
})

$initialLanguage = $env:RIOT_FIX_GUI_LANG
if ($initialLanguage -notin 'ru', 'en') {
    $initialLanguage = (Get-UICulture).TwoLetterISOLanguageName
}
$language.SelectedIndex = if ($initialLanguage -eq 'ru') { 0 } else { 1 }

$timer = [Windows.Threading.DispatcherTimer]::new()
$timer.Interval = [TimeSpan]::FromMilliseconds(100)

function Start-Operation([string]$command, [string]$caption, [bool]$elevate) {
    $info = [Diagnostics.ProcessStartInfo]::new($env:RIOT_FIX_GUI_EXE, "$command --no-pause --lang $($state.Language)")
    $info.UseShellExecute = $elevate
    $info.CreateNoWindow = -not $elevate
    if ($elevate) {
        $info.Verb = 'runas'
    } else {
        $info.RedirectStandardOutput = $true
        $info.RedirectStandardError = $true
        $info.StandardOutputEncoding = [Text.Encoding]::UTF8
        $info.StandardErrorEncoding = [Text.Encoding]::UTF8
    }
    try {
        $process = [Diagnostics.Process]::Start($info)
    } catch {
        Set-Status 'LaunchFailed'
        $state.LogKey = $null
        $log.Text = $_.Exception.GetBaseException().Message
        $copy.IsEnabled = $true
        return
    }
    $operation = @{ Process = $process; Elevated = $elevate; Command = $command }
    if ($elevate) {
        Set-LogMessage 'AdminWindow'
    } else {
        $operation.Streams = @(
            @{ Reader = $process.StandardOutput; Pending = $process.StandardOutput.ReadLineAsync() },
            @{ Reader = $process.StandardError; Pending = $process.StandardError.ReadLineAsync() }
        )
        Set-LogMessage 'Checking'
        $cancel.Visibility = 'Visible'
        $cancel.IsEnabled = $true
    }
    Set-Status $caption
    $actions.IsEnabled = $false
    $copy.IsEnabled = $false
    $progress.IsIndeterminate = $true
    $state.Operation = $operation
    $timer.Start()
}

$timer.Add_Tick({
    $operation = $state.Operation
    $newLines = [Collections.Generic.List[string]]::new()
    foreach ($stream in $operation.Streams) {
        for ($i = 0; $i -lt 100 -and $stream.Pending -and $stream.Pending.IsCompleted; $i++) {
            $line = $stream.Pending.GetAwaiter().GetResult()
            if ($null -eq $line) {
                $stream.Pending = $null
                break
            }
            $newLines.Add($line)
            $stream.Pending = $stream.Reader.ReadLineAsync()
        }
    }
    if ($newLines.Count) {
        if (!$operation.HasOutput) {
            $state.LogKey = $null
            $log.Clear()
        }
        $operation.HasOutput = $true
        $log.AppendText(($newLines -join [Environment]::NewLine) + [Environment]::NewLine)
        $log.ScrollToEnd()
    }
    if (!$operation.Process.HasExited) { return }
    if ($operation.Streams | Where-Object { $_.Pending }) { return }
    $timer.Stop()
    $code = $operation.Process.ExitCode
    if ($operation.Elevated) {
        if ($code -eq 0) {
            switch ($operation.Command) {
                '--voice-fix' { Set-LogMessage 'VoiceApplied' }
                '--voice-reset' { Set-LogMessage 'VoiceRemoved' }
                default { Set-LogMessage 'RepairDone' }
            }
        } else {
            Set-LogMessage 'ExitCode' $code
        }
    } elseif (!$operation.HasOutput) {
        $state.LogKey = $null
        $log.Clear()
    }
    if ($operation.Cancelled) {
        Set-Status 'Stopped'
    } else {
        switch ($code) {
            0 { Set-Status 'Done' }
            2 { Set-Status 'Warnings' }
            default { Set-Status 'Failed' }
        }
    }
    $operation.Process.Dispose()
    $state.Operation = $null
    $progress.IsIndeterminate = $false
    $actions.IsEnabled = $true
    $copy.IsEnabled = $true
    $cancel.Visibility = 'Collapsed'
})

$cancel.Add_Click({
    $operation = $state.Operation
    if (!$operation -or $operation.Elevated -or $operation.Process.HasExited) { return }
    try {
        $operation.Process.Kill()
        $operation.Cancelled = $true
        $cancel.IsEnabled = $false
    } catch {
        if (!$operation.Process.HasExited) { Set-Status 'StopFailed' }
    }
})
$window.FindName('Quick').Add_Click({ Start-Operation '--check' 'QuickRunning' $false })
$window.FindName('Scan').Add_Click({ Start-Operation '--dry-run' 'ScanRunning' $false })
$window.FindName('Repair').Add_Click({ Start-Operation '--apply' 'RepairRunning' $true })
$window.FindName('RestartClient').Add_Click({ Start-Operation '--restart-client' 'RestartRunning' $false })
$window.FindName('VoiceCheck').Add_Click({ Start-Operation '--voice-check' 'VoiceCheckRunning' $false })
$window.FindName('VoiceFix').Add_Click({ Start-Operation '--voice-fix' 'VoiceFixRunning' $true })
$window.FindName('VoiceReset').Add_Click({ Start-Operation '--voice-reset' 'VoiceResetRunning' $true })
$copy.Add_Click({
    try { [Windows.Clipboard]::SetText($log.Text) } catch { Set-Status 'ClipboardBusy' }
})
$window.Add_Closing({ param($sender, $eventArgs)
    if ($state.Operation) { $eventArgs.Cancel = $true }
})

[void]$window.ShowDialog()
