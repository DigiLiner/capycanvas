param([Parameter(Mandatory)][object]$Root,[Parameter(Mandatory)][string]$Name,[string]$Caption,[switch]$Inspect,[switch]$PassThru)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
$id=$Name.ToLowerInvariant().Replace('application-menu-','')
if($id -notin @('file','edit','image','layer','select','filter','view','window')){throw "Unknown application menu: $Name"}
$menuRoute=@{id=$null}
$menuNodes=@()
function Visible-Control([string]$Id,$Type,[switch]$ByName){
    foreach($entry in $menuNodes){
        $current=$entry.Current
        $value=if($ByName){$current.Name}else{$current.AutomationId}
        if($value -ceq $Id -and !$current.IsOffscreen -and (!$Type -or $current.ControlType -eq $Type)){return $entry}
    }
}
function Menu-Button {
    foreach($entry in @('application-menus','application-primary-menu','header-recovery-menu')){
        $button=Visible-Control $entry ([System.Windows.Automation.ControlType]::Button)
        if($button){$menuRoute.id=$entry;return $button}
    }
}
function Submenu {
    $entry=Visible-Control ('application-menu-'+$id) ([System.Windows.Automation.ControlType]::MenuItem)
    if(!$entry -and $menuRoute.id -ne 'application-menus'){$entry=Visible-Control ('menu-'+$id) ([System.Windows.Automation.ControlType]::MenuItem)}
    if(!$entry -and $menuRoute.id -ne 'application-menus' -and $Caption){$entry=Visible-Control $Caption ([System.Windows.Automation.ControlType]::MenuItem) -ByName}
    $entry
}
$watch=[Diagnostics.Stopwatch]::StartNew()
do{
    $menuNodes=$Root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
    $button=Visible-Control ("application-menu-"+$id) ([System.Windows.Automation.ControlType]::Button)
    $overflow=if(!$button){Menu-Button}
    $submenu=if(!$button -and !$Inspect){Submenu}
    if($button -or $overflow -or $submenu){break}
    Start-Sleep -Milliseconds 50
}while($watch.Elapsed.TotalSeconds -lt 8*$(if($env:CAPY_WAIT_SCALE){[double]$env:CAPY_WAIT_SCALE}else{1}))
if($Inspect){
    if(!$button){$button=$overflow}
    if(!$button){throw 'The application menu has no visible entry point'}
    return $button
}
if($button){
    $button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    if($PassThru){$button}
    return
}
if(!$submenu){
    if(!$overflow){throw 'The compact application menu is unavailable'}
    $overflow.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    $watch=[Diagnostics.Stopwatch]::StartNew()
    do{
        $menuNodes=$Root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
        $submenu=Submenu
        if($submenu){break}
        Start-Sleep -Milliseconds 50
    }while($watch.Elapsed.TotalSeconds -lt 5)
}
if(!$submenu -or !$submenu.Current.IsEnabled){throw "Application menu is unavailable: $Name"}
$expand=$submenu.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern)
$expand.Expand()
$watch=[Diagnostics.Stopwatch]::StartNew()
while($expand.Current.ExpandCollapseState -ne [System.Windows.Automation.ExpandCollapseState]::Expanded){
    if($watch.Elapsed.TotalSeconds -ge 5){throw "Application menu did not expand: $Name"}
    Start-Sleep -Milliseconds 50
}
if($PassThru){$submenu}
