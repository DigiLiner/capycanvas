function Protect-FixtureSessionPayload([string]$Path,[string]$User){
    & icacls.exe $Path /inheritance:r /grant:r ($User+':(OI)(CI)F') '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' '*S-1-5-32-555:(OI)(CI)RX' /Q|Out-Null
    if($LASTEXITCODE){throw 'Could not protect the shared fixture payload'}
    & icacls.exe (Join-Path $Path '*') /reset /T /C /Q|Out-Null
    if($LASTEXITCODE){throw 'Could not reset the shared fixture payload permissions'}
}
function New-FixtureSessionView([string]$Payload,[string]$Repository,[string]$BuildDirectory){
    $record=Get-Content -LiteralPath (Join-Path $BuildDirectory 'fixture-build.json') -Raw|ConvertFrom-Json
    $builds=@(@{directory=$BuildDirectory;files=$record.files})
    foreach($control in $record.controls.PSObject.Properties){
        $builds+=@{directory=$BuildDirectory+'-'+$control.Name;files=$control.Value}
    }
    $excluded=@($builds|ForEach-Object {$_.directory})
    & robocopy.exe $Payload $Repository /E /MT:4 /XD $excluded /NFL /NDL /NJH /NJS /NP|Out-Null
    if($LASTEXITCODE -ge 8){throw 'Could not copy the fixture source'}
    foreach($build in $builds){
        $view=Join-Path $Repository $build.directory.Substring($Payload.Length+1)
        foreach($file in $build.files.PSObject.Properties){
            $destination=Join-Path $view $file.Name
            [IO.Directory]::CreateDirectory((Split-Path $destination))|Out-Null
            New-Item -ItemType HardLink -Path $destination -Target (Join-Path $build.directory $file.Name) -ErrorAction Stop|Out-Null
        }
    }
    $recordPath=Join-Path $Repository ((Join-Path $BuildDirectory 'fixture-build.json').Substring($Payload.Length+1))
    [IO.File]::Copy((Join-Path $BuildDirectory 'fixture-build.json'),$recordPath)
}
