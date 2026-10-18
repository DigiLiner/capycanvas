param([string]$HelperPath=(Join-Path $PSScriptRoot '../scripts/CapyUia.ps1'))
$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($HelperPath,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Workspace publisher helper parse failed'}
$function=$ast.FindAll({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Workspace-Root'},$true)
if($function.Count -ne 1){throw 'Expected one workspace publisher helper'}
Invoke-Expression $function[0].Extent.Text
function Node([string]$Status='',[object[]]$Children=@(),[object[]]$Descendants=@()){
    $node=[pscustomobject]@{Current=[pscustomobject]@{ItemStatus=$Status};Children=$Children;Descendants=$Descendants;DescendantQueries=0}
    $node|Add-Member -MemberType ScriptMethod -Name FindAll -Value {
        param($Scope,$Condition)
        if($Scope -eq [System.Windows.Automation.TreeScope]::Children){return $this.Children}
        $this.DescendantQueries++;return $this.Descendants
    }
    $node
}
function Routed([object[]]$Leaves,[object[]]$Global){
    $branch=Node -Children $Leaves
    for($depth=0;$depth -lt 3;$depth++){$branch=Node -Children @($branch)}
    $branch.Descendants=@($Leaves)+@($Global);$branch
}
$publisher=Node '{"groups":[],"elements":[]}'
$unrelated=Node '{"selected":true}'
$malformed=Node '{not-json'
$caption=Node 'Size'
$root=Routed @($malformed,$unrelated,$publisher) @()
if((Workspace-Root) -ne $publisher){throw 'The local query changed publisher selection amid unrelated/malformed statuses'}
if($root.DescendantQueries -ne 0){throw 'A local publisher unnecessarily queried the global tree'}
$root=Node -Descendants @($caption,$malformed,$unrelated,$publisher)
if((Workspace-Root) -ne $publisher -or $root.DescendantQueries -ne 1){throw 'An absent child route did not preserve the original global publisher selection'}
$root=Routed @($caption,$malformed,$unrelated) @($publisher)
if((Workspace-Root) -ne $publisher -or $root.DescendantQueries -ne 1){throw 'Unrelated or malformed child statuses suppressed the original global fallback'}
$root=Routed @($caption,$unrelated) @($malformed,$unrelated)
if($null -ne (Workspace-Root) -or $root.DescendantQueries -ne 1){throw 'A genuinely missing publisher was fabricated or skipped original fallback'}
$root=Routed @($publisher) @()
$first=Workspace-Root
$replacement=Node '{"groups":[],"elements":[],"revision":2}'
$root=Routed @($replacement) @()
if((Workspace-Root) -ne $replacement -or $first -eq $replacement){throw 'A fresh root replacement reused a stale publisher'}
@{checks=5;scope='Schema discrimination, lazy original fallback, missing publisher and fresh root replacement; not UIA selector/timing proof'}|ConvertTo-Json