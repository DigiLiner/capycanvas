$ErrorActionPreference='Stop'
function Assert-FixtureEnvironmentRestore([IO.FileInfo[]]$Files,[string]$Helper){
    $failures=[Collections.Generic.List[string]]::new()
    foreach($file in $Files){
        if($file.FullName -eq $Helper){continue}
        $tokens=$null;$errors=$null
        $ast=[Management.Automation.Language.Parser]::ParseFile($file.FullName,[ref]$tokens,[ref]$errors)
        if($errors.Count){throw "$($file.FullName): $($errors.Message -join '; ')"}
        foreach($call in $ast.FindAll({param($node)
            $node -is [Management.Automation.Language.InvokeMemberExpressionAst] -and
            $node.Member.Value -match '^SetEnvironmentVariable(?:A|W)?$'
        },$true)){
            $failures.Add("$($file.FullName):$($call.Extent.StartLineNumber): use Restore-CapyEnvironmentValue")
        }
        foreach($command in $ast.FindAll({param($node)$node -is [Management.Automation.Language.CommandAst] -and $node.GetCommandName() -eq 'Add-Type'},$true)){
            foreach($definition in @($command.CommandElements|Where-Object {$_ -is [Management.Automation.Language.StringConstantExpressionAst] -or $_ -is [Management.Automation.Language.ExpandableStringExpressionAst]})){
                $code=$definition.Value
                $direct=[regex]::IsMatch($code,'\b(?:System\.)?Environment\s*\.\s*SetEnvironmentVariable\s*\(')
                $imports=[regex]::Matches($code,'(?s)\[\s*(?:global::)?(?:\w+\.)*DllImport(?:Attribute)?\s*\((?<arguments>.*?)\)\s*\](?<declaration>[^;{}]*);')
                foreach($import in $imports){
                    if($import.Groups['declaration'].Value -match '\bSetEnvironmentVariable(?:A|W)?\s*\(' -or $import.Groups['arguments'].Value -match '\bEntryPoint\s*=\s*(?:"SetEnvironmentVariable(?:A|W)?"|nameof\s*\(\s*SetEnvironmentVariable(?:A|W)?\s*\))'){$direct=$true}
                }
                if($direct){$failures.Add("$($file.FullName):$($definition.Extent.StartLineNumber): use Restore-CapyEnvironmentValue")}
            }
        }
    }
    if($failures.Count){throw ($failures -join "`n")}
}
function Test-ShortcutsEnvironmentRestore([string]$Repo,[string]$Directory){
    . (Join-Path $Repo 'apps/layer-windows/scripts/CapyEnvironment.ps1')
    $tokens=$null;$errors=$null
    $ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $Repo 'tools/windows-vm/run-fixtures.ps1'),[ref]$tokens,[ref]$errors)
    if($errors.Count){throw ($errors.Message -join '; ')}
    $shortcuts=$ast.Find({param($node)$node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Start-Shortcuts'},$true)
    if(!$shortcuts){throw 'Missing actual Start-Shortcuts function'}
    . ([scriptblock]::Create($shortcuts.Extent.Text))
    $scripts=$Directory;$Output=$Directory;$Executable=Join-Path $Directory 'shortcuts-control.exe'
    [IO.File]::WriteAllText((Join-Path $Directory 'CapyUia.ps1'),'')
    [IO.File]::WriteAllText((Join-Path $Directory 'ui-state-7-1.json'),'{}')
    $fakeReview=[pscustomobject]@{Id=7;StartTime=[DateTime]::UtcNow.AddSeconds(-2)}
    $fakeReview|Add-Member ScriptMethod Refresh {}
    function Start-Process($FilePath,$WorkingDirectory,[switch]$PassThru){
        if($FilePath -ne $Executable -or $WorkingDirectory -ne $Directory -or !$PassThru){throw 'Shortcuts changed its process launch'}
        if($env:CAPY_SMOKE_TEST -ne '1' -or $env:CAPY_TRACE_UI -ne '1' -or $env:CAPY_STORAGE_DIR -ne (Join-Path $Output 'shortcuts-profile')){throw 'Shortcuts omitted its fixture environment'}
        if($launchFails){throw 'shortcuts-launch-control'}
        $fakeReview
    }
    function Owned-DrawingWindow($Review,[string]$TraceDirectory){
        if($Review -ne $fakeReview -or $TraceDirectory -ne $Directory){throw 'Shortcuts changed its owned drawing lookup'}
        [pscustomobject]@{Handle=[IntPtr]1}
    }
    $names=@('CAPY_SMOKE_TEST','CAPY_TRACE_UI','CAPY_STORAGE_DIR')
    $environment=[Environment]::GetEnvironmentVariables('Process')
    $saved=@{};foreach($name in $names+@('CAPY_FIXTURE_CONTEXT')){$saved[$name]=if($environment.Contains($name)){$environment[$name]}else{$null}}
    try{
        $env:CAPY_FIXTURE_CONTEXT=Join-Path $Directory 'context.json'
        foreach($initial in @($null,'fixture-original','')){foreach($launchFails in @($false,$true)){
            $expected=@{}
            foreach($name in $names){Restore-CapyEnvironmentValue $name $initial;$expected[$name]=$initial}
            $failure=$null;$arguments=@()
            try{$arguments=@(Start-Shortcuts)}catch{$failure=$_}
            if($launchFails){if(!$failure -or $failure.Exception.Message -ne 'shortcuts-launch-control'){throw "Shortcuts did not preserve the launch failure: $failure"}}
            elseif($failure){throw $failure}
            else{
                $wanted=@('-File',(Join-Path $scripts 'exercise-shortcuts.ps1'),'-ProcessId',7,'-StateFile',(Join-Path $Directory 'ui-state-7-1.json'))
                if(($arguments -join '|') -ne ($wanted -join '|')){throw 'Shortcuts changed its generated fixture arguments'}
            }
            $phase=if($launchFails){'launch failure'}else{'success'}
            $actual=[Environment]::GetEnvironmentVariables('Process')
            foreach($name in $names){
                if($null -eq $expected[$name]){
                    if($actual.Contains($name)){throw "Start-Shortcuts $phase restored absent $name as a present environment variable"}
                }elseif(!$actual.Contains($name) -or $actual[$name] -cne $expected[$name]){throw "Start-Shortcuts $phase changed saved $name"}
            }
        }}
    }finally{foreach($entry in $saved.GetEnumerator()){Restore-CapyEnvironmentValue $entry.Key $entry.Value}}
    'Actual Start-Shortcuts restores absent and non-null values after successful and failed launches; generated arguments retained.'
}
$repo=(Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$helper=Join-Path $repo 'apps/layer-windows/scripts/CapyEnvironment.ps1'
$files=@(Get-ChildItem (Join-Path $PSScriptRoot '../scripts'),$PSScriptRoot,(Join-Path $repo 'tools/windows-vm') -Filter '*.ps1' -File)
Assert-FixtureEnvironmentRestore $files $helper
$directory=Join-Path $repo ('artifacts/windows/fixture-environment/'+[guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($directory)|Out-Null
try{
    $control=Join-Path $directory 'control.ps1'
    foreach($source in @('[Environment]::SetEnvironmentVariable("name",$null)','[System.Environment]::SetEnvironmentVariable("name","value","Process")','[environment]::setenvironmentvariable("name","")')){
        [IO.File]::WriteAllText($control,$source)
        $failure=$null;try{Assert-FixtureEnvironmentRestore @(Get-Item $control) $helper}catch{$failure=$_.Exception.Message}
        if(!$failure -or !$failure.Contains('control.ps1:1: use Restore-CapyEnvironmentValue')){throw "Environment guard accepted $source"}
    }
    foreach($source in @(
        '[CapyIconEnvironment]::SetEnvironmentVariableW("name",$null)',
        'Add-Type -TypeDefinition ''public class Bad { public void Set() { System.Environment.SetEnvironmentVariable("name","value"); } }''',
        'Add-Type -TypeDefinition ''using System.Runtime.InteropServices; public class Bad { [DllImport("kernel32.dll")] public static extern bool SetEnvironmentVariableW(string name,string value); }''',
        'Add-Type -TypeDefinition ''public class Bad { [global::System.Runtime.InteropServices.DllImportAttribute("kernel32.dll",EntryPoint="SetEnvironmentVariableA")] public static extern bool Write(string name,string value); }''',
        'Add-Type -TypeDefinition ''using System.Runtime.InteropServices; public class Bad { [DllImport("kernel32.dll",EntryPoint="SetEnvironmentVariableW")] public static extern bool Delete(string name,string value); }''',
        'Add-Type -TypeDefinition ''using System.Runtime.InteropServices; public class Bad { [DllImport("kernel32.dll",EntryPoint=nameof(SetEnvironmentVariableW))] public static extern bool Write(string name,string value); }'''
    )){
        [IO.File]::WriteAllText($control,$source)
        $failure=$null;try{Assert-FixtureEnvironmentRestore @(Get-Item $control) $helper}catch{$failure=$_.Exception.Message}
        if(!$failure -or !$failure.Contains('control.ps1:1: use Restore-CapyEnvironmentValue')){throw "Environment guard accepted $source"}
    }
    [IO.File]::WriteAllText($control,'Add-Type -TypeDefinition ''using System.Runtime.InteropServices; public class Read { [DllImport("kernel32.dll",EntryPoint="GetEnvironmentVariableW")] public static extern uint Get(string name,System.IntPtr buffer,uint size); public string Value(string name) { return System.Environment.GetEnvironmentVariable(name); } }''')
    Assert-FixtureEnvironmentRestore @(Get-Item $control) $helper
    [IO.File]::WriteAllText($control,'# [Environment]::SetEnvironmentVariable("name",$null)' + "`n" + '[Environment]::GetEnvironmentVariable("name")')
    Assert-FixtureEnvironmentRestore @(Get-Item $control) $helper
    Assert-FixtureEnvironmentRestore @(Get-Item $helper) $helper
    Test-ShortcutsEnvironmentRestore $repo $directory
    'Fixture environment guard rejects direct and imported setters outside the shared helper; getters remain allowed.'
}finally{[IO.Directory]::Delete($directory,$true)}
