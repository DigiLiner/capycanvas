$ErrorActionPreference='Stop'
$path=Join-Path $PSScriptRoot '../../../tools/performance/windows-pen-latency.ps1'
$Project='C:\private fixture\painting.capy'
$expected='painting.capy '+[char]0x00b7+' Capy Canvas'
foreach($encoding in @([Text.Encoding]::UTF8,[Text.Encoding]::GetEncoding(1252))) {
    $source=$encoding.GetString([IO.File]::ReadAllBytes($path))
    $errors=$null;$tokens=$null
    $ast=[Management.Automation.Language.Parser]::ParseInput($source,[ref]$tokens,[ref]$errors)
    if($errors.Count){throw 'Pen capture did not parse.'}
    foreach($name in @('projectTitle','modifiedProjectTitle')) {
        $assignment=$ast.Find({param($node)
            $node -is [Management.Automation.Language.AssignmentStatementAst] -and $node.Left -is [Management.Automation.Language.VariableExpressionAst] -and $node.Left.VariablePath.UserPath -eq $name
        },$true)
        if(!$assignment){throw "Pen capture has no $name assignment."}
        . ([scriptblock]::Create($assignment.Extent.Text))
    }
    if($projectTitle -cne $expected -or $modifiedProjectTitle -cne ([char]0x2022+' '+$expected)){throw 'Pen capture title checks depend on script encoding.'}
}
Add-Type -Path (Join-Path $PSScriptRoot '../../../tools/performance/WindowsPenMotion.cs')
if([Runtime.InteropServices.Marshal]::SizeOf([type][WindowsPenMotion+PenInfo]) -ne 120){throw 'Pen capture ABI layout changed.'}
foreach($angles in @(@([double]::PositiveInfinity,0),@([double]::NegativeInfinity,0),@(0,0),@(7,0),@(1,[double]::NaN),@(1,[double]::PositiveInfinity),@(1,[double]::NegativeInfinity))) {
    $rejected=$false
    try {[WindowsPenMotion]::Run(0,0,0,1,1,1,240,'unused.csv',$angles[0],$angles[1])}
    catch [Management.Automation.MethodInvocationException] {
        if($_.Exception.InnerException -isnot [ArgumentOutOfRangeException]){throw}
        $rejected=$true
    }
    if(!$rejected){throw 'Pen capture accepted an invalid angle.'}
}
Write-Output 'Pen capture title encoding, ABI and input validation regressions passed.'
