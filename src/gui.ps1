$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName PresentationFramework, PresentationCore, WindowsBase

[xml]$layout = @'
<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        Title="Riot Fix" Width="940" Height="640" MinWidth="810" MinHeight="595"
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
        <StackPanel>
          <TextBlock Text="Riot Fix" FontSize="35" FontWeight="SemiBold"/>
          <TextBlock Text="Подключение и голосовой чат VALORANT" Foreground="#B5C5DA"/>
        </StackPanel>
      </DockPanel>
    </Border>
    <StackPanel DockPanel.Dock="Left" x:Name="Actions" Width="235" Margin="0,0,19,0">
      <TextBlock Text="Подключение" FontSize="17" FontWeight="SemiBold" Margin="0,0,0,9"/>
      <Button x:Name="Quick" Content="Проверить Riot" Foreground="#10263B" Background="#8BD0FF" BorderBrush="#8BD0FF"/>
      <Button x:Name="Scan" Content="Полная диагностика"/>
      <Button x:Name="Repair" Content="Применить общий фикс"/>
      <Border BorderBrush="#33445A" BorderThickness="0,0,0,1" Margin="0,9,0,14"/>
      <TextBlock Text="Голосовой чат" FontSize="17" FontWeight="SemiBold" Margin="0,0,0,9"/>
      <Button x:Name="VoiceCheck" Content="Проверить маршрут войса"/>
      <Button x:Name="VoiceFix" Content="Применить фикс войса"/>
      <Button x:Name="VoiceReset" Content="Отменить фикс войса"/>
      <TextBlock Text="Для применения и отката нужны права администратора."
                 TextWrapping="Wrap" Foreground="#B5C5DA" Margin="0,8,8,0" FontSize="12"/>
    </StackPanel>
    <Border Background="#151F2E" BorderBrush="#33445A" BorderThickness="1" CornerRadius="8" Padding="17">
      <DockPanel>
        <TextBlock DockPanel.Dock="Top" x:Name="Status" Text="Готово к проверке" FontSize="21" FontWeight="SemiBold"/>
        <ProgressBar DockPanel.Dock="Top" x:Name="Progress" Height="4" Margin="0,10,0,12"
                     Foreground="#68BAF7" Background="#28384D"/>
        <Button DockPanel.Dock="Bottom" x:Name="Copy" Content="Скопировать журнал"
                HorizontalAlignment="Right" Margin="0,10,0,0" Height="33" IsEnabled="False"/>
        <Button DockPanel.Dock="Bottom" x:Name="Cancel" Content="Остановить проверку"
                HorizontalAlignment="Left" Margin="0,7,0,0" Height="33" Visibility="Collapsed"/>
        <TextBox x:Name="Log" IsReadOnly="True" AcceptsReturn="True" TextWrapping="Wrap"
                 AutomationProperties.Name="Журнал диагностики" VerticalScrollBarVisibility="Auto"
                 FontFamily="Consolas" FontSize="13" Background="#0F1724" Foreground="#D3DFF0"
                 BorderBrush="#304157" Padding="10" Text="Выбери действие слева. Здесь появятся результаты."/>
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
$state = @{ Operation = $null }
$timer = [Windows.Threading.DispatcherTimer]::new()
$timer.Interval = [TimeSpan]::FromMilliseconds(100)

function Start-Operation([string]$command, [string]$caption, [bool]$elevate) {
    $info = [Diagnostics.ProcessStartInfo]::new($env:RIOT_FIX_GUI_EXE, "$command --no-pause")
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
        $status.Text = 'Действие не запущено'
        $log.Text = $_.Exception.GetBaseException().Message
        $copy.IsEnabled = $true
        return
    }
    $operation = @{ Process = $process; Elevated = $elevate; Command = $command }
    if ($elevate) {
        $log.Text = 'Прогресс выводится в окне администратора.'
    } else {
        $operation.Streams = @(
            @{ Reader = $process.StandardOutput; Pending = $process.StandardOutput.ReadLineAsync() },
            @{ Reader = $process.StandardError; Pending = $process.StandardError.ReadLineAsync() }
        )
        $log.Text = 'Выполняется прямая проверка…'
        $cancel.Visibility = 'Visible'
        $cancel.IsEnabled = $true
    }
    $status.Text = $caption
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
        if (!$operation.HasOutput) { $log.Clear() }
        $operation.HasOutput = $true
        $log.AppendText(($newLines -join [Environment]::NewLine) + [Environment]::NewLine)
        $log.ScrollToEnd()
    }
    if (!$operation.Process.HasExited) { return }
    if ($operation.Streams | Where-Object { $_.Pending }) { return }
    $timer.Stop()
    $code = $operation.Process.ExitCode
    if ($operation.Elevated) {
        $log.Text = if ($code -eq 0) {
            switch ($operation.Command) {
                '--voice-fix' { 'Команда завершилась. Проверь вход в голосовой канал VALORANT.' }
                '--voice-reset' { 'Запись фикса войса удалена.' }
                default { 'Команда завершилась. Перезапусти Riot Client и проверь игру.' }
            }
        } else {
            "Код завершения $code. Запусти диагностику, чтобы посмотреть состояние подключения."
        }
    }
    $status.Text = if ($operation.Cancelled) { 'Проверка остановлена' } else { switch ($code) {
        0 { 'Готово' }
        2 { 'Есть сетевые предупреждения' }
        default { 'Действие завершилось с ошибкой' }
    } }
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
        if (!$operation.Process.HasExited) { $status.Text = 'Не удалось остановить проверку' }
    }
})
$window.FindName('Quick').Add_Click({ Start-Operation '--check' 'Проверяем подключение к Riot…' $false })
$window.FindName('Scan').Add_Click({ Start-Operation '--dry-run' 'Проверяем сеть и Zapret…' $false })
$window.FindName('Repair').Add_Click({ Start-Operation '--apply' 'Применяем общий фикс…' $true })
$window.FindName('VoiceCheck').Add_Click({ Start-Operation '--voice-check' 'Проверяем адреса голосового сервиса…' $false })
$window.FindName('VoiceFix').Add_Click({ Start-Operation '--voice-fix' 'Проверяем и применяем маршрут…' $true })
$window.FindName('VoiceReset').Add_Click({ Start-Operation '--voice-reset' 'Удаляем запись для войса…' $true })
$copy.Add_Click({
    try { [Windows.Clipboard]::SetText($log.Text) } catch { $status.Text = 'Буфер обмена занят' }
})
$window.Add_Closing({ param($sender, $eventArgs)
    if ($state.Operation) { $eventArgs.Cancel = $true }
})

[void]$window.ShowDialog()
