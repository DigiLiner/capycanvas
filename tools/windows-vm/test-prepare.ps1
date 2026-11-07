$ErrorActionPreference='Stop'
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'prepare.ps1'),[ref]$tokens,[ref]$errors)
if(@($errors).Count){throw ($errors|Out-String)}
& {
    param([string]$Policy)
    $now=[datetime]'2026-11-07T22:35:00Z';$user=[Environment]::UserName
    function Get-LocalUser {
        [CmdletBinding()]param([string]$Name)
        if($Name -ne $user -or $PSBoundParameters.Count -ne 1 -or $state.netCalls){throw 'Policy queried the wrong account or changed it before reading expiry'}
        $state.userReads++
        if($case.readFailure){throw 'Fixture account lookup failed'}
        $state.account
    }
    function Get-Date {$now}
    function Get-Process {
        [CmdletBinding()]param([string]$Name,[switch]$IncludeUserName)
        if($Name -ne 'explorer' -or !$IncludeUserName -or $state.netCalls){throw 'Policy changed before observing the owned desktop'}
        $state.desktopReads++
        [pscustomobject]@{UserName='fixture\other-user'}
        if($case.desktop){[pscustomobject]@{UserName=('fixture\'+$user)}}
    }
    function net.exe {
        $arguments=@($args);$state.netCalls++
        if($state.userReads -ne 1 -or $state.desktopReads -ne 1){throw 'Policy changed before observing expiry and desktop'}
        if($state.netCalls -eq 1){
            if(($arguments -join ' ') -ne 'accounts /maxpwage:unlimited'){throw 'Policy changed a password or unrelated machine setting'}
            $global:LASTEXITCODE=[int]$case.setExit
            if(!$case.setExit){$state.account.PasswordExpires=$null;$state.maximumAge='Unlimited'}
            'Fixture policy set result'
        }elseif($state.netCalls -eq 2){
            if(($arguments -join ' ') -ne 'accounts'){throw 'Policy verification changed machine settings'}
            $global:LASTEXITCODE=[int]$case.verifyExit
            'Maximum password age (days): '+$case.reported
            'Lockout duration (minutes): Unlimited'
        }else{throw 'Policy retried a failed operation'}
    }
    $previousExit=Get-Variable LASTEXITCODE -Scope Global -ErrorAction SilentlyContinue
    try{
        foreach($case in @(
            @{expiry=$now.AddMinutes(-1);desktop=$false},
            @{expiry=$now.AddMinutes(-1);desktop=$true},
            @{expiry=$now.AddDays(1);desktop=$false},
            @{expiry=$null;desktop=$true},
            @{expiry=$now.AddMinutes(-1);desktop=$false;setExit=5;error='set unlimited VM password age.*exit 5.*Fixture policy set result'},
            @{expiry=$now.AddMinutes(-1);desktop=$false;verifyExit=2;error='verify VM password age.*exit 2.*Maximum password age'},
            @{expiry=$now.AddMinutes(-1);desktop=$false;reported='42';error='maximum password age is not Unlimited.*42'},
            @{expiry=$now.AddMinutes(-1);desktop=$false;readFailure=$true;error='Fixture account lookup failed'}
        )){
            if(!$case.ContainsKey('reported')){$case.reported='Unlimited'}
            $state=@{account=[pscustomobject]@{PasswordExpires=$case.expiry};userReads=0;desktopReads=0;netCalls=0;maximumAge='42';password='retained';enabled=$true;otherPolicy='retained'}
            $expired=$null -ne $case.expiry -and $case.expiry -le $now
            $failure=$null;$output=@();try{$output=@(& ([scriptblock]::Create($Policy)))}catch{$failure=$_}
            if($case.error){
                if(!$failure -or $failure.Exception.Message -notmatch $case.error){throw "Policy failure was hidden or replaced: $failure"}
                $expectedCalls=if($case.readFailure){0}elseif($case.setExit){1}else{2}
                if($state.netCalls -ne $expectedCalls){throw 'Policy continued or retried after failure'}
            }else{
                if($failure){throw $failure}
                if($output.Count -ne 1){throw 'Policy leaked account or command output'}
                $result=$output[0]|ConvertFrom-Json
                if((($result.PSObject.Properties.Name|Sort-Object)-join ',') -ne 'desktop_ready,password_expired'){throw 'Policy receipt changed its field set'}
                if($result.password_expired -isnot [bool] -or $result.desktop_ready -isnot [bool] -or $result.password_expired -ne $expired -or $result.desktop_ready -ne $case.desktop){throw 'Policy lost the pre-repair expiry or desktop observation'}
                if($state.netCalls -ne 2 -or $state.maximumAge -ne 'Unlimited' -or $state.userReads -ne 1 -or $state.desktopReads -ne 1){throw 'Policy did not apply and verify once'}
            }
            if($state.password -ne 'retained' -or !$state.enabled -or $state.otherPolicy -ne 'retained'){throw 'Policy changed a credential, enabled state or unrelated policy'}
        }
    }finally{
        if($previousExit){Set-Variable LASTEXITCODE $previousExit.Value -Scope Global}else{Remove-Variable LASTEXITCODE -Scope Global -ErrorAction SilentlyContinue}
    }
} (Get-Content -LiteralPath (Join-Path $PSScriptRoot 'password-policy.ps1') -Raw)
$definition=$ast.Find({param($node)$node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Enable-FixtureJapaneseInput'},$true)
if(!$definition){throw 'VM preparation does not enable the fixture Japanese input profile'}
. ([scriptblock]::Create($definition.Extent.Text))
$tip='0411:{03B5835F-F03C-411B-9CE2-AA23E1171E36}{A76C93D9-5523-4E90-AAFA-4DB112F9AC76}'
function Language([string]$Tag,[string[]]$Tips){
    $values=[Collections.Generic.List[string]]::new();foreach($value in $Tips){$values.Add($value)}
    [pscustomobject]@{LanguageTag=$Tag;InputMethodTips=$values}
}
function Get-WindowsCapability([switch]$Online,[string]$Name){
    if(!$Online -or $Name -ne 'Language.Basic~~~ja-JP~0.0.1.0'){throw 'Unexpected capability lookup'}
    [pscustomobject]@{State=$script:capabilityState}
}
function Add-WindowsCapability([switch]$Online,[string]$Name){
    if(!$script:inTask){throw 'Basic installation did not run in the elevated interactive task'}
    if(!$Online -or $Name -ne 'Language.Basic~~~ja-JP~0.0.1.0'){throw 'Preparation installed an unrelated capability'}
    $script:installs++
    if($script:installFailure -eq 'throw'){throw 'Fixture installation failure'}
    if($script:installFailure -ne 'unchanged'){$script:capabilityState='Installed'}
}
function Get-Process($Name,[switch]$IncludeUserName,$Id,$ErrorAction){
    if($Id){if($Id -ne $PID){throw 'Queried an unrelated executable'};return [pscustomobject]@{Path='fixture-powershell.exe'}}
    if($Name -ne 'explorer' -or !$IncludeUserName){throw 'Unexpected desktop readiness lookup'}
    $script:explorerChecks++;[pscustomobject]@{UserName='fixture\other-user'}
    if($script:explorerChecks -ge $script:explorerAfter){[pscustomobject]@{UserName=('fixture\'+[Environment]::UserName)}}
}
function Get-Date {$script:clock}
function Start-Sleep([int]$Seconds){if($Seconds -ne 1){throw 'Unexpected readiness observation cadence'};$script:clock=$script:clock.AddSeconds($Seconds);$script:sleeps++}
function New-ScheduledTaskAction($Execute,$Argument){if(!$Execute -or $Argument -notmatch '-EncodedCommand ([A-Za-z0-9+/=]+)$'){throw 'Invalid capability action'};[pscustomobject]@{Execute=$Execute;Argument=$Argument}}
function New-ScheduledTaskPrincipal($UserId,$LogonType,$RunLevel){
    if($UserId -ne [Environment]::UserName -or $LogonType -ne 'Interactive' -or $RunLevel -ne 'Highest'){throw 'Capability task changed private user or elevation'}
    [pscustomobject]@{UserId=$UserId}
}
function New-ScheduledTaskSettingsSet([switch]$AllowStartIfOnBatteries,[switch]$DontStopIfGoingOnBatteries,$ExecutionTimeLimit){if($ExecutionTimeLimit -ne 0){throw 'Capability task acquired an installation deadline'};@{}}
function Register-ScheduledTask($TaskName,$Action,$Principal,$Settings){
    if($script:explorerChecks -lt $script:explorerAfter){throw 'Registered before the owned interactive desktop was ready'}
    if($TaskName -notmatch '^capycanvas-japanese-input-[a-f0-9]{32}$' -or $script:task){throw 'Capability task did not have a unique owned name'}
    $script:task=[pscustomobject]@{Name=$TaskName;Action=$Action;State='Ready';LastRunTime=[datetime]::MinValue;LastTaskResult=0};$script:registered++
}
function Start-ScheduledTask($TaskName){
    if($TaskName -ne $script:task.Name){throw 'Started an unrelated task'}
    $script:task.State='Running';$script:task.LastRunTime=[datetime]::MinValue.AddTicks(1);$script:inTask=$true
    try{$null=$script:task.Action.Argument -match '-EncodedCommand ([A-Za-z0-9+/=]+)$';$command=[Text.Encoding]::Unicode.GetString([Convert]::FromBase64String($Matches[1]));& ([scriptblock]::Create($command))}catch{$script:task.LastTaskResult=1}finally{$script:inTask=$false;$script:task.State='Ready'}
}
function Get-ScheduledTaskInfo($TaskName){
    if($TaskName -ne $script:task.Name){throw 'Queried unrelated task info'}
    if($script:taskObservations -and $script:task.LastRunTime -gt [datetime]::MinValue){
        if($script:observationIndex -ge $script:taskObservations.Count){throw 'Read task info again after its terminal observation'}
        $observation=$script:taskObservations[$script:observationIndex];$script:observationIndex++
        return [pscustomobject]@{LastRunTime=$script:task.LastRunTime;LastTaskResult=$observation.result}
    }
    $script:task
}
function Get-ScheduledTask($TaskName,$ErrorAction){
    if($script:task -and $TaskName -ne $script:task.Name){throw 'Queried unrelated task'}
    if($script:taskObservations -and $script:observationIndex -gt 0){
        $state=$script:taskObservations[$script:observationIndex-1].state
        if($null -eq $state){return $null}
        return [pscustomobject]@{State=$state}
    }
    $script:task
}
function Unregister-ScheduledTask($TaskName,[switch]$Confirm){if($Confirm -or $TaskName -ne $script:task.Name -or $script:task.State -ne 'Ready'){throw 'Removed an unrelated or running task'};$script:task=$null;$script:removed++}
function Get-WinUserLanguageList {if($script:capabilityState -ne 'Installed'){throw 'User input profile touched before Basic installation'};,$script:languages}
function New-WinUserLanguageList([string]$Language){
    if($Language -ne 'ja-JP'){throw 'Preparation requested an unrelated user language'}
    $result=[Collections.Generic.List[object]]::new();$result.Add((Language 'ja-JP' @($script:newTip)));,$result
}
function Set-WinUserLanguageList($LanguageList,[switch]$Force){
    if(!$Force -or ![object]::ReferenceEquals($LanguageList,$script:languages)){throw 'Preparation replaced the current user language list'}
    $script:writes++
}
$script:installFailure='';$script:inTask=$false;$script:task=$null;$script:taskObservations=$null
foreach($script:explorerAfter in @(1,2)){foreach($initialCapability in @('Installed','NotPresent')){foreach($script:newTip in @('0411:00000411',$tip)){foreach($existingJapanese in @($false,$true)){
    $script:capabilityState=$initialCapability;$script:installs=0;$script:registered=0;$script:removed=0
    $script:explorerChecks=0;$script:clock=[datetime]'2026-01-01';$script:sleeps=0
    $script:languages=[Collections.Generic.List[object]]::new();$script:writes=0
    $english=Language 'en-US' @('0409:00000409');$german=Language 'de-DE' @('0407:00000407')
    $script:languages.Add($english)
    if($existingJapanese){$script:languages.Add((Language 'ja-JP' @('0411:00000411')))}
    $script:languages.Add($german)
    Enable-FixtureJapaneseInput
    $order=($script:languages|ForEach-Object LanguageTag)-join ','
    $expected=if($existingJapanese){'en-US,ja-JP,de-DE'}else{'en-US,de-DE,ja-JP'}
    $japanese=@($script:languages|Where-Object LanguageTag -eq 'ja-JP')[0]
    $originalTip=if($existingJapanese){'0411:00000411'}else{$script:newTip}
    if($order -ne $expected -or @($japanese.InputMethodTips|Where-Object {$_ -eq $tip}).Count -ne 1 -or $japanese.InputMethodTips -notcontains $originalTip){throw 'Japanese preparation reordered languages or replaced existing input methods'}
    if(![object]::ReferenceEquals($script:languages[0],$english) -or ($english.InputMethodTips -join ',') -ne '0409:00000409' -or ($german.InputMethodTips -join ',') -ne '0407:00000407'){throw 'Japanese preparation changed existing user languages'}
    if($script:writes -ne 1){throw 'Missing Japanese input profile was not persisted once'}
    Enable-FixtureJapaneseInput
    if($script:writes -ne 1 -or $script:languages.Count -ne 3){throw 'Japanese preparation was not idempotent'}
    $expectedInstalls=if($initialCapability -eq 'Installed'){0}else{1}
    if($script:installs -ne $expectedInstalls -or $script:capabilityState -ne 'Installed'){throw 'Basic capability installation was missing, repeated or unconditional'}
    if($script:registered -ne $expectedInstalls -or $script:removed -ne $expectedInstalls -or $script:task){throw 'Capability task was repeated, unconditional or left registered'}
    if($script:sleeps -ne $expectedInstalls*($script:explorerAfter-1)){throw 'Preparation ignored desktop readiness or waited with no missing capability'}
}}}}
foreach($script:installFailure in @('throw','unchanged')){
    $script:capabilityState='NotPresent';$script:installs=0;$script:registered=0;$script:removed=0;$script:writes=0;$failed=$false
    $script:explorerAfter=1;$script:explorerChecks=0;$script:clock=[datetime]'2026-01-01';$script:sleeps=0
    try{Enable-FixtureJapaneseInput}catch{$failed=$true}
    if(!$failed -or $script:installs -ne 1 -or $script:writes -ne 0){throw 'Failed Basic installation continued into user profile changes'}
    if($script:registered -ne 1 -or $script:removed -ne 1 -or $script:task){throw 'Failed capability task escaped owned cleanup'}
}
foreach($terminalResult in @(0,1)){
    $script:installFailure=if($terminalResult){'throw'}else{''}
    $script:capabilityState='NotPresent';$script:installs=0;$script:registered=0;$script:removed=0;$script:writes=0
    $script:explorerAfter=1;$script:explorerChecks=0;$script:clock=[datetime]'2026-01-01';$script:sleeps=0
    $script:languages=[Collections.Generic.List[object]]::new();$script:languages.Add((Language 'en-US' @('0409:00000409')))
    $script:observationIndex=0;$script:taskObservations=@(
        @{state=$null;result=267009},
        @{state='Ready';result=267009},
        @{state='Running';result=0},
        @{state='Ready';result=$terminalResult}
    )
    $failure=$null;try{Enable-FixtureJapaneseInput}catch{$failure=$_}
    if($script:observationIndex -ne 4 -or $script:sleeps -ne 3){throw "Preparation accepted an incomplete scheduler observation: $failure"}
    if($terminalResult){
        if(!$failure -or $failure.Exception.Message -notmatch 'failed: 1$' -or $script:writes -ne 0){throw 'Completed installation failure was not preserved'}
    }elseif($failure -or $script:writes -ne 1){throw "Completed installation did not enable its input profile: $failure"}
    if($script:registered -ne 1 -or $script:installs -ne 1 -or $script:removed -ne 1 -or $script:task){throw 'Scheduler observations repeated installation or escaped owned cleanup'}
    $script:taskObservations=$null
}
$script:installFailure='';$script:capabilityState='NotPresent';$script:installs=0;$script:registered=0;$script:writes=0
$script:explorerAfter=[int]::MaxValue;$script:explorerChecks=0;$script:clock=[datetime]'2026-01-01';$script:sleeps=0;$failed=$false
try{Enable-FixtureJapaneseInput}catch{$failed=$true}
if(!$failed -or $script:registered -ne 0 -or $script:installs -ne 0 -or $script:writes -ne 0 -or $script:sleeps -ne 120){throw 'Missing interactive desktop escaped its existing120s deadline or launched work'}
Write-Output 'VM preparation preserves user language order and existing tips while enabling Microsoft Japanese IME.'
