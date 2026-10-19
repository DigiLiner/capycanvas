$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Drawing
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot '../scripts/layer-paper.ps1'),[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Paper helper does not parse'}
$function=$ast.Find({param($node)$node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Relief-Comparison'},$true)
. ([scriptblock]::Create($function.Extent.Text))
$flat=[int[]]@(1..100|ForEach-Object {[Drawing.Color]::White.ToArgb()})
$subtle=$flat.Clone()
for($i=0;$i -lt 50;$i++){$subtle[$i]=[Drawing.Color]::FromArgb(255,254,254,254).ToArgb()}
if(!(Relief-Comparison $flat $subtle).textured){throw 'Visible low-contrast relief was rejected'}
if((Relief-Comparison $flat $flat).textured){throw 'Missing relief was accepted'}
$uniform=[int[]]@(1..100|ForEach-Object {[Drawing.Color]::FromArgb(255,254,254,254).ToArgb()})
if((Relief-Comparison $flat $uniform).textured){throw 'Uniform brightness change was accepted as relief'}
$isolated=$flat.Clone();$isolated[0]=[Drawing.Color]::Black.ToArgb()
if((Relief-Comparison $flat $isolated).textured){throw 'Isolated pixel change was accepted as relief'}
if((Relief-Comparison $flat $subtle).changed -eq 0){throw 'Persistent relief was accepted as faded'}
if((Relief-Comparison $flat $flat).changed -ne 0){throw 'Exact baseline restoration was rejected'}
$refused=$false
try{Relief-Comparison $flat @(-1)|Out-Null}catch{$refused=$true}
if(!$refused){throw 'Changed sample geometry was accepted'}
'Paper relief appearance and exact fade: 7 passed'

foreach($name in @('Texture-Open','Texture-Visible')){
    $function=$ast.Find({param($node)$node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name},$true)
    . ([scriptblock]::Create($function.Extent.Text))
}
$textureControls=@{
    'texture-apply'=[pscustomobject]@{Current=[pscustomobject]@{IsOffscreen=$false}}
    'texture-tile-0'=[pscustomobject]@{Current=[pscustomobject]@{IsOffscreen=$false;BoundingRectangle=[Drawing.RectangleF]::new(0,10,20,20)}}
    'texture-cancel'=[pscustomobject]@{Current=[pscustomobject]@{BoundingRectangle=[Drawing.RectangleF]::new(0,100,20,20)}}
}
function Find([string]$Value){
    if($Value -eq 'texture-cancel' -and !$script:CapyPopups){return $null}
    $textureControls[$Value]
}
function Control([string]$Value,[switch]$Arranged){
    $item=Find $Value
    if(!$item){throw "Missing control: $Value"}
    $item
}
function Invoke([string]$Value){}
function Wait-Until([scriptblock]$Condition,[string]$Message){if(!(&$Condition)){throw $Message}}
$script:CapyPopups=$false
$entryTokens=$null;$entryErrors=$null
$entry=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot '../scripts/exercise-layers.ps1'),[ref]$entryTokens,[ref]$entryErrors)
if($entryErrors.Count){throw 'Layers fixture does not parse'}
$scope=$entry.Find({param($node)$node -is [Management.Automation.Language.AssignmentStatementAst] -and $node.Left -is [Management.Automation.Language.VariableExpressionAst] -and $node.Left.VariablePath.UserPath -eq 'CapyPopups'},$true)
if(!$scope){throw 'Layers fixture does not configure its owned-popup scope'}
. ([scriptblock]::Create($scope.Extent.Text))
Texture-Open 'layer-texture-1'
if(!(Texture-Visible 0 ([pscustomobject]@{top=0}) 1)){throw 'A texture tile with an owned popup Cancel was not visible'}
'Paper texture owned-popup scope: 1 passed'
