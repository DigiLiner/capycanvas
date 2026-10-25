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
$repo=(Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$run=Join-Path $repo ('artifacts/windows/script-encoding/'+[Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($run)|Out-Null
$records=@([pscustomobject]@{tag='ja';title=([string][char]0x65e5+[char]0x672c+[char]0x8a9e);hint=([char]::ConvertFromUtf32(0x1f3a8)+[char]0x2026)})
$Name='captions';$layout=$records[0];$model=$records[0]
$OutputDirectory=$run;$layoutEvidence=$records[0];$elements=$records[0];$manifest=$records[0]
$sources=@(
    @{file='exercise-localization.ps1';needles=@('configuration-captions.json');outputs=@('configuration-captions.json')},
    @{file='CapyUia.ps1';needles=@("'-layout.json'","'-model.json'");outputs=@('captions-layout.json','captions-model.json')},
    @{file='capture-editor.ps1';needles=@('layout-$name.json','model-$name.json','elements-$name.json','fixtures.json');outputs=@('layout-captions.json','model-captions.json','elements-captions.json','fixtures.json')}
)
$writers=@(foreach($source in $sources){
    $ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot ('../scripts/'+$source.file)),[ref]$tokens,[ref]$errors)
    for($i=0;$i -lt $source.needles.Count;$i++){
        $needle=$source.needles[$i]
        $writer=$ast.Find({param($node)$node -is [Management.Automation.Language.PipelineAst] -and $node.Extent.Text.Contains($needle)}.GetNewClosure(),$true)
        if(!$writer){throw "Missing evidence writer in $($source.file): $needle"}
        @{node=$writer;file=$source.outputs[$i]}
    }
})
try{
    foreach($case in $writers){
        & ([scriptblock]::Create($case.node.Extent.Text))
        $bytes=[IO.File]::ReadAllBytes((Join-Path $run $case.file))
        if($bytes.Length -ge 3 -and $bytes[0] -eq 239 -and $bytes[1] -eq 187 -and $bytes[2] -eq 191){throw "Evidence JSON must omit the UTF-8 BOM: $($case.file)"}
        $actual=[Text.UTF8Encoding]::new($false,$true).GetString($bytes)|ConvertFrom-Json
        if($actual.title -cne $records[0].title -or $actual.hint -cne $records[0].hint){throw "Localization evidence lost Unicode text: $($case.file)"}
    }
}finally{foreach($case in $writers){[IO.File]::Delete((Join-Path $run $case.file))};[IO.Directory]::Delete($run)}
Write-Output "$($writers.Count) capture and localization evidence writers preserve Japanese, emoji and ellipsis as UTF-8 without BOM."
