$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
Add-Type -TypeDefinition 'public static class CapyWindowApi { public static uint Owner; public static uint GetWindowThreadProcessId(System.IntPtr hwnd,out uint owner){owner=Owner;return 1;} }'
$helper=Join-Path $PSScriptRoot '../scripts/CapyUia.ps1'
$tokens=$null;$errors=$null;$ast=[Management.Automation.Language.Parser]::ParseFile($helper,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Renderer helper parse failed'}
$function=$ast.Find({param($node)$node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Assert-RendererHealthy'},$true)
if(!$function){throw 'Renderer health assertion is missing'}
. ([scriptblock]::Create($function.Extent.Text))
$ProcessId=$PID;[CapyWindowApi]::Owner=$PID
function Drawing($Status){
    $root=[pscustomobject]@{Current=[pscustomobject]@{ProcessId=$PID};Status=$Status}
    $root|Add-Member ScriptMethod FindFirst {
        param($scope,$condition)
        if($scope -ne [System.Windows.Automation.TreeScope]::Descendants -or $condition.Property -ne [System.Windows.Automation.AutomationElement]::AutomationIdProperty -or $condition.Value -ne 'canvas-status'){throw 'Renderer status lookup left the owned drawing scope'}
        $this.Status
    }
    [pscustomobject]@{Handle=[IntPtr]123456;Root=$root}
}
function Status([string]$Message,[bool]$Hidden=$false,[int]$Owner=$PID){[pscustomobject]@{Current=[pscustomobject]@{Name=$Message;IsOffscreen=$Hidden;ProcessId=$Owner}}}
$fatal='canvas renderer failed: effect shader: color: Color processing produced non-finite values, invalid coverage or RGB outside the selected storage range (status 0x00000003)'
$checks=0
foreach($status in @($null,(Status 'Preparing canvas'),(Status 'Painting is unavailable. You can still save your drawing.'),(Status $fatal $true))){
    Assert-RendererHealthy $PID (Drawing $status);$checks++
}
foreach($message in @($fatal,($fatal -replace '0x00000003','0x480d0d0e'),'Unexpected render worker failure')){
    $failed=$false
    try{Assert-RendererHealthy $PID (Drawing (Status $message))}catch{if($_.Exception.Message -cne $message){throw};$failed=$true}
    if(!$failed){throw 'A visible native renderer failure was accepted'}
    $checks++
}
foreach($mismatch in @('native-owner','root-owner','status-owner')){
    $drawing=Drawing (Status $fatal)
    if($mismatch -eq 'native-owner'){[CapyWindowApi]::Owner=$PID+1}
    if($mismatch -eq 'root-owner'){$drawing.Root.Current.ProcessId=$PID+1}
    if($mismatch -eq 'status-owner'){$drawing.Root.Status.Current.ProcessId=$PID+1}
    $failed=$false
    try{Assert-RendererHealthy $PID $drawing}catch{$failed=$true}
    [CapyWindowApi]::Owner=$PID
    if(!$failed){throw 'Foreign renderer ownership was accepted'}
    $checks++
}
$window=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot '../scripts/exercise-window.ps1'),[ref]$tokens,[ref]$errors)
$close=$window.Find({param($node)$node -is [Management.Automation.Language.IfStatementAst] -and $node.Extent.Text.StartsWith("if(`$Action -eq 'Close')")},$true)
$guard=$close.Clauses[0].Item2.Statements[0]
if(!$guard -or !$guard.Extent.Text.StartsWith('Assert-RendererHealthy ')){throw 'Close must check renderer health before authorization'}
$drawing=Drawing (Status $fatal);$failed=$false
try{. ([scriptblock]::Create($guard.Extent.Text))}catch{if($_.Exception.Message -cne $fatal){throw};$failed=$true}
if(!$failed){throw 'Actual Close guard accepted the captured renderer failure'}
$checks++
Write-Output "Renderer health passed $checks cases using actual helper and Close guard; captured fatal diagnostics remain exact."
