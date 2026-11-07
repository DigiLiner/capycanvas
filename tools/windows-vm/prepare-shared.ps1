$secure = ConvertTo-SecureString $sharedPassword -AsPlainText -Force
foreach ($account in $sharedAccounts) {
    if (!(Get-LocalUser -Name $account -ErrorAction SilentlyContinue)) {
        New-LocalUser -Name $account -Password $secure -PasswordNeverExpires -AccountNeverExpires | Out-Null
        Add-LocalGroupMember -Group 'Remote Desktop Users' -Member $account
    }
}
New-Item -ItemType Directory -Force C:\capyrun | Out-Null
$serverManager = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\Server\ServerManager'
New-Item -Path $serverManager -Force | Out-Null
New-ItemProperty -Path $serverManager -Name DoNotOpenAtLogon -Value 1 -PropertyType DWord -Force | Out-Null
$dumps = [Microsoft.Win32.Registry]::LocalMachine.CreateSubKey('SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps\CapyCanvas.exe')
try {
    $dumps.SetValue('DumpCount', 3, [Microsoft.Win32.RegistryValueKind]::DWord)
    $dumps.SetValue('DumpType', 2, [Microsoft.Win32.RegistryValueKind]::DWord)
} finally { $dumps.Dispose() }
$sshd = Join-Path $env:ProgramData 'ssh\sshd_config'
$lines = @(Get-Content -LiteralPath $sshd | Where-Object { $_ -notmatch '^\s*MaxStartups\s' })
Set-Content -LiteralPath $sshd -Value (@('MaxStartups 64') + $lines)
