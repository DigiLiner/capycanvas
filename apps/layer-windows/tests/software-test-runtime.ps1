$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot '../scripts/WindowsArchitecture.ps1')
. (Join-Path $PSScriptRoot '../scripts/WindowsTestRuntime.ps1')
. (Join-Path $PSScriptRoot '../scripts/PortablePackage.ps1')
$repo=(Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$parent=[IO.Path]::GetFullPath((Join-Path $repo 'artifacts/windows/software-runtime-tests'))+'\'
$run=Join-Path $parent ([Guid]::NewGuid().ToString('N'))
$packages=Join-Path $run 'packages'
$deps=Join-Path $run 'deps'
$package=([xml](Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'packages.config'))).packages.package
$source=Join-Path $packages ("$($package.id).$($package.version)/build/native/bin/arm64/d3d10warp.dll")
function Write-Binary([string]$Path,[uint16]$Machine) {
    [IO.Directory]::CreateDirectory((Split-Path -Parent $Path))|Out-Null
    $bytes=[byte[]]::new(0x88)
    [BitConverter]::GetBytes([uint16]0x5A4D).CopyTo($bytes,0)
    [BitConverter]::GetBytes([uint32]0x80).CopyTo($bytes,0x3C)
    [BitConverter]::GetBytes([uint32]0x4550).CopyTo($bytes,0x80)
    [BitConverter]::GetBytes($Machine).CopyTo($bytes,0x84)
    [IO.File]::WriteAllBytes($Path,$bytes)
}
try {
    Write-Binary $source 0x8664
    $rejected=$false
    try {Install-WindowsSoftwareTestRuntime $packages $deps arm64|Out-Null} catch {$rejected=$true}
    if(!$rejected -or (Test-Path $deps)){throw 'Wrong-architecture WARP was staged.'}
    Write-Binary $source 0xAA64
    [IO.File]::WriteAllText((Join-Path (Split-Path -Parent $source) 'd3d10warp.pdb'),'private-symbols')
    $staged=Install-WindowsSoftwareTestRuntime $packages $deps arm64
    if(!$staged.created -or (Get-FileHash $staged.path).Hash -ne (Get-FileHash $source).Hash -or @(Get-ChildItem $deps -File).Count -ne 1){throw 'Test runtime staging copied incorrect files.'}
    $existing=Install-WindowsSoftwareTestRuntime $packages $deps arm64
    Remove-WindowsSoftwareTestRuntime $existing
    if($existing.created -or !(Test-Path $staged.path)){throw 'Staging removed a pre-existing matching runtime.'}
    $rejected=$false
    try {Assert-WindowsDistributionRuntime $deps arm64} catch {if($_.Exception.Message -notmatch 'cannot be redistributed'){throw};$rejected=$true}
    if(!$rejected){throw 'Portable packaging accepted development-only WARP.'}
    $manifest=@{schema=1;architecture='arm64';release_identity=$true;source_commit='runtime-fixture';files=@(@{path='d3d10warp.dll';bytes=(Get-Item $staged.path).Length;sha256=$staged.sha256})}
    $manifest|ConvertTo-Json -Depth 4|Set-Content (Join-Path $deps 'package-manifest.json')
    $result=Join-Path $run 'result.json'
    @{payload=$deps;source_commit='runtime-fixture'}|ConvertTo-Json|Set-Content $result
    $rejected=$false
    try {Read-PortablePackage $result|Out-Null} catch {if($_.Exception.Message -notmatch 'cannot be redistributed'){throw};$rejected=$true}
    if(!$rejected){throw 'MSIX and installer input accepted development-only WARP.'}
    [IO.File]::WriteAllText($staged.path,'another-owner')
    foreach($action in @({Install-WindowsSoftwareTestRuntime $packages $deps arm64|Out-Null},{Remove-WindowsSoftwareTestRuntime $staged})) {
        $rejected=$false
        try {& $action} catch {$rejected=$true}
        if(!$rejected -or [IO.File]::ReadAllText($staged.path) -ne 'another-owner'){throw 'Test runtime overwrote or removed another owner.'}
    }
    [IO.File]::WriteAllBytes($staged.path,[IO.File]::ReadAllBytes($source))
    Remove-WindowsSoftwareTestRuntime $staged
    if(Test-Path $staged.path){throw 'Owned software runtime survived cleanup.'}
    [IO.File]::WriteAllText((Join-Path $deps 'd3d10warp.pdb'),'private-symbols')
    $rejected=$false
    try {Assert-WindowsDistributionRuntime $deps arm64} catch {$rejected=$true}
    if(!$rejected){throw 'Packaging accepted development-only WARP symbols.'}
    $x64=Join-Path $packages ("$($package.id).$($package.version)/build/native/bin/x64/d3d10warp.dll")
    Write-Binary $x64 0x8664
    $staged=Install-WindowsSoftwareTestRuntime $packages (Join-Path $run 'x64-deps') x64
    if(!(Test-WindowsBinaryArchitecture $staged.path x64)){throw 'x64 WARP was not staged.'}
    Remove-WindowsSoftwareTestRuntime $staged
    Write-Output 'Software test runtime architecture, ownership, cleanup and distribution regressions passed.'
} finally {
    $actual=(Resolve-Path -LiteralPath $run).Path
    if(!$actual.StartsWith($parent,[StringComparison]::OrdinalIgnoreCase)){throw 'Unexpected test cleanup directory.'}
    if(@(Get-Item $actual)+@(Get-ChildItem $actual -Recurse -Force)|Where-Object {($_.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0}){throw 'Test cleanup refuses reparse points.'}
    Remove-Item -LiteralPath $actual -Recurse -Force
}
