param([Parameter(Mandatory)][string]$Executable,[Parameter(Mandatory)][string]$Output)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot '../scripts/CapyEnvironment.ps1')
if(!$env:CAPY_PRIVATE_DESKTOP){throw 'Workflow layout regression requires the normal private test desktop.'}
[IO.Directory]::CreateDirectory($Output)|Out-Null
$result=Join-Path ([IO.Path]::GetFullPath($Output)) ([guid]::NewGuid().ToString('N')+'.json')
$names=@('CAPY_WORKFLOW_LAYOUT_FIXTURE','CAPY_ICON_ORPC_FIXTURE','CAPY_NAVIGATOR_LAYOUT','CAPY_SCROLL_INDICATORS','CAPY_FEEDBACK_SDK')
$environment=[Environment]::GetEnvironmentVariables('Process');$saved=@{}
foreach($name in $names){$saved[$name]=if($environment.Contains($name)){$environment[$name]}else{$null}}
try{
    foreach($name in $names){Restore-CapyEnvironmentValue $name $null}
    $env:CAPY_WORKFLOW_LAYOUT_FIXTURE=$result
    $process=Start-Process -FilePath $Executable -WorkingDirectory $Output -PassThru
    @{pid=$process.Id;start_ticks=$process.StartTime.ToUniversalTime().Ticks;result=$result}|ConvertTo-Json|Set-Content -LiteralPath ($result+'.process.json')
    if(!$process.WaitForExit(300000)){Stop-Process -Id $process.Id;throw 'Workflow layout terminal receipt UNREACHED: native timeout.'}
    $process.Refresh()
    if(!(Test-Path -LiteralPath $result)){throw "Workflow layout terminal receipt UNREACHED: PID $($process.Id), exit $($process.ExitCode)."}
    $receipt=Get-Content -Raw -LiteralPath $result|ConvertFrom-Json
    if($receipt.process_id -ne $process.Id -or !$receipt.terminal -or $receipt.exit_code -ne $process.ExitCode -or $receipt.exit_code -eq 2){throw 'Workflow layout measurement preconditions UNREACHED.'}
    if($receipt.measured -ne $receipt.desired){throw 'Workflow page settle retained the outgoing measured height'}
    if($receipt.exit_code -ne 0){throw 'Workflow layout regression failed.'}
    Write-Output 'Workflow page settle publishes the incoming native desired height'
}finally{foreach($name in $names){Restore-CapyEnvironmentValue $name $saved[$name]}}
