$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
$tokens=$null;$errors=$null
$path=Join-Path $PSScriptRoot '../scripts/open-application-menu.ps1'
$ast=[Management.Automation.Language.Parser]::ParseFile($path,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Application menu helper does not parse.'}
$function=$ast.Find({param($node)$node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Visible-Control'},$true)
. ([scriptblock]::Create($function.Extent.Text))
$button=[System.Windows.Automation.ControlType]::Button
$menu=[System.Windows.Automation.ControlType]::MenuItem
$Root=[pscustomobject]@{Elements=@(
    [pscustomobject]@{Current=[pscustomobject]@{AutomationId='application-menu-file';Name='File';ControlType=$button;IsEnabled=$true;IsOffscreen=$true}},
    [pscustomobject]@{Current=[pscustomobject]@{AutomationId='application-menu-file';Name='File';ControlType=$menu;IsEnabled=$true;IsOffscreen=$false}},
    [pscustomobject]@{Current=[pscustomobject]@{AutomationId='application-menu-file';Name='File';ControlType=$button;IsEnabled=$true;IsOffscreen=$false}}
)}
$Root|Add-Member ScriptMethod FindAll {
    param($Scope,$Condition)
    if($Condition -eq [System.Windows.Automation.Condition]::TrueCondition){return $this.Elements}
    @()
}
$menuNodes=$Root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
$expected=$Root.Elements[2]
if(![object]::ReferenceEquals((Visible-Control 'application-menu-file' $button),$expected)){throw 'Visible menu was lost when the provider omitted filtered property matches.'}
if(![object]::ReferenceEquals((Visible-Control 'File' $button -ByName),$expected)){throw 'Visible menu caption did not resolve.'}
if(Visible-Control 'file' $button -ByName){throw 'Caption matching changed case sensitivity.'}
if(Visible-Control 'application-menu-edit' $button){throw 'A different menu was returned.'}
$disabled=[pscustomobject]@{Current=[pscustomobject]@{AutomationId='application-menu-file';Name='File';ControlType=$button;IsEnabled=$false;IsOffscreen=$false}}
$menuNodes=@($disabled,$expected)
if(![object]::ReferenceEquals((Visible-Control 'application-menu-file' $button),$expected)){throw 'A disabled duplicate hid the actionable menu.'}
if(![object]::ReferenceEquals((Visible-Control 'application-menu-file' $button -Enabled:$false),$disabled)){throw 'Disabled menu inspection was lost.'}
$Root.Elements=@($Root.Elements[0])
$menuNodes=$Root.Elements
if(Visible-Control 'application-menu-file' $button){throw 'An offscreen menu was returned.'}
$invoke=[pscustomobject]@{Calls=0}
$invoke|Add-Member ScriptMethod Invoke {$this.Calls++}
$target=[pscustomobject]@{Current=$expected.Current;Invoker=$invoke}
$target|Add-Member ScriptMethod GetCurrentPattern {param($Pattern);$this.Invoker}
$delayed=[pscustomobject]@{Queries=0;Target=$target;Disabled=$disabled}
$delayed|Add-Member ScriptMethod FindAll {
    param($Scope,$Condition)
    if($Condition -ne [System.Windows.Automation.Condition]::TrueCondition){return @()}
    $this.Queries++
    if($this.Queries -gt 1){return @($this.Target)}
    @($this.Disabled)
}
& $path -Root $delayed -Name File
if($invoke.Calls -ne 1 -or $delayed.Queries -ne 2){throw 'Menu routes did not share one snapshot before retrying the primary button.'}
Write-Output 'Application menu current-property query regression checks passed.'
