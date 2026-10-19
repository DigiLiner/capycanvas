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
    [pscustomobject]@{Current=[pscustomobject]@{AutomationId='application-menu-file';Name='File';ControlType=$button;IsOffscreen=$true}},
    [pscustomobject]@{Current=[pscustomobject]@{AutomationId='application-menu-file';Name='File';ControlType=$menu;IsOffscreen=$false}},
    [pscustomobject]@{Current=[pscustomobject]@{AutomationId='application-menu-file';Name='File';ControlType=$button;IsOffscreen=$false}}
)}
$Root|Add-Member ScriptMethod FindAll {
    param($Scope,$Condition)
    if($Condition -eq [System.Windows.Automation.Condition]::TrueCondition){return $this.Elements}
    @()
}
$expected=$Root.Elements[2]
if(![object]::ReferenceEquals((Visible-Control 'application-menu-file' $button),$expected)){throw 'Visible menu was lost when the provider omitted filtered property matches.'}
if(![object]::ReferenceEquals((Visible-Control 'File' $button -ByName),$expected)){throw 'Visible menu caption did not resolve.'}
if(Visible-Control 'file' $button -ByName){throw 'Caption matching changed case sensitivity.'}
if(Visible-Control 'application-menu-edit' $button){throw 'A different menu was returned.'}
$Root.Elements=@($Root.Elements[0])
if(Visible-Control 'application-menu-file' $button){throw 'An offscreen menu was returned.'}
Write-Output 'Application menu current-property query regression checks passed.'
