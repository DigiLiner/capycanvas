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
