$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot '../scripts/exercise-effects.ps1'),[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Effects fixture does not parse'}
$helper=$ast.Find({param($node)$node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Assert-NumericReadoutFit'},$true)
. ([scriptblock]::Create($helper.Extent.Text))
$range=[pscustomobject]@{Font='Segoe UI';Points=[double]11}
$range|Add-Member ScriptMethod GetAttributeValue {
    param($Attribute)
    if($Attribute -eq [System.Windows.Automation.TextPattern]::FontNameAttribute){return $this.Font}
    if($Attribute -eq [System.Windows.Automation.TextPattern]::FontSizeAttribute){return $this.Points}
    throw 'Unexpected text attribute'
}
$entry=[pscustomobject]@{
    Current=[pscustomobject]@{BoundingRectangle=[pscustomobject]@{Width=[double]58}}
    Text=[pscustomobject]@{DocumentRange=$range}
    Value=[pscustomobject]@{Current=[pscustomobject]@{Value='55.5'}}
}
$entry|Add-Member ScriptMethod GetCurrentPattern {
    param($Pattern)
    if($Pattern -eq [System.Windows.Automation.TextPattern]::Pattern){return $this.Text}
    if($Pattern -eq [System.Windows.Automation.ValuePattern]::Pattern){return $this.Value}
    throw 'Unexpected control pattern'
}
Assert-NumericReadoutFit $entry 1.5
$entry.Current.BoundingRectangle.Width=35.71875*1.5
$rejected=$false
try{Assert-NumericReadoutFit $entry 1.5}catch{if($_.Exception.Message -notmatch 'Numeric readout.*needs'){throw};$rejected=$true}
if(!$rejected){throw 'The retained clipped fractional readout geometry was accepted'}
'Numeric readout current and clipped geometry: 2 passed'
