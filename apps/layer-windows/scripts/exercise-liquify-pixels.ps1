param([Parameter(Mandatory)][string]$Executable,[ValidateSet('dark','light')][string]$Theme='dark')
$ErrorActionPreference='Stop'
$scripts=$PSScriptRoot
$runs=Join-Path (Resolve-Path (Join-Path $scripts '../../..')).Path 'artifacts/windows/test-desktop'
[IO.Directory]::CreateDirectory($runs)|Out-Null
$before=@(Get-ChildItem -LiteralPath $runs -Directory|ForEach-Object Name)
$env:CAPY_WAIT_SCALE=$null
try {
 & (Join-Path $scripts 'test-desktop.ps1') -Script (Join-Path $scripts 'exercise-idle-brush.ps1') -Parameters @{Executable=$Executable;Theme=$Theme} -TimeoutSeconds 1700
} finally {
 $created=@(Get-ChildItem -LiteralPath $runs -Directory|Where-Object {$_.Name -notin $before})
 if($created.Count -eq 1 -and $env:CAPY_FIXTURE_CONTEXT){
  $innerContext=Join-Path $created[0].FullName 'fixture.json'
  if(Test-Path -LiteralPath $innerContext){
   $inner=(Get-Content -LiteralPath $innerContext -Raw|ConvertFrom-Json).run
   if($inner -and (Test-Path -LiteralPath $inner)){
    & robocopy.exe $inner (Join-Path $created[0].FullName 'idle-brush') /E /R:0 /W:0 /NFL /NDL /NJH /NJS | Out-Null
    if($LASTEXITCODE -ge 8){throw 'Could not preserve private Liquify evidence'}
   }
  }
  @{run=$created[0].FullName}|ConvertTo-Json|Set-Content -LiteralPath $env:CAPY_FIXTURE_CONTEXT
 }
}
