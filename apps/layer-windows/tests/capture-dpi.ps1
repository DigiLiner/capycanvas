$ErrorActionPreference='Stop'
$scripts=Join-Path $PSScriptRoot '../scripts'
function Read-Ast([string]$Name){
    $tokens=$null;$errors=$null
    $ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $scripts $Name),[ref]$tokens,[ref]$errors)
    if(@($errors).Count){throw "$Name failed parsing: $($errors.Message -join '; ')"};$ast
}
function Import-NativeType($Ast,[string]$Name){
    if($Name -as [type]){return}
    $type=@($Ast.FindAll({param($n)$n -is [Management.Automation.Language.StringConstantExpressionAst] -and $n.Value.Contains('public static class '+$Name)}.GetNewClosure(),$true))
    if($type.Count -ne 1){throw "Missing actual native API declaration: $Name"};Add-Type -TypeDefinition $type[0].Value
}
$inspect=Read-Ast 'inspect-window.ps1';$localization=Read-Ast 'exercise-localization.ps1'
Import-NativeType $inspect 'CapyWindowCapture';Import-NativeType (Read-Ast 'CapyUia.ps1') 'CapyWindowApi'
Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;
public static class CapyCaptureDpiTest {
    [DllImport("user32.dll")]static extern IntPtr GetThreadDpiAwarenessContext();
    [DllImport("user32.dll")]static extern bool AreDpiAwarenessContextsEqual(IntPtr a,IntPtr b);
    public static bool Is(int context){return AreDpiAwarenessContextsEqual(GetThreadDpiAwarenessContext(),new IntPtr(context));}
}
'@
$scopes=@(@{name='inspect-window';body=$inspect.EndBlock;variable='$captureDpi';api='CapyWindowCapture';boundaries=@('$p=Get-Process','$graphics.CopyFromScreen')})
foreach($name in @('Canvas-Pixels','Stable-CanvasPixels')){
    $fn=@($localization.FindAll({param($n)$n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $name}.GetNewClosure(),$true))
    if($fn.Count -ne 1){throw "Missing actual capture function: $name"}
    $scopes+=@{name=$name;body=$fn[0].Body.EndBlock;variable=$(if($name -eq 'Canvas-Pixels'){'$dpi'}else{'$phaseDpi'});api='CapyWindowApi';boundaries=$(if($name -eq 'Canvas-Pixels'){@('Prepare-CanvasPixels','$graphics.CopyFromScreen')}else{@('Prepare-CanvasPixels','Wait-StablePixels','$after=Canvas-Projection')})}
}
$original=[CapyWindowApi]::SetThreadDpiAwarenessContext([IntPtr](-1))
try{
    foreach($scope in $scopes){
        $statements=@($scope.body.Statements)
        $entry=@($statements|Where-Object {$_ -is [Management.Automation.Language.AssignmentStatementAst] -and $_.Left.Extent.Text -eq $scope.variable})
        $guard=@($statements|Where-Object {$_ -is [Management.Automation.Language.TryStatementAst]})
        $setter="[$($scope.api)]::SetThreadDpiAwarenessContext([IntPtr](-4))"
        $restorer="[$($scope.api)]::SetThreadDpiAwarenessContext($($scope.variable))"
        if($entry.Count -ne 1 -or $guard.Count -ne 1 -or !$guard[0].Finally -or !$entry[0].Right.Extent.Text.Contains($setter) -or !$guard[0].Finally.Extent.Text.Contains($restorer) -or $entry[0].Extent.StartOffset -gt $guard[0].Extent.StartOffset){throw "$($scope.name) lacks a balanced actual physical DPI scope"}
        foreach($boundary in $scope.boundaries){if(!$guard[0].Body.Extent.Text.Contains($boundary)){throw "$($scope.name) prepares or captures outside its physical DPI scope"}}
        $probe=[scriptblock]::Create($entry[0].Extent.Text+"`ntry {if(![CapyCaptureDpiTest]::Is(-4)){throw 'Capture body lacks physical DPI'};if(`$shouldThrow){throw 'Controlled capture exception'}}finally "+$guard[0].Finally.Extent.Text)
        foreach($shouldThrow in @($false,$true)){
            $caught=$false
            try{. $probe}catch{if($_.Exception.Message -cne 'Controlled capture exception'){throw};$caught=$true}
            if($caught -ne $shouldThrow -or ![CapyCaptureDpiTest]::Is(-1)){throw "$($scope.name) leaked caller DPI on success or exception"}
        }
    }
}finally{[CapyWindowApi]::SetThreadDpiAwarenessContext($original)|Out-Null}
Write-Output 'Capture DPI scopes passed: actual inspect and localization setter/finally AST, physical preparation boundaries, success and exception restoration; no UI or GPU.'
