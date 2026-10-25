$ErrorActionPreference='Stop'
$files=Get-ChildItem (Join-Path $PSScriptRoot '../scripts'),$PSScriptRoot -Filter '*.ps1' -File
foreach($file in $files){
    $bytes=[IO.File]::ReadAllBytes($file.FullName)
    $source=[Text.UTF8Encoding]::new($false,$true).GetString($bytes)
    if($source -match '[^\x00-\x7F]' -and !($bytes.Length -ge 3 -and $bytes[0] -eq 239 -and $bytes[1] -eq 187 -and $bytes[2] -eq 191)){
        throw "Unicode PowerShell source needs a UTF-8 BOM for Windows PowerShell: $($file.Name)"
    }
    $tokens=$null;$errors=$null
    [Management.Automation.Language.Parser]::ParseFile($file.FullName,[ref]$tokens,[ref]$errors)|Out-Null
    if($errors.Count){throw "$($file.Name): $($errors.Message -join '; ')"}
}
Write-Output "PowerShell source encoding and parsing passed for $($files.Count) scripts."
