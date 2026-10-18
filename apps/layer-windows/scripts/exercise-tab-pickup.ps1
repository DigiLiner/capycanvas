param([Parameter(Mandatory)][string]$Executable,[ValidateSet('touch','pen','mouse')][string]$Device='touch')
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'CapyUia.ps1')
$CapyFind='prefer-visible';$CapyPopups=$true;$CapyEach={[CapyRowPointer]::Verify()}
Add-Type -Path (Join-Path $PSScriptRoot 'RowPointerDriver.cs')
$repo=(Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$Executable=(Resolve-Path -LiteralPath $Executable).Path
$directory=Split-Path -Parent $Executable
$run=Join-Path $repo ('artifacts/windows/tab-pickup/'+[Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($run)|Out-Null
function Point([string]$Id,[switch]$Name) {
    $stable=@{bounds=$null;count=0}
    Wait-Until {
        $item=Find $Id -Name:$Name
        if(!$item -or $item.Current.IsOffscreen){return $false}
        $bounds=$item.Current.BoundingRectangle
        if($bounds.IsEmpty -or $bounds.Width -le 0 -or $bounds.Height -le 0){return $false}
        if($bounds -eq $stable.bounds){$stable.count++}else{$stable.bounds=$bounds;$stable.count=0}
        $stable.count -ge 2
    } "Source $Id did not arrange visibly"
    # A row-root source uses its transparent left padding, outside child buttons.
    $x=if($Id -like 'layer-row-*'){$stable.bounds.X+2}else{$stable.bounds.X+$stable.bounds.Width*.5}
    @{x=[int]$x;y=[int]($stable.bounds.Y+$stable.bounds.Height*.5)}
}

function Gesture {try{(Workspace-Root).Current.HelpText|ConvertFrom-Json}catch{}}
function Presentation {try{(Workspace-Root).Current.ItemStatus|ConvertFrom-Json}catch{}}
function Layout {(Model).layout|ConvertTo-Json -Depth 80 -Compress}
function Tab-Placement {((Model).state.workspace.layout|ConvertTo-Json -Depth 80 -Compress) -replace '"active":"[^"]*"','"active":null'}
function Panel-Group([string]$Panel) {(Model).layout.groups|Where-Object {$_.panels -contains $Panel}}
function Workspace-History([string]$Id) {
    & (Join-Path $PSScriptRoot 'open-application-menu.ps1') -Root $root -Name 'window'
    Invoke $Id
}
function Tap([string]$Id) {
    $at=Point $Id
    [CapyRowPointer]::Down($Device,$at.x,$at.y)
    Start-Sleep -Milliseconds 35
    [CapyRowPointer]::Up()
}
function Drifting-TabTaps([string]$Prefix,[string[]]$Panels,[switch]$Drawer) {
    $before=Tab-Placement
    $expanded=(Model).state.customization.expanded|ConvertTo-Json -Depth 30 -Compress
    $drifts=@(@(6,0),@(0,6),@(4,4),@(-6,0),@(0,-6),@(-4,-4))
    if($Device -eq 'mouse'){$drifts=@(@(1,0),@(0,1),@(1,1),@(-1,0),@(0,-1),@(-1,-1))}
    $scale=[CapyRowPointer]::GetDpiForWindow($review.MainWindowHandle)/96.
    if($scale -le 0 -or [double]::IsNaN($scale) -or [double]::IsInfinity($scale)){throw 'Cannot read the owned tab window DPI'}
    for($i=0;$i -lt $drifts.Count;$i++){
        $panel=$Panels[$i%$Panels.Count];$id=$Prefix+$panel;$script:case="$id-drift-$i"
        $selected={if($Drawer){@((Model).state.customization.column_drawers|Where-Object {$_.tabs.active -eq $panel}).Count -eq 1}else{(Panel-Group $panel).active -eq $panel}}
        if(& $selected){throw "$case did not target an inactive tab"}
        $at=Point $id;$bounds=(Control $id).Current.BoundingRectangle
        $x=[int]($at.x+$drifts[$i][0]*$scale);$y=[int]($at.y+$drifts[$i][1]*$scale)
        if($x -le $bounds.Left -or $x -ge $bounds.Right -or $y -le $bounds.Top -or $y -ge $bounds.Bottom){throw "$case left its tab bounds"}
        if($Device -eq 'pen'){[CapyRowPointer]::PenHover($at.x,$at.y);Start-Sleep -Milliseconds 35}
        [CapyRowPointer]::Down($Device,$at.x,$at.y)
        Start-Sleep -Milliseconds 35
        [CapyRowPointer]::Move($x,$y)
        Start-Sleep -Milliseconds 35
        [CapyRowPointer]::Up($Device -eq 'pen')
        Wait-Until {(& $selected) -and (Gesture).phase -eq 'idle' -and !(Gesture).ignore_click} "$case did not select on its first tap"
        Start-Sleep -Milliseconds 150
        if(!(& $selected) -or (Tab-Placement) -ne $before -or ((Model).state.customization.expanded|ConvertTo-Json -Depth 30 -Compress) -ne $expanded){throw "$case changed the settled selection, order, layout or configuration"}
    }
}
function Cancel-Contact {
    if($Device -eq 'mouse'){[CapyRowPointer]::Key(0x1b);[CapyRowPointer]::Up()}
    else{[CapyRowPointer]::Cancel()}
}
function Tear-Off([string]$Panel,[switch]$Gradual,[switch]$Cancel) {
    $script:case="$Panel-$(if($Gradual){'gradual'}else{'direct'})-$(if($Cancel){'cancel'}else{'commit'})"
    $before=Layout
    $at=Point "panel-tab-$Panel";$to=Point 'drawing-canvas'
    [CapyRowPointer]::Down($Device,$at.x,$at.y)
    Start-Sleep -Milliseconds 35
    if($Gradual){
        for($i=1;$i -le 12;$i++){
            [CapyRowPointer]::Move([int]($at.x+($to.x-$at.x)*$i/12),[int]($at.y+($to.y-$at.y)*$i/12))
            Start-Sleep -Milliseconds 18
        }
    }else{[CapyRowPointer]::Move($to.x,$to.y)}
    Wait-Until {(Gesture).phase -eq 'dragging' -and (Gesture).captured} "$case did not retain the contact over canvas"
    $gesture=Gesture
    if($gesture.device -ne $Device -or $gesture.requires_hold -or $gesture.menu_open){throw "$case used incorrect pickup arbitration"}
    # Continue the same contact after its original tab has been reparented.
    Start-Sleep -Milliseconds 200
    [CapyRowPointer]::Move($to.x+30,$to.y+15)
    Wait-Until {(Gesture).phase -eq 'dragging' -and (Gesture).captured} "$case lost capture after reparenting"
    if($Cancel){
        Cancel-Contact
        Wait-Until {(Gesture).phase -eq 'idle' -and (Layout) -eq $before} "$case did not restore the workspace"
    }else{
        [CapyRowPointer]::Up()
        Wait-Until {(Gesture).phase -eq 'idle' -and (Panel-Group $Panel).floating} "$case did not finish the tear-off"
        $after=Layout
        Workspace-History 'undo_workspace';Wait-Until {(Layout) -eq $before} "$case did not undo in one step"
        Workspace-History 'redo_workspace';Wait-Until {(Layout) -eq $after} "$case did not redo in one step"
        Workspace-History 'undo_workspace';Wait-Until {(Layout) -eq $before} "$case did not restore the fixture"
    }
    if((Gesture).menu_open -or (Model).state.document_file.modified){throw "$case changed the drawing or retained a menu"}
    Write-Output "$case passed"
}
try {
    Enter-CapyEnvironment
    $env:CAPY_STORAGE_DIR=Join-Path $run 'profile';$env:CAPY_TRACE_UI='1'
    $stderr=Join-Path $run 'stderr.log'
    $review=Start-Process -FilePath $Executable -WorkingDirectory $directory -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $run 'stdout.log') -RedirectStandardError $stderr
    $null=$review.Handle
    @{process_id=$review.Id;run=$run;device=$Device}|ConvertTo-Json|Set-Content (Join-Path $repo 'artifacts/windows/tab-pickup-review.json')
    Write-Output "Owned tab pickup review $($review.Id) ($Device)"
    Wait-Until {$review.Refresh();$review.MainWindowHandle -ne [IntPtr]::Zero -and (Model).brush_ready -and (Model).windows_workspace.ready -and !(Model).windows_workspace.busy} 'Tab review did not start' 45
    $root=[System.Windows.Automation.AutomationElement]::FromHandle($review.MainWindowHandle)
    $null=[CapyRowPointer]::SetThreadDpiAwarenessContext([IntPtr](-4))
    $null=[CapyRowPointer]::SetForegroundWindow($review.MainWindowHandle)
    [CapyRowPointer]::Initialize([uint32]$review.Id)
    $script:case='short-tab-selection'
    Tap 'panel-tab-sizes'
    Wait-Until {(Panel-Group 'sizes').active -eq 'sizes'} 'Short tab click did not select Sizes'
    Tap 'panel-tab-tool_settings'
    Wait-Until {(Panel-Group 'tool_settings').active -eq 'tool_settings'} 'Short tab click did not restore Properties'
    Drifting-TabTaps 'panel-tab-' @('sizes','tool_settings')
    $before=Tab-Placement
    $at=Point 'panel-tab-properties';[CapyRowPointer]::RightClick($at.x,$at.y);Invoke 'Collapse column' -Name
    $restoreSteps=1
    Wait-Until {$null -ne (Find 'column-icon-properties')} 'Properties column did not collapse'
    $at=Point 'column-icon-properties';[CapyRowPointer]::RightClick($at.x,$at.y)
    $toggle=(Control 'Open individual panels' -Name).GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern)
    if($toggle.Current.ToggleState -eq [System.Windows.Automation.ToggleState]::Off){$toggle.Toggle();$restoreSteps++}
    [CapyRowPointer]::Key(0x1b)
    Tap 'column-icon-properties'
    Wait-Until {$null -ne (Find 'drawer-tab-adjustments')} 'Properties drawer did not expose its tabs'
    Invoke 'drawer-tab-properties'
    Wait-Until {@((Model).state.customization.column_drawers|Where-Object {$_.tabs.active -eq 'properties'}).Count -eq 1} 'Drawer did not select its initial Properties tab'
    Drifting-TabTaps 'drawer-tab-' @('adjustments','properties') -Drawer
    for($i=0;$i -lt $restoreSteps;$i++){
        $previous=Tab-Placement;Workspace-History 'undo_workspace'
        Wait-Until {(Tab-Placement) -ne $previous} 'Drawer setup Undo did not finish'
    }
    Wait-Until {(Tab-Placement) -eq $before -and $null -ne (Find 'panel-tab-properties')} 'Drawer tap fixture did not restore the docked column'
    Tear-Off 'layers'
    Tear-Off 'layers' -Cancel
    Tear-Off 'tool_settings' -Gradual
    Tear-Off 'tool_settings' -Gradual -Cancel
    & (Join-Path $PSScriptRoot 'exercise-window.ps1') -ProcessId $review.Id -Action Close -DiscardUnsaved
    if((Get-Item -LiteralPath $stderr).Length){throw 'Native stderr requires inspection'}
    [pscustomobject]@{device=$Device;short_click='passed';drifting_panel_tabs='passed';drifting_drawer_tabs='passed';direct_tearoff='passed';gradual_tearoff='passed';retained_contact='passed';capture_cancel='passed';workspace_undo_redo='passed';zero_exit='passed';scope='OS-injected input; physical digitizers and latency remain separate'}|ConvertTo-Json
}catch{
    $failure=$_
    @{case=$script:case;injection_gap_ms=[CapyRowPointer]::MaxGapMilliseconds;error=$failure.ToString();gesture=(Gesture);presentation=(Presentation)}|ConvertTo-Json -Depth 60|Set-Content (Join-Path $run 'failure.json')
    if($review -and !$review.HasExited){
        & (Join-Path $PSScriptRoot 'inspect-window.ps1') -ProcessId $review.Id -Output (Join-Path $run 'failure.png') -ClientOnly *> (Join-Path $run 'failure-capture.json')
    }
    throw $failure
}finally{
    [CapyRowPointer]::Dispose()
    Exit-CapyEnvironment
}
