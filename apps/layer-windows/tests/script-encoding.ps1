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
$fixture=Join-Path $PSScriptRoot '../scripts/exercise-localization.ps1'
$ast=[Management.Automation.Language.Parser]::ParseFile($fixture,[ref]$tokens,[ref]$errors)
$writer=$ast.Find({param($node)$node -is [Management.Automation.Language.PipelineAst] -and $node.Extent.Text.Contains("'configuration-captions.json'")},$true)
if(!$writer){throw 'Configuration caption evidence writer is missing'}
$repo=(Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$run=Join-Path $repo ('artifacts/windows/script-encoding/'+[Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($run)|Out-Null
$records=@([pscustomobject]@{tag='ja';title=([string][char]0x65e5+[char]0x672c+[char]0x8a9e);hint=[char]::ConvertFromUtf32(0x1f3a8)})
$evidence=Join-Path $run 'configuration-captions.json'
try{
    & ([scriptblock]::Create($writer.Extent.Text))
    $actual=[Text.UTF8Encoding]::new($false,$true).GetString([IO.File]::ReadAllBytes($evidence))|ConvertFrom-Json
    if($actual.title -cne $records[0].title -or $actual.hint -cne $records[0].hint){throw 'Configuration caption evidence lost Unicode text'}
}finally{[IO.File]::Delete($evidence);[IO.Directory]::Delete($run)}
Write-Output 'Configuration caption evidence preserves Unicode text.'
