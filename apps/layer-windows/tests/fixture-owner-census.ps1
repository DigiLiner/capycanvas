param([Parameter(Mandatory)][string]$Helper,[Parameter(Mandatory)][string]$Output)
$ErrorActionPreference='Stop'
if(Test-Path $Output){throw 'Fresh owned output required'}
New-Item -ItemType Directory $Output|Out-Null
$before=$env:CAPY_FIXTURE_CONTEXT
try {
 $env:CAPY_FIXTURE_CONTEXT=Join-Path $Output 'context.json'
 Add-Type -TypeDefinition 'public static class CapyWindowApi { public static uint Owner; public static uint GetWindowThreadProcessId(System.IntPtr hwnd,out uint owner){owner=Owner;return 1;} }'
 $tokens=$null;$errors=$null;$ast=[Management.Automation.Language.Parser]::ParseFile((Resolve-Path $Helper),[ref]$tokens,[ref]$errors)
 if($errors.Count){throw 'Helper parse failed'}
 foreach($name in @('Update-CapyFixtureContext','Register-CapyFixtureWindow')){
  $node=$ast.Find({param($n)$n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $name},$true)
  . ([scriptblock]::Create($node.Extent.Text))
 }
 function Read-Snapshot($Path){Get-Content -LiteralPath $Path -Raw|ConvertFrom-Json}
 $process=Get-Process -Id $PID;[CapyWindowApi]::Owner=$PID
 $registry=Join-Path $Output "windows-$PID.json";$handle=123456
 function Register {Register-CapyFixtureWindow $process $handle $Output}
 function Expect-Failure([scriptblock]$Action){$failed=$false;try{&$Action}catch{$failed=$true};if(!$failed){throw 'Expected rejection'}}
 function Registry($Value){$Value|ConvertTo-Json -Depth 8|Set-Content -LiteralPath $registry}
 Register
 $entry=(Get-Content $env:CAPY_FIXTURE_CONTEXT -Raw|ConvertFrom-Json).windows[0]
 if($entry.process_id -ne $PID -or $entry.hwnd -ne $handle -or $entry.executable -ne $process.Path -or $null -ne $entry.window_id){throw 'Missing-registry native ownership mismatch'}
 Registry @{process_id=$PID;windows=@(@{hwnd=$handle;id=77})};Register
 $entry=(Get-Content $env:CAPY_FIXTURE_CONTEXT -Raw|ConvertFrom-Json).windows[0]
 if($entry.window_id -ne 77){throw 'Late registry was not enriched'}
 Remove-Item -LiteralPath $registry;Expect-Failure {Register}
 Registry @{process_id=$PID;windows=@(@{hwnd=$handle;id=77})};Register
 Set-Content $registry '{';Expect-Failure {Register}
 Registry @{process_id=($PID+1);windows=@(@{hwnd=$handle;id=77})};Expect-Failure {Register}
 Registry @{process_id=$PID;windows=@(@{hwnd=($handle+1);id=77})};Expect-Failure {Register}
 Registry @{process_id=$PID;windows=@(@{hwnd=$handle;id=77},@{hwnd=$handle;id=78})};Expect-Failure {Register}
 Registry @{process_id=$PID;windows=@(@{hwnd=$handle;id=78})};Expect-Failure {Register}
 Registry @{process_id=$PID;windows=@(@{hwnd=$handle;id=77})};[CapyWindowApi]::Owner=$PID+1;Expect-Failure {Register};[CapyWindowApi]::Owner=$PID
 $context=Get-Content $env:CAPY_FIXTURE_CONTEXT -Raw|ConvertFrom-Json -AsHashtable;$context.windows[0].executable='foreign.exe';$context|ConvertTo-Json -Depth 8|Set-Content $env:CAPY_FIXTURE_CONTEXT;Expect-Failure {Register}
 @{status='passed';cases=10;native_stub_scope='Own process/executable with controlled HWND owner; no app/window/input';helper_sha256=(Get-FileHash $Helper).Hash}|ConvertTo-Json|Set-Content (Join-Path $Output 'result.json')
} finally {$env:CAPY_FIXTURE_CONTEXT=$before}
