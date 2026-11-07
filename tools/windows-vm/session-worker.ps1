param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$Output,
    [Parameter(Mandatory)][string]$BuildRecord,
    [Parameter(Mandatory)][string]$QueueConfig,
    [ValidateSet('release','dev-perf')][string]$RustProfile='release',
    [ValidateRange(1,1440)][int]$TimeoutMinutes=20
)
$ErrorActionPreference='Stop'
function Set-UserValue([string]$Path,[string]$Name,[int]$Value){
    if(!(Test-Path -LiteralPath $Path)){New-Item -Path $Path -Force|Out-Null}
    New-ItemProperty -Path $Path -Name $Name -Value $Value -PropertyType DWord -Force|Out-Null
}
function Enable-JapaneseProfile{
    $tip='0411:{03B5835F-F03C-411B-9CE2-AA23E1171E36}{A76C93D9-5523-4E90-AAFA-4DB112F9AC76}'
    $languages=Get-WinUserLanguageList
    if($languages|Where-Object {$_.InputMethodTips -contains $tip}){return}
    $japanese=@($languages|Where-Object LanguageTag -eq 'ja-JP')[0]
    if(!$japanese){$japanese=(New-WinUserLanguageList -Language 'ja-JP')[0];$languages.Add($japanese)}
    if($japanese.InputMethodTips -notcontains $tip){$japanese.InputMethodTips.Add($tip)}
    Set-WinUserLanguageList -LanguageList $languages -Force
}
try{
    New-Item -ItemType Directory -Force $Output,([Environment]::GetFolderPath([Environment+SpecialFolder]::DesktopDirectory))|Out-Null
    $personalize='HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize'
    Set-UserValue $personalize AppsUseLightTheme 0
    Set-UserValue $personalize SystemUsesLightTheme 0
    Enable-JapaneseProfile
    & (Join-Path $PSScriptRoot 'run-fixtures.ps1') -Executable $Executable -Output $Output -BuildRecord $BuildRecord -QueueConfig $QueueConfig -RustProfile $RustProfile -TimeoutMinutes $TimeoutMinutes -HardwareGpu -SharedSession
}catch{
    ($_|Out-String)+$_.ScriptStackTrace|Set-Content (Join-Path $Output 'runner-error.txt')
    exit 1
}
