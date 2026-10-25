param([Parameter(Mandatory)][string]$Executable,[ValidateSet('dark','light')][string]$Theme='dark')
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'CapyUia.ps1')
Add-Type -Path (Join-Path $PSScriptRoot 'RowPointerDriver.cs')
Add-Type -AssemblyName System.Drawing
$CapyCaptureDelay=250
$repo=(Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$Executable=(Resolve-Path -LiteralPath $Executable).Path
$directory=Split-Path -Parent $Executable
$run=Join-Path $repo ('artifacts/windows/tilt/'+[Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($run)|Out-Null
function Json($Value){ConvertTo-Json -InputObject $Value -Compress -Depth 40}
function Tilt{(Model).state.tilt_calibration}
function Points{(Tilt).editor.points}
function Curve{Json (Points)}
function Saved{Json (Model).state.settings.tilt_calibrations}
function Same-TiltProfiles($Left,$Right){
    $keys=@($Left.PSObject.Properties.Name|Sort-Object)
    if((Json $keys) -ne (Json @($Right.PSObject.Properties.Name|Sort-Object))){return $false}
    foreach($key in $keys){
        if((Json $Left.($key).axes) -ne (Json $Right.($key).axes) -or !(Same-Curve $Left.($key).response $Right.($key).response)){return $false}
    }
    $true
}
function History{
    $state=(Model).state
    Json @($state.document_file.revision,$state.document_file.modified,(@($state.commands|Where-Object id -eq 'undo')[0].enabled),(@($state.commands|Where-Object id -eq 'redo')[0].enabled))
}
function Center([string]$Id){$r=(Control $Id -Arranged).Current.BoundingRectangle;@([int]($r.Left+$r.Width/2),[int]($r.Top+$r.Height/2))}
function Native-Click([string]$Id,[string]$Device='mouse'){
    $at=Center $Id
    [CapyRowPointer]::Down($Device,$at[0],$at[1]);[CapyRowPointer]::Up()
}
function Start-Review([switch]$Maximized){
    $script:launch++
    $script:stderr=Join-Path $run "stderr-$launch.log"
    $script:review=Start-Process -FilePath $Executable -WorkingDirectory $directory -WindowStyle Hidden -PassThru -RedirectStandardError $stderr
    $null=$review.Handle;$owned=@{window=$null}
    Wait-Until {$owned.window=Owned-DrawingWindow $review;$null -ne $owned.window -and (Model).brush_ready} 'Tilt review did not start' 45
    $script:root=$owned.window.Root;$script:drawingWindow=$owned.window.Handle
    if($Maximized){Maximize-DrawingWindow}
    Use-PaintWorkspace
    if(!(Model).windows_isolated_settings){throw 'Tilt review requires isolated preferences'}
    [CapyRowPointer]::SetForegroundWindow($drawingWindow)|Out-Null
    [CapyRowPointer]::Initialize([uint32]$review.Id)
}
function Close-Review{
    [CapyRowPointer]::Dispose()
    & (Join-Path $PSScriptRoot 'exercise-window.ps1') -ProcessId $review.Id -WindowHandle $drawingWindow.ToInt64() -Action Close -DiscardUnsaved
    if((Get-Item -LiteralPath $stderr).Length){throw "Tilt native stderr requires inspection: $stderr"}
}
function Tool([string]$Id){
    $command=@((Model).state.commands|Where-Object id -eq $Id)[0]
    (Control 'drawing-canvas' -Arranged).SetFocus()
    [CapyRowPointer]::Chord([uint32]$review.Id,[uint16[]]@(0x11),0x4B)
    Wait-Until {Find 'command-search' -Visible} 'Command search did not open'
    (Control 'command-search').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($command.label)
    Wait-Until {$row=Find 'command-result-0' -Visible;$row -and $row.Current.Name.StartsWith($command.label)} "Tool search did not find $Id"
    [CapyRowPointer]::Key([uint32]$review.Id,0x0D)
    Wait-Until {!(Find 'command-search' -Visible) -and @((Model).state.commands|Where-Object {$_.id -eq $Id -and $_.selected}).Count -eq 1} "Tool did not activate: $Id"
}
function Open-Tilt{
    Invoke-Id 'settings-button'
    Wait-Until {(Model).state.settings_open} 'Preferences action was not acknowledged'
    Control 'Preferences' -Name -Type ([System.Windows.Automation.ControlType]::Window)|Out-Null
    Invoke 'Pen & Input' -Name
    $adjust=Control 'setting-action-pen_tilt';$scroll=$null
    if($adjust.TryGetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern,[ref]$scroll)){$scroll.ScrollIntoView()}
    Invoke 'setting-action-pen_tilt'
    Wait-Until {(Tilt) -and !(Find 'Preferences' -Name -Type ([System.Windows.Automation.ControlType]::Window))} 'Adjust did not replace Preferences with Pen tilt'
    Control 'pen-tilt-dialog' -Arranged|Out-Null
    Control 'pen-tilt-curve' -Arranged|Out-Null
}
function At([double]$X,[double]$Y){
    $r=(Control 'pen-tilt-curve' -Arranged).Current.BoundingRectangle
    $inset=(Tilt).editor.controls.inset*$script:scale
    @([int]($r.Left+$inset+$X*($r.Width-2*$inset)),[int]($r.Top+$inset+(1-$Y)*($r.Height-2*$inset)))
}
function Drag([string]$Device,[int[]]$From,[int[]]$To,[switch]$Cancel){
    [CapyRowPointer]::Down($Device,$From[0],$From[1])
    try{
        for($i=1;$i -le 8;$i++){[CapyRowPointer]::Move([int]($From[0]+($To[0]-$From[0])*$i/8),[int]($From[1]+($To[1]-$From[1])*$i/8));Start-Sleep -Milliseconds 30}
    }finally{if($Cancel){[CapyRowPointer]::Cancel()}else{[CapyRowPointer]::Up()}}
    $settled=@{curve='';count=0}
    Wait-Until {$value=Curve;if($value -eq $settled.curve){$settled.count++}else{$settled.curve=$value;$settled.count=0};$settled.count -ge 3} 'The released native curve did not settle'
}
function Canvas-Point{Clear-CanvasPoint 'pen-tilt-dialog'}
function Assert-Geometry{
    $script:scale=[CapyRowPointer]::GetDpiForWindow($drawingWindow)/96.
    $frame=(Control 'pen-tilt-dialog' -Arranged).Current.BoundingRectangle
    $plot=(Control 'pen-tilt-curve' -Arranged).Current.BoundingRectangle
    $preview=(Control 'pen-tilt-direction' -Arranged).Current.BoundingRectangle
    if($frame.Width/$scale -lt 383 -or [Math]::Abs($plot.Width-$plot.Height) -gt $scale -or [Math]::Abs(($plot.Left+$plot.Right-$frame.Left-$frame.Right)/2) -gt $scale){throw 'Tilt plot is not square and centered in its minimum-width utility'}
    if([Math]::Abs($preview.Width/$scale-72) -gt 1 -or [Math]::Abs($preview.Height/$scale-72) -gt 1 -or [Math]::Abs(($preview.Left+$preview.Right-$plot.Left-$plot.Right)/2) -gt $scale){throw 'Tilt preview is not 72px on the plot centerline'}
    $controls=@{}
    foreach($id in @('firmer','lighter','reset','cancel','apply','reverse-horizontal','reverse-vertical','swap-axes')){
        $r=(Control "pen-tilt-$id" -Arranged).Current.BoundingRectangle
        if([Math]::Abs($r.Height/$scale-36) -gt 1){throw "Tilt $id is not tile-height: $r at scale $scale"}
        if($id -in @('reverse-horizontal','reverse-vertical','swap-axes') -and [Math]::Abs($r.Width/$scale-36) -gt 1){throw "Tilt $id is not a square tile segment"}
        $controls[$id]=@($r.X,$r.Y,$r.Width,$r.Height)
    }
    $reset=(Control 'pen-tilt-reset' -Arranged).Current.BoundingRectangle;$cancel=(Control 'pen-tilt-cancel' -Arranged).Current.BoundingRectangle;$apply=(Control 'pen-tilt-apply' -Arranged).Current.BoundingRectangle
    if([Math]::Abs(($reset.Left-$frame.Left)/$scale-16) -gt 1 -or [Math]::Abs(($frame.Right-$apply.Right)/$scale-16) -gt 1 -or [Math]::Abs(($apply.Left-$cancel.Right)/$scale-8) -gt 1){throw 'Tilt footer does not preserve 16px gutters and 8px action spacing'}
    $first=(Control 'pen-tilt-reverse-horizontal' -Arranged).Current.BoundingRectangle;$last=(Control 'pen-tilt-swap-axes' -Arranged).Current.BoundingRectangle
    if([Math]::Abs(($first.Left+$last.Right-$preview.Left-$preview.Right)/2) -gt $scale -or [Math]::Abs(($first.Top-$preview.Bottom)/$scale-8) -gt 1){throw 'Tilt segments are not centered 8px below the preview'}
    @{theme=$Theme;scale=$scale;controls=$controls;frame=@($frame.X,$frame.Y,$frame.Width,$frame.Height);plot=@($plot.X,$plot.Y,$plot.Width,$plot.Height);preview=@($preview.X,$preview.Y,$preview.Width,$preview.Height)}|ConvertTo-Json -Depth 8|Set-Content (Join-Path $run 'geometry.json')
}
function Preview([string]$Stage,[string]$ExpectedCurve=''){
    $history=History;$draft=if($ExpectedCurve){$ExpectedCurve}else{Curve};$paperPoint=Canvas-Point
    $pixels=Wait-StablePixels {Screen-Pixels $paperPoint[0] $paperPoint[1]}
    $circle=Center 'pen-tilt-direction'
    [CapyRowPointer]::PenTilt(40,0);[CapyRowPointer]::PenHover($circle[0],$circle[1])
    Wait-Until {(Tilt).ready -and (Tilt).input_direction -and [Math]::Abs((Tilt).input_direction[0]-40./90.) -lt .015} "$Stage circle hover did not sense native tilt"
    if((Curve) -ne $draft){throw "$Stage first hover did not preserve or load the expected device response"}
    [CapyRowPointer]::PenTilt(0,60);[CapyRowPointer]::Down('pen',$circle[0],$circle[1])
    try{
        [CapyRowPointer]::Move($circle[0]+4,$circle[1]+4)
        Wait-Until {(Tilt).input_direction -and [Math]::Abs((Tilt).input_direction[1]-2./3.) -lt .015 -and (Tilt).editor.marker} "$Stage circle contact did not update the live tilt marker"
        Capture "$Stage-circle-contact" -WithModel
    }finally{[CapyRowPointer]::Up();[CapyRowPointer]::PenLeave()}
    $frame=(Control 'pen-tilt-dialog' -Arranged).Current.BoundingRectangle
    [CapyRowPointer]::PenTilt(-30,0);[CapyRowPointer]::PenHover([int]($frame.Left+90*$scale),[int]($frame.Top+24*$scale))
    Wait-Until {(Tilt).input_direction -and [Math]::Abs((Tilt).input_direction[0]+1./3.) -lt .015} "$Stage dialog header hover did not sense tilt"
    [CapyRowPointer]::PenLeave()
    $preview=(Control 'pen-tilt-direction' -Arranged).Current.BoundingRectangle
    $gap=@([int]($frame.Left+8*$scale),[int]($preview.Top+$preview.Height/2))
    [CapyRowPointer]::PenTilt(0,-40);[CapyRowPointer]::PenHover($gap[0],$gap[1]);[CapyRowPointer]::Down('pen',$gap[0],$gap[1])
    try{
        Wait-Until {(Tilt).input_direction -and [Math]::Abs((Tilt).input_direction[1]+40./90.) -lt .015} "$Stage opaque body gutter did not sense native contact"
        Capture "$Stage-body-contact" -WithModel
    }finally{[CapyRowPointer]::Up();[CapyRowPointer]::PenLeave()}
    if((Wait-StablePixels {Screen-Pixels $paperPoint[0] $paperPoint[1]}) -ne $pixels){throw "$Stage sensor-only input changed the exact canvas sample"}
    if((History) -ne $history -or (Curve) -ne $draft){throw "$Stage sensor-only input changed artwork history or the curve draft"}
}
function Set-Axes([int]$Mask,[string]$Device='mouse'){
    $names=@('reverse_horizontal','reverse_vertical','swap');$ids=@('reverse-horizontal','reverse-vertical','swap-axes')
    for($axis=0;$axis -lt 3;$axis++){
        $expected=($Mask -band (1 -shl $axis)) -ne 0
        if([bool]((Tilt).axes.($names[$axis])) -ne $expected){
            Native-Click "pen-tilt-$($ids[$axis])" $Device
            Wait-Until {[bool]((Tilt).axes.($names[$axis])) -eq $expected} 'Independent tilt segment did not publish its toggle'
        }
    }
}
try{
    Enter-CapyEnvironment
    $env:CAPY_STORAGE_DIR=Join-Path $run 'profile'
    [IO.File]::WriteAllText((Settings-File),(@{theme=$Theme;cursor='none';language=@{Explicit='en'}}|ConvertTo-Json -Depth 4))
    $env:CAPY_TRACE_UI='1';$env:CAPY_SMOKE_TEST='1';$env:CAPY_TEST_DISPLAY='1';$env:CAPY_TEST_PRIMARY='1'
    [CapyRowPointer]::SetThreadDpiAwarenessContext([IntPtr](-4))|Out-Null
    Start-Review;Tool 'lasso';Open-Tilt;Assert-Geometry
    if((Tilt).ready -or (Control 'pen-tilt-apply').Current.IsEnabled){throw 'A cold editor already claims a pen device'}
    $saved=Saved;$curve=Curve;$oldX=(Tilt).bounds.x;$frame=(Control 'pen-tilt-dialog' -Arranged).Current.BoundingRectangle
    Drag 'mouse' @([int]($frame.Left+90*$scale),[int]($frame.Top+24*$scale)) @([int]($frame.Left-110*$scale),[int]($frame.Top+44*$scale))
    Wait-Until {[Math]::Abs((Tilt).bounds.x-($oldX-200)) -lt 1 -and [Math]::Abs((Control 'pen-tilt-dialog' -Arranged).Current.BoundingRectangle.Left-($frame.Left-200*$scale)) -lt 2} 'Cold utility drag did not publish its final native placement'
    $bounds=(Tilt).bounds;$canvas=Canvas-Point
    @{bounds=$bounds;curve=(Points)}|ConvertTo-Json -Depth 8|Set-Content (Join-Path $run 'first-pen-before.json')
    [CapyRowPointer]::PenTilt(40,0);[CapyRowPointer]::Down('pen',$canvas[0],$canvas[1])
    try{
        Wait-Until {(Tilt).ready -and (Find 'pen-tilt-dialog' -Visible)} 'First Lasso penDown hid the Tilt utility'
        $after=(Tilt).bounds
        @{bounds=$after;curve=(Points)}|ConvertTo-Json -Depth 8|Set-Content (Join-Path $run 'first-pen-after.json')
        if([Math]::Abs($after.x-$bounds.x) -gt .5 -or [Math]::Abs($after.y-$bounds.y) -gt .5 -or [Math]::Abs($after.width-$bounds.width) -gt .5 -or [Math]::Abs($after.height-$bounds.height) -gt .5 -or (Curve) -ne $curve){throw 'First pen identification reset untouched placement or response'}
        Capture 'first-lasso-down' -WithModel
    }finally{[CapyRowPointer]::Up();[CapyRowPointer]::PenLeave()}
    Native-Click 'pen-tilt-lighter'
    Wait-Until {(Curve) -ne $curve} 'Controls stopped responding after first canvas contact'
    Native-Click 'pen-tilt-cancel';Wait-Until {$null -eq (Tilt)} 'Cancel did not close the first utility'
    if((Saved) -ne $saved){throw 'Cancel saved the first unbound response'}
    Close-Review
    Start-Review -Maximized;Tool 'pen'
    Wait-Until {(Model).brush_ready} 'The painting preset was not ready for sensor isolation checks' 45
    Open-Tilt;Assert-Geometry
    if((Tilt).ready){throw 'The restarted cold session identified a pen before input'}
    Preview 'cold'
    $neutral=Curve;$saved=Saved
    for($mask=0;$mask -lt 8;$mask++){
        Set-Axes $mask (@('mouse','touch','pen')[$mask%3])
        $at=Center 'pen-tilt-direction';[CapyRowPointer]::PenTilt(30,20);[CapyRowPointer]::PenHover($at[0],$at[1])
        $pose=@{view=$null}
        Wait-Until {$pose.view=Tilt;$pose.view.input_direction -and $pose.view.adjusted_direction -and $pose.view.input_direction[0] -gt 0 -and $pose.view.input_direction[1] -gt 0} 'The new diagonal native tilt pose was not acknowledged'
        $input=@($pose.view.input_direction);$expected=@($input[0],$input[1])
        if($mask -band 4){$expected=@($input[1],$input[0])}
        if($mask -band 1){$expected[0]=-$expected[0]};if($mask -band 2){$expected[1]=-$expected[1]}
        Wait-Until {$out=(Tilt).adjusted_direction;[Math]::Abs($out[0]-$expected[0]) -lt .002 -and [Math]::Abs($out[1]-$expected[1]) -lt .002} "Axes combination $mask did not transform the live direction"
        [CapyRowPointer]::PenLeave()
    }
    Capture 'axes-combined' -WithModel
    Drag 'mouse' (At .45 .55) (At .55 .65)
    Wait-Until {@(Points).Count -eq 3 -and (Curve) -ne $neutral} 'Tilt graph did not add and edit its shared control'
    $draft=Curve
    @{curve=(Points)}|ConvertTo-Json -Depth 8|Set-Content (Join-Path $run 'curve-before-pen-cancel.json')
    $point=(Points)[1]
    Drag 'pen' (At $point[0] $point[1]) (At .65 .7) -Cancel
    @{curve=(Points)}|ConvertTo-Json -Depth 8|Set-Content (Join-Path $run 'curve-after-pen-cancel.json')
    Wait-Until {(Curve) -eq $draft} 'Native pen cancellation did not restore the Tilt curve'
    Native-Click 'pen-tilt-cancel';Wait-Until {$null -eq (Tilt)} 'Cancel did not close Tilt'
    if((Saved) -ne $saved){throw 'Cancel committed draft curve or axes'}
    Open-Tilt;Drag 'touch' (At .45 .55) (At .55 .65);Set-Axes 7
    Wait-Until {@(Points).Count -eq 3} 'Touch did not edit the Tilt response'
    $applied=Curve;$axes=Json (Tilt).axes
    Native-Click 'pen-tilt-apply';Wait-Until {$null -eq (Tilt) -and (Saved) -ne $saved} 'Apply did not save Tilt calibration'
    $persisted=Saved
    Wait-Until {Same-TiltProfiles (Read-Snapshot (Settings-File)).tilt_calibrations (Model).state.settings.tilt_calibrations} 'Apply did not persist the per-device profile'
    Open-Tilt;Native-Click 'pen-tilt-reset'
    Wait-Until {(Curve) -eq $neutral -and !(Tilt).axes.reverse_horizontal -and !(Tilt).axes.reverse_vertical -and !(Tilt).axes.swap} 'Reset did not restore neutral curve and all three independent axes'
    Native-Click 'pen-tilt-close';Wait-Until {$null -eq (Tilt)} 'Close did not dismiss Tilt'
    Open-Tilt
    if((Curve) -ne $applied -or (Json (Tilt).axes) -ne $axes){throw 'Close committed an unapplied Reset'}
    Native-Click 'pen-tilt-close';Close-Review
    Start-Review -Maximized;Tool 'pen'
    Wait-Until {(Model).brush_ready} 'The painting preset was not ready for sensor isolation checks' 45
    Open-Tilt
    Preview 'persisted' $applied
    if((Saved) -ne $persisted -or (Curve) -ne $applied -or (Json (Tilt).axes) -ne $axes){throw 'Restart did not rebind the same persisted physical-source calibration'}
    Capture 'persisted-profile' -WithModel
    Native-Click 'pen-tilt-close';Close-Review
    @{theme=$Theme;cold_lasso_and_placement='passed';circle_and_header_sensor_only='passed';all_axes_combinations='passed';curve_mouse_touch_pen='passed';apply_cancel_reset_and_restart='passed';scope='WARP and isolated OS-injected input; physical pen, rotated hardware and frame performance unverified';run=$run}|ConvertTo-Json
}catch{
    try{Capture 'failure' -WithModel}catch{}
    [IO.File]::WriteAllText((Join-Path $run 'failure.txt'),($_|Out-String)+$_.ScriptStackTrace);throw
}finally{
    [CapyRowPointer]::Dispose()
    Exit-CapyEnvironment
}
