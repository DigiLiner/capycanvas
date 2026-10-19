param([Parameter(Mandatory)][string]$Executable,[ValidateSet('touch','pen','mouse')][string]$Device='touch',[switch]$Overflow,[ValidateSet('dark','light')][string]$Theme='dark')
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'CapyUia.ps1')
$CapyPopups=$true
Add-Type -Path (Join-Path $PSScriptRoot 'RowPointerDriver.cs')
if($env:CAPY_PRIVATE_DESKTOP){[CapyRowPointer]::VerifyPrivateDesktop($env:CAPY_PRIVATE_DESKTOP)}
$CapyEach={if($env:CAPY_PRIVATE_DESKTOP){[CapyRowPointer]::VerifyPrivateDesktop($env:CAPY_PRIVATE_DESKTOP)};[CapyRowPointer]::Verify()}
$repo=(Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$Executable=(Resolve-Path -LiteralPath $Executable).Path
$run=Join-Path $repo ('artifacts/windows/switcher-drag/'+[Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($run)|Out-Null
$directory=$run;$CapyTraceDirectory=$run
function Storage {(Model).windows_workspace}
function Manager {$w=(Model).windows_workspace;if($w -and ($null -ne $w.page -or $null -ne $w.prompt)){$w}}
function Layout($Value=(Model)) {$Value.state.workspace|ConvertTo-Json -Depth 80 -Compress}
function Choose([string]$Name){
    (Control $Name -Name -Within (Control 'workspace-manager') -Type ([System.Windows.Automation.ControlType]::Button)).GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
}
function Settled {
    Wait-Until {$v=Manager;$s=Storage;$null -ne $v -and !$v.loading -and !$v.busy -and !$s.switcher_busy} 'Manager did not settle'
    if((Manager).error){throw (Manager).error}
    if((Storage).switcher_error){throw (Storage).switcher_error}
}
function Closed {
    Wait-Until {$null -eq (Manager) -and $null -eq (Find 'workspace-manager')} 'Manager did not close'
}
function Open-Manager {
    Closed
    & (Join-Path $PSScriptRoot 'open-application-menu.ps1') -Root $root -Name 'window'
    (Control 'Workspaces' -Name -Type ([System.Windows.Automation.ControlType]::MenuItem)).GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Expand()
    Invoke 'Manage Workspaces…' -Name
    Wait-Until {$null -ne (Find 'workspace-manager')} 'Manager did not open'
    Settled
}
function Preference([string]$Id,[string]$Action){
    Settled
    $revision=(Storage).switcher_revision
    Invoke ('workspace-manager-options-'+$Id)
    $item=Control ('workspace-manager-'+$Action)
    if($Action -eq 'show'){$item.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()}
    else{$item.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()}
    Wait-Until {(Storage).switcher_revision -gt $revision -or (Storage).switcher_error} 'Preference was not acknowledged'
    Settled
}
function Launch([string]$Label){
    $script:stderr=Join-Path $run ($Label+'-stderr.log')
    $script:review=Start-Process -FilePath $Executable -WorkingDirectory $directory -WindowStyle Hidden -PassThru -RedirectStandardError $stderr
    $null=$review.Handle
    @{process_id=$review.Id;run=$run}|ConvertTo-Json|Set-Content (Join-Path $repo 'artifacts/windows/switcher-drag-review.json')
    Write-Output "Owned switcher review $($review.Id) ($Label)"
    $owned=@{window=$null}
    Wait-Until {$owned.window=Owned-DrawingWindow $review;$owned.window -and (Model).brush_ready -and (Storage).ready -and !(Storage).switcher_busy} 'Switcher review did not start' 75
    $script:root=$owned.window.Root;$script:drawingWindow=$owned.window.Handle
    [CapyWindowApi]::ShowWindow($drawingWindow,3)|Out-Null
    if(Find 'Test stroke' -Name){throw 'Switcher fixture requires the production UI without smoke controls'}
}
function Close {
    & (Join-Path $PSScriptRoot 'exercise-window.ps1') -ProcessId $review.Id -WindowHandle $drawingWindow.ToInt64() -StateDirectory $run -Action Close
    if(!$review.WaitForExit(15000)){throw 'Owned switcher review did not close'}
    if($review.ExitCode -ne 0){throw "Owned switcher review exited $($review.ExitCode)"}
    if((Get-Item -LiteralPath $stderr).Length){throw 'Native stderr requires inspection'}
}
function Gesture {
    $control=Find 'workspace-manager-items'
    if($control){try{$control.Current.ItemStatus|ConvertFrom-Json}catch{}}
}
function Rows-Arranged {
    $stable=@{signature=$null;count=0}
    Wait-Until {
        $manager=Manager
        $list=Find 'workspace-manager-items'
        if(!$manager -or !$list -or $manager.loading -or $manager.busy){return $false}
        $viewport=$list.Current.BoundingRectangle;$previousTop=[double]::NegativeInfinity;$visible=@()
        foreach($row in $manager.rows){
            $item=Find ('workspace-manager-row-'+$row.id)
            if(!$item){continue}
            $bounds=$item.Current.BoundingRectangle
            if($bounds.IsEmpty -or $bounds.Height -le 0 -or $bounds.Bottom -le $viewport.Top -or $bounds.Top -ge $viewport.Bottom){continue}
            if($bounds.Top -lt $previousTop){$stable.count=0;return $false}
            $previousTop=$bounds.Top
            $visible+=($row.id+':'+$bounds.ToString())
        }
        if(!$visible.Count){return $false}
        $signature=$visible -join '|'
        if($signature -eq $stable.signature){$stable.count++}else{$stable.signature=$signature;$stable.count=0}
        $stable.count -ge 2
    } 'Native row arrangement did not match saved order'
}
function Point([string]$Id,[switch]$Grip,[switch]$Top){
    Rows-Arranged
    $control=Control ($(if($Grip){'workspace-manager-grip-'}else{'workspace-manager-row-'})+$Id)
    $b=$control.Current.BoundingRectangle
    @{x=[int]($b.X+$b.Width*$(if($Grip){.5}else{.4}));y=[int]($b.Y+$b.Height*$(if($Top){.12}else{.5}))}
}
function Press([string]$Id,[switch]$Grip){
    $at=Point $Id -Grip:$Grip
    [CapyRowPointer]::Down($Device,$at.x,$at.y)
}
function Move-To($At){
    [CapyRowPointer]::Move($At.x,$At.y)
}
function Native-TapAt($Control,[double]$Drift,[switch]$Vertical,[double]$Fraction=.5,[int]$MoveSettleMilliseconds=0){
    $bounds=$Control.Current.BoundingRectangle;$x=[int]($bounds.X+$bounds.Width*$Fraction);$y=[int]($bounds.Y+$bounds.Height/2)
    $dx=if($Vertical){0}else{[int]($Drift*$scale)};$dy=[int]($Drift*$scale)
    if($x+$dx -ge $bounds.Right -or $y+$dy -ge $bounds.Bottom){throw 'Invalid run: native row/button tap drift leaves its bounds'}
    $trace=@{device=$Device;runtime_id=@($Control.GetRuntimeId());bounds=$bounds.ToString();from=@($x,$y);to=@(($x+$dx),($y+$dy));drift_dip=$Drift;move_settle_ms=$MoveSettleMilliseconds;gesture_before=Gesture}
    [CapyRowPointer]::Down($Device,$x,$y);Start-Sleep -Milliseconds 35
    if($Drift){[CapyRowPointer]::Move($x+$dx,$y+$dy)}
    if($MoveSettleMilliseconds){Start-Sleep -Milliseconds $MoveSettleMilliseconds}
    if($MoveSettleMilliseconds){$trace.gesture_before_up=Gesture}
    [CapyRowPointer]::Up($Device -eq 'pen')
    $trace.gesture_after=Gesture
    $trace
}
function Preview-Intact {
    $current=Model
    if(!$current){throw 'Owned atomic snapshot is unavailable'}
    if($current.windows_workspace.selected -ne $preview){throw 'Pointer activity changed the selected workspace row'}
    $actual=Layout $current
    if($actual -ne $previewLayout){
        [IO.File]::WriteAllText((Join-Path $run 'expected-preview.json'),$previewLayout)
        [IO.File]::WriteAllText((Join-Path $run 'actual-preview.json'),$actual)
        throw 'Pointer activity changed the workspace preview layout'
    }
}
function Order-Intact($Order,$Revision){
    $current=(Model).windows_workspace
    if(!$current){throw 'Owned atomic snapshot is unavailable'}
    if(($current.order -join '|') -ne ($Order -join '|') -or $current.switcher_revision -ne $Revision){
        @{case=$script:gestureCase;expected_order=$Order;expected_revision=$Revision;actual_order=$current.order;
            actual_revision=$current.switcher_revision;gesture=(Gesture)}|ConvertTo-Json -Depth 8|
            Set-Content (Join-Path $run 'order-failure.json')
        throw 'Incomplete gesture persisted a reorder'
    }
}
function Preview-Select {
    (Control ('workspace-manager-row-'+$preview)).GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
    Wait-Until {(Manager).selected -eq $preview -and !(Manager).loading} 'Preview did not settle'
    $stable=@{layout=$null;count=0}
    Wait-Until {
        $layout=Layout
        if($layout -eq $stable.layout){$stable.count++}else{$stable.layout=$layout;$stable.count=0}
        $stable.count -ge 3
    } 'Native preview measurements did not settle'
    $script:previewLayout=$stable.layout
}
function Source-Id {@((Storage).order|Where-Object {$_ -ne $preview})[-1]}

function Middle-RowDrag([switch]$Grip){
    $script:gestureCase=if($Grip){'middle-grip-rejected'}else{'middle-body-rejected'}
    $saved=@((Storage).order);$revision=(Storage).switcher_revision;$source=Source-Id
    $from=Point $source -Grip:$Grip;$to=Point $saved[0] -Top
    $sourceControlId=$(if($Grip){'workspace-manager-grip-'}else{'workspace-manager-row-'})+$source
    $targetControlId='workspace-manager-row-'+$saved[0];$ids=@($sourceControlId,$targetControlId)
    $hits=@(for($i=0;$i -lt 2;$i++){
        $at=@($from,$to)[$i];$item=Control $ids[$i] -Arranged;$bounds=$item.Current.BoundingRectangle
        $viewport=(Control 'workspace-manager-items').Current.BoundingRectangle
        $native=[CapyRowPointer+Point]::new();$native.x=$at.x;$native.y=$at.y;$hwnd=[CapyRowPointer]::WindowFromPoint($native);$owner=[uint32]0
        [CapyRowPointer]::GetWindowThreadProcessId($hwnd,[ref]$owner)|Out-Null
        $hit=Input-Trace $at.x $at.y
        if($item.Current.IsOffscreen -or !$bounds.Contains($at.x,$at.y) -or !$viewport.Contains($at.x,$at.y) -or $owner -ne $review.Id -or $hit.foreground -ne $drawingWindow.ToInt64() -or !@($hit.hit|Where-Object {$_.id -eq $ids[$i] -and $_.process -eq $review.Id}).Count -or !$item.Current.BoundingRectangle.Equals($bounds)){throw 'Invalid run: middle row drag lacks its visible owned row or grip hit'}
        @{id=$ids[$i];point=$at;bounds=$bounds.ToString();runtime_id=@($item.GetRuntimeId());hwnd=$hwnd.ToInt64();hit=$hit}
    })
    $before=Gesture
    if($before.phase -ne 'idle' -or $before.source -or $before.can_drop -or $before.panning -or (Find 'workspace-manager-move-up')){throw 'Invalid run: middle row drag inherited an active contact or menu'}
    $trace=@{case=$script:gestureCase;device=$Device;button='middle';grip=[bool]$Grip;source=$source;from=$from;to=$to;hits=$hits;gesture_before=$before;order_before=$saved;revision_before=$revision;selected_before=(Manager).selected;passed=$false}
    try {
        $trace.down_called_utc=[DateTime]::UtcNow.ToString('o');$previous=0L;$requestGap=0.
        [CapyRowPointer]::Down('mouse',$from.x,$from.y,'middle');$previous=[Diagnostics.Stopwatch]::GetTimestamp()
        try {
            for($step=1;$step -le 12;$step++){
                [CapyRowPointer]::Move([int]($from.x+($to.x-$from.x)*$step/12),[int]($from.y+($to.y-$from.y)*$step/12))
                $now=[Diagnostics.Stopwatch]::GetTimestamp();$requestGap=[Math]::Max($requestGap,($now-$previous)*1000./[Diagnostics.Stopwatch]::Frequency);$previous=$now
                Start-Sleep -Milliseconds 15
            }
            [CapyRowPointer]::Up()
            $now=[Diagnostics.Stopwatch]::GetTimestamp();$requestGap=[Math]::Max($requestGap,($now-$previous)*1000./[Diagnostics.Stopwatch]::Frequency)
            $trace.up_returned_utc=[DateTime]::UtcNow.ToString('o');$trace.contact_ms=[CapyRowPointer]::ContactDurationMilliseconds;$trace.max_gap_ms=[CapyRowPointer]::MaxGapMilliseconds;$trace.request_max_gap_ms=$requestGap
        } finally {if([CapyRowPointer]::Active){[CapyRowPointer]::Cancel()}}
        Settled;Start-Sleep -Milliseconds 200
        Preview-Intact;Order-Intact $saved $revision
        $after=Gesture;$trace.gesture_after=$after;$trace.menu_after=$null -ne (Find 'workspace-manager-move-up');$trace.selected_after=(Manager).selected;$trace.order_after=@((Storage).order);$trace.revision_after=(Storage).switcher_revision
        if($after.phase -ne 'idle' -or $after.source -or $after.can_drop -or $after.panning -or $trace.menu_after -or ($after.last_release|ConvertTo-Json -Depth 8 -Compress) -ne ($before.last_release|ConvertTo-Json -Depth 8 -Compress) -or ($after.last_cancel|ConvertTo-Json -Depth 8 -Compress) -ne ($before.last_cancel|ConvertTo-Json -Depth 8 -Compress)){throw 'Middle-button row motion acquired a custom owner or changed the native menu'}
        $trace.passed=$true
    } finally {$secondaryContacts.Add($trace);$secondaryContacts|ConvertTo-Json -Depth 20|Set-Content (Join-Path $run 'secondary-contacts.json')}
    Write-Output ('mouse '+$script:gestureCase+' passed')
}

function No-Drop([switch]$Outside){
    $script:gestureCase=if($Outside){'outside'}else{'no-op'}
    $source=Source-Id;$saved=@((Storage).order);$revision=(Storage).switcher_revision
    if($Outside){
        $bounds=(Control 'workspace-manager-items').Current.BoundingRectangle
        $at=@{x=[int]($bounds.X+$bounds.Width*.4);y=[int]($bounds.Y-12)}
    }else{$at=Point $source}
    Press $source -Grip;Start-Sleep -Milliseconds 45;Move-To $at
    Wait-Until {$g=Gesture;$g.phase -eq 'dragging' -and !$g.can_drop} 'No-op or outside drop was offered as a reorder'
    $hit=Gesture
    if($hit.source -ne $source -or $hit.device -ne $Device -or !$hit.grip){throw 'No-op fixture hit the wrong source or device'}
    [CapyRowPointer]::Up()
    Wait-Until {(Gesture).phase -eq 'idle'} 'Rejected drop did not release its state'
    Preview-Intact;Order-Intact $saved $revision
}

function Drag([switch]$Grip,[switch]$Hold,[switch]$Cancel){
    $source=Source-Id;$saved=@((Storage).order);$revision=(Storage).switcher_revision
    $target=Point $saved[0] -Top
    Press $source -Grip:$Grip
    if($Hold){
        Wait-Until {(Gesture).phase -eq 'held'} "$Device hold was not recognized" 4
        if(!(Find 'workspace-manager-move-up')){throw 'Hold did not open a native context menu'}
    }else{Start-Sleep -Milliseconds 45}
    Preview-Intact;Order-Intact $saved $revision
    Move-To $target
    Wait-Until {$g=Gesture;$g.phase -eq 'dragging' -and $g.can_drop} "$Device did not drag with the original contact"
    $gesture=Gesture
    if($gesture.device -ne $Device -or $gesture.grip -ne [bool]$Grip -or $gesture.source -ne $source){throw 'Native pointer device or actual row hit differs from the fixture'}
    if(Find 'workspace-manager-move-up'){throw 'Dragging left the context menu open'}
    Preview-Intact;Order-Intact $saved $revision
    if($Cancel){
        $previousRelease=(Gesture).last_release.generation
        $previousCancel=(Gesture).last_cancel.generation
        if($Device -eq 'mouse'){[CapyRowPointer]::Key(27);[CapyRowPointer]::Up()}else{[CapyRowPointer]::Cancel()}
        Wait-Until {(Gesture).phase -eq 'idle'} 'Canceled drag did not release its state'
        $terminal=(Gesture).last_release
        if($terminal.generation -ne $previousRelease -and $terminal.commit){
            $terminal|ConvertTo-Json -Depth 6|Set-Content (Join-Path $run 'canceled-drop.json')
            throw 'Canceled pointer was submitted as a drop'
        }
        $cancellation=(Gesture).last_cancel
        $expectedReasons=if($Device -eq 'mouse'){@('escape')}else{@('pointer_canceled','capture_lost')}
        if(!$cancellation -or $cancellation.generation -eq $previousCancel -or $cancellation.reason -notin $expectedReasons){
            throw 'The fixture did not deliver the expected native cancellation'
        }
        $cancellation|ConvertTo-Json|Set-Content (Join-Path $run 'cancellation.json')
        Preview-Intact;Order-Intact $saved $revision
    }else{
        [CapyRowPointer]::Up()
        Wait-Until {(Storage).switcher_revision -gt $revision} 'Drop was not persisted'
        Settled;Preview-Intact
        if((Storage).switcher_revision -ne $revision+1 -or (Storage).order[0] -ne $source){throw 'Drop did not commit one reorder'}
    }
}
try {
    Enter-CapyEnvironment
    $env:CAPY_STORAGE_DIR=Join-Path $run 'profile'
    [IO.File]::WriteAllText((Settings-File),(@{language=@{Explicit='en'};theme=$Theme}|ConvertTo-Json -Compress))
    $env:CAPY_TRACE_UI='1';$env:CAPY_TEST_DISPLAY='1';$env:CAPY_TEST_PRIMARY='1'
    Launch $Device
    [CapyRowPointer]::SetThreadDpiAwarenessContext([IntPtr](-4))|Out-Null
    [CapyRowPointer]::Initialize([uint32]$review.Id)
    $scale=[CapyRowPointer]::GetDpiForWindow($drawingWindow)/96.
    if($scale -le 0 -or [double]::IsNaN($scale) -or [double]::IsInfinity($scale)){throw 'Invalid run: owned window DPI is unavailable'}
    if(![CapyRowPointer]::SetForegroundWindow($drawingWindow)){throw 'Could not focus owned input review'}
    $active=(Storage).id
    $preview=@((Storage).order|Where-Object {$_ -ne $active})[0]
    Open-Manager;Preview-Select
    if(!(Find ('workspace-manager-current-'+$active))){throw 'Current workspace checkmark is missing'}
    $wrong=Find ('workspace-manager-current-'+$preview)
    if($wrong -and !$wrong.Current.IsOffscreen){throw 'Dialog preview moved the current workspace checkmark'}
    $taps=[Collections.Generic.List[object]]::new()
    $secondaryContacts=[Collections.Generic.List[object]]::new()
    if($Device -eq 'mouse'){Middle-RowDrag;Middle-RowDrag -Grip}
    $bodyDrifts=if($Device -eq 'mouse'){@(0,1)}else{@(0,1,3,6)}
    foreach($drift in $bodyDrifts){
      foreach($moveSettle in $(if($drift -eq 6){@(0,35)}else{@(0)})){
        $source=Source-Id;$item=Control ('workspace-manager-row-'+$source) -Arranged;$list=Control 'workspace-manager-items';$scroll=$null
        $hasScroll=$list.TryGetCurrentPattern([System.Windows.Automation.ScrollPattern]::Pattern,[ref]$scroll)
        $beforeScroll=if($hasScroll){$scroll.Current.VerticalScrollPercent}else{$null}
        $trace=Native-TapAt $item $drift -Vertical:($drift -eq 6) -Fraction .4 -MoveSettleMilliseconds $moveSettle;$trace.case='row-body-'+$drift+$(if($moveSettle){'-settled'}else{''});$trace.expected=$source;$trace.scroll_before=$beforeScroll
        try{Wait-Until {(Manager).selected -eq $source -and !(Manager).loading} "$Device row-body $drift DIP tap no longer previews";$trace.passed=$true}
        finally{$trace.actual=(Manager).selected;$trace.gesture_final=Gesture;$trace.scroll_after=if($hasScroll){$scroll.Current.VerticalScrollPercent}else{$null};$taps.Add($trace);$taps|ConvertTo-Json -Depth 15|Set-Content (Join-Path $run 'native-taps.json')}
        Preview-Select
      }
    }
    Write-Output "$Device short click passed"
    foreach($drift in @(0,1,3)){
        $source=Source-Id;$trace=Native-TapAt (Control ('workspace-manager-options-'+$source) -Arranged) $drift;$trace.case='row-options-'+$drift
        try{Wait-Until {$null -ne (Find 'workspace-manager-move-up')} "$Device options $drift DIP tap did not open its menu";Preview-Intact;$trace.passed=$true}
        finally{$trace.gesture_final=Gesture;$taps.Add($trace);$taps|ConvertTo-Json -Depth 15|Set-Content (Join-Path $run 'native-taps.json')}
        [CapyRowPointer]::Key(27);Wait-Until {!(Find 'workspace-manager-move-up')} 'Acknowledged row options did not dismiss'
    }
    if($Device -ne 'mouse'){
        $script:gestureCase='motion-before-hold-outside'
        $saved=@((Storage).order);$revision=(Storage).switcher_revision;$source=Source-Id;$at=Point $source
        $listBounds=(Control 'workspace-manager-items').Current.BoundingRectangle;$hostBounds=$root.Current.BoundingRectangle
        $outside=@{x=[int]($listBounds.Left-12);y=$at.y}
        if($outside.x -le $hostBounds.Left -or $outside.x -ge $hostBounds.Right -or $outside.y -le $hostBounds.Top -or $outside.y -ge $hostBounds.Bottom -or $listBounds.Contains($outside.x,$outside.y)){throw 'Invalid run: no blank owned list surround for outside release'}
        Press $source;Start-Sleep -Milliseconds 45
        Move-To $outside
        [CapyRowPointer]::Up()
        Wait-Until {!(Gesture).panning} 'Native panning did not settle before the next gesture'
        @{source=$source;from=$at;to=$outside;list_bounds=$listBounds.ToString();host_bounds=$hostBounds.ToString();gesture=(Gesture);selected=(Manager).selected}|ConvertTo-Json -Depth 10|Set-Content (Join-Path $run 'outside-contact.json')
        Preview-Intact;Order-Intact $saved $revision
        if(Find 'workspace-manager-move-up'){throw 'Motion before hold opened a context menu'}
        Write-Output "$Device motion before hold passed"
        Press $source
        Wait-Until {(Gesture).phase -eq 'held'} "$Device hold was not recognized" 4
        [CapyRowPointer]::Up()
        Wait-Until {$null -ne (Find 'workspace-manager-move-up')} 'Held release did not retain its menu'
        Preview-Intact;Order-Intact $saved $revision
        [CapyRowPointer]::Key(27)
        Wait-Until {$null -eq (Find 'workspace-manager-move-up')} 'Escape did not dismiss the held menu'
        if(!(Manager)){throw 'Dismissing the menu also closed the manager'}
        Write-Output "$Device held menu release passed"
    }else{
        $saved=@((Storage).order);$revision=(Storage).switcher_revision
        Press (Source-Id);Start-Sleep -Milliseconds 1100
        if(Find 'workspace-manager-move-up'){throw 'Mouse hold opened a menu'}
        Preview-Intact;Order-Intact $saved $revision
        [CapyRowPointer]::Up()
        Wait-Until {!(Manager).loading} 'Mouse click did not settle'
        Preview-Select
    }
    Drag -Grip
    Write-Output "$Device immediate grip passed"
    Drag -Hold:($Device -ne 'mouse')
    Write-Output "$Device body reorder passed"
    Drag -Grip -Cancel
    Write-Output "$Device cancellation passed"
    No-Drop;No-Drop -Outside
    Write-Output "$Device no-op and outside drops passed"
    $source=Source-Id;$revision=(Storage).switcher_revision
    Invoke ('workspace-manager-grip-'+$source)
    Invoke 'workspace-manager-move-up'
    Wait-Until {(Storage).switcher_revision -gt $revision} 'Accessible grip command did not persist'
    Settled;Preview-Intact
    (Control ('workspace-manager-row-'+$preview)).SetFocus()
    [CapyRowPointer]::Key(38)
    Wait-Until {(Manager).selected -ne $preview -and !(Manager).loading} 'Keyboard navigation no longer previews'
    [CapyRowPointer]::Key(13)
    if(!(Manager)){throw 'Enter unexpectedly applied and closed the manager'}
    Preview-Select
    Write-Output "$Device accessible grip and keyboard preview passed"
    $at=Point (Source-Id)
    [CapyRowPointer]::RightClick($at.x,$at.y)
    Wait-Until {$null -ne (Find 'workspace-manager-move-up')} 'Secondary click did not open the row menu'
    Preview-Intact
    [CapyRowPointer]::Key(27)
    Wait-Until {$null -eq (Find 'workspace-manager-move-up')} 'Secondary menu did not dismiss'
    (Control ('workspace-manager-row-'+$preview)).SetFocus()
    [CapyRowPointer]::Key(93)
    Wait-Until {$null -ne (Find 'workspace-manager-move-up')} 'Keyboard menu action did not open the row menu'
    Preview-Intact
    [CapyRowPointer]::Key(27)
    Wait-Until {$null -eq (Find 'workspace-manager-move-up')} 'Keyboard menu did not dismiss'
    Write-Output "$Device secondary and keyboard menus passed"
    Capture ($Device+'-rows')
    Choose 'Cancel';Closed
    if((Storage).id -ne $active){throw 'Cancel changed the active workspace'}

    if($Overflow){
        for($i=1;$i -le 6;$i++){
            Open-Manager;Invoke 'workspace-manager-create'
            Wait-Until {(Manager).prompt.title -eq 'New Workspace'} 'Creation prompt missing'
            (Control 'workspace-manager-name').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue("Row overflow workspace $i")
            Choose 'Create and Switch';Closed
            Wait-Until {$s=Storage;$s.ready -and !$s.busy -and !$s.switcher_busy} 'Created workspace did not settle'
        }
        $active=(Storage).id
        Open-Manager;Preview-Select
        $scroller=(Control 'workspace-manager-items').GetCurrentPattern([System.Windows.Automation.ScrollPattern]::Pattern)
        Wait-Until {$scroller.Current.VerticallyScrollable} 'Rows did not overflow'
        $scroller.SetScrollPercent([System.Windows.Automation.ScrollPattern]::NoScroll,0)
        Wait-Until {$scroller.Current.VerticalScrollPercent -le 1} 'Rows did not scroll to the beginning'
        $saved=@((Storage).order);$revision=(Storage).switcher_revision
        if($Device -ne 'mouse'){
            $at=Point $saved[2]
            Press $saved[2];Start-Sleep -Milliseconds 45
            Move-To @{x=$at.x;y=$at.y-100}
            Start-Sleep -Milliseconds 45;[CapyRowPointer]::Up()
            Wait-Until {$scroller.Current.VerticalScrollPercent -gt 1 -and !(Gesture).panning} "$Device did not preserve ordinary row scrolling"
            Preview-Intact;Order-Intact $saved $revision
            $scroller.SetScrollPercent([System.Windows.Automation.ScrollPattern]::NoScroll,0)
            Wait-Until {$scroller.Current.VerticalScrollPercent -le 1} 'Rows did not return to the beginning'
        }
        $source=$saved[0]
        $bounds=(Control 'workspace-manager-items').Current.BoundingRectangle
        Press $source -Grip;Start-Sleep -Milliseconds 45
        Move-To @{x=[int]($bounds.X+$bounds.Width*.4);y=[int]($bounds.Bottom-5)}
        Wait-Until {$scroller.Current.VerticalScrollPercent -ge 99 -and (Gesture).phase -eq 'dragging' -and (Gesture).can_drop} 'Drag did not retain its source while scrolling to the end'
        Preview-Intact;Order-Intact $saved $revision
        [CapyRowPointer]::Up()
        Wait-Until {(Storage).switcher_revision -gt $revision} 'Scrolled drop was not persisted'
        Settled;Preview-Intact
        if((Storage).order[-1] -ne $source -or (Storage).switcher_revision -ne $revision+1){throw 'Scrolled drop did not commit one move to the end'}
        Write-Output "$Device scroll overflow and retained drag passed"
        $scroller.SetScrollPercent([System.Windows.Automation.ScrollPattern]::NoScroll,0)
        Wait-Until {$scroller.Current.VerticalScrollPercent -le 1} 'Rows did not return to the beginning'
        $script:gestureCase='source-invalidation'
        $saved=@((Storage).order);$revision=(Storage).switcher_revision;$source=$saved[0]
        $at=Point $saved[2]
        Press $source -Grip;Start-Sleep -Milliseconds 45;Move-To $at
        Wait-Until {(Gesture).phase -eq 'dragging'} 'Source invalidation review did not begin dragging'
        $search=Control 'workspace-manager-search'
        $search.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue('No matching row for input review')
        Wait-Until {(Gesture).phase -eq 'idle' -and @((Manager).rows).Count -eq 0} 'Filtering away the source did not cancel the drag'
        [CapyRowPointer]::Cancel()
        Order-Intact $saved $revision
        $search.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue('')
        Settled;Preview-Select
        Write-Output "$Device source invalidation passed"
        $script:gestureCase='minimize'
        $saved=@((Storage).order);$revision=(Storage).switcher_revision
        $at=Point $saved[2]
        Press $saved[0] -Grip;Start-Sleep -Milliseconds 45;Move-To $at
        Wait-Until {(Gesture).phase -eq 'dragging'} 'Blur review did not begin dragging'
        $previousRelease=(Gesture).last_release.generation;$previousCancel=(Gesture).last_cancel.generation
        $window=$root.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern)
        $window.SetWindowVisualState([System.Windows.Automation.WindowVisualState]::Minimized)
        Wait-Until {(Gesture).phase -eq 'idle'} 'Losing the window did not cancel the drag'
        $terminal=Gesture
        $terminal|ConvertTo-Json -Depth 8|Set-Content (Join-Path $run 'blur-cancellation.json')
        if(($terminal.last_release.generation -ne $previousRelease -and $terminal.last_release.commit) -or
            !$terminal.last_cancel -or $terminal.last_cancel.generation -eq $previousCancel){throw 'Losing the window did not cancel before committing a drop'}
        [CapyRowPointer]::Cancel()
        $window.SetWindowVisualState([System.Windows.Automation.WindowVisualState]::Normal)
        Wait-Until {$window.Current.WindowVisualState -eq [System.Windows.Automation.WindowVisualState]::Normal} 'Review window did not restore'
        Settled;Preview-Intact;Order-Intact $saved $revision
        Write-Output "$Device minimize and blur cancellation passed"
        Capture ($Device+'-overflow')
        Choose 'Cancel';Closed
        if((Storage).id -ne $active){throw 'Overflow review changed the active workspace'}
    }
    Close
    @{device=$Device;theme=$Theme;short_click='passed';pickup_and_menu='passed';grip_and_body_drop='passed';preview_preserved='passed';
        single_reorder_commit='passed';cancellation='passed';no_op_and_outside_drops='passed';accessible_grip_and_keyboard='passed';secondary_and_keyboard_menus='passed';secondary_mouse_drags=@{cases=$secondaryContacts.Count;passed=@($secondaryContacts|Where-Object passed).Count};overflow_source_invalidation_and_blur=[bool]$Overflow;zero_exit='passed';
        scope='OS-injected native input; physical digitizers and performance require separate acceptance'}|
        ConvertTo-Json|Set-Content (Join-Path $run 'results.json')
    Get-Content (Join-Path $run 'results.json')
}catch{
    try{@{case=$script:gestureCase;gesture=(Gesture);model=(Model)}|ConvertTo-Json -Depth 90|Set-Content (Join-Path $run 'failure-state.json')}catch{}
    [IO.File]::WriteAllText((Join-Path $run 'failure.txt'),($_|Out-String)+$_.ScriptStackTrace)
    throw
}finally{
    try{[CapyRowPointer]::Dispose()}catch{}
    if($review -and !$review.HasExited){if($drawingWindow){$null=[CapyWindowApi]::PostMessage($drawingWindow,0x10,[UIntPtr]::Zero,[IntPtr]::Zero)};if(!$review.WaitForExit(10000)){$review.Kill();$review.WaitForExit()}}
    Exit-CapyEnvironment
}
