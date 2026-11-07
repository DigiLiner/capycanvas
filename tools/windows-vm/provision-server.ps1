$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
& (Join-Path $PSScriptRoot 'password-policy.ps1') | Out-Null
$downloads = New-Item -ItemType Directory -Force (Join-Path $env:TEMP 'capycanvas-provision')

function Install-Download([string]$Uri, [string]$Name, [string]$Arguments, [int[]]$Success = @(0)) {
    $installer = Join-Path $downloads $Name
    Invoke-WebRequest $Uri -OutFile $installer -UseBasicParsing
    $process = [Diagnostics.Process]::Start([Diagnostics.ProcessStartInfo]@{ FileName = $installer; Arguments = $Arguments; UseShellExecute = $false })
    $process.WaitForExit()
    if ($process.ExitCode -notin $Success) { throw "$installer exited with $($process.ExitCode)." }
}

$release = Invoke-RestMethod 'https://api.github.com/repos/PowerShell/PowerShell/releases/latest' -UseBasicParsing
$asset = $release.assets | Where-Object name -Like 'PowerShell-*-win-x64.msi' | Select-Object -First 1
$package = Join-Path $downloads $asset.name
Invoke-WebRequest $asset.browser_download_url -OutFile $package -UseBasicParsing
$process = Start-Process msiexec.exe -ArgumentList @('/i', ('"' + $package + '"'), '/quiet', '/norestart', 'ADD_PATH=1') -Wait -PassThru
if ($process.ExitCode -notin 0, 3010) { throw "PowerShell 7 setup exited with $($process.ExitCode)." }
Install-Download 'https://aka.ms/vs/17/release/vc_redist.x64.exe' 'vc_redist.x64.exe' '/install /quiet /norestart' @(0, 3010)
Remove-Item -Recurse -Force $downloads

Add-MpPreference -ExclusionPath 'C:\capyrun'
$feature = Install-WindowsFeature RDS-RD-Server -IncludeManagementTools
if (!$feature.Success) { throw 'RD Session Host installation failed.' }
$policy = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows NT\Terminal Services'
New-Item -Path $policy -Force | Out-Null
New-ItemProperty -Path $policy -Name bEnumerateHWBeforeSW -Value 1 -PropertyType DWord -Force | Out-Null
New-ItemProperty -Path $policy -Name fSingleSessionPerUser -Value 1 -PropertyType DWord -Force | Out-Null
Set-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server' fDenyTSConnections 0
Enable-NetFirewallRule -DisplayGroup 'Remote Desktop'
$manager = 'HKCU:\Software\Microsoft\ServerManager'
New-Item -Path $manager -Force | Out-Null
New-ItemProperty -Path $manager -Name DoNotOpenServerManagerAtLogon -Value 1 -PropertyType DWord -Force | Out-Null
& cscript.exe //Nologo C:\Windows\System32\slmgr.vbs /ato
