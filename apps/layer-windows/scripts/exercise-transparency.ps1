param([Parameter(Mandatory)][string]$Executable)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'CapyUia.ps1')
$CapyFind='visible'
Add-Type -Path (Join-Path $PSScriptRoot 'RowPointerDriver.cs')
Add-Type -Path (Join-Path $PSScriptRoot 'CanvasTouchDriver.cs')
$null=[CapyCanvasTouch]::SetThreadDpiAwarenessContext([IntPtr](-4))
$repo=(Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$Executable=(Resolve-Path -LiteralPath $Executable).Path
$directory=Split-Path -Parent $Executable
$run=Join-Path $repo ('artifacts/windows/transparency/'+[Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($run)|Out-Null
$ControlType=[System.Windows.Automation.ControlType]
Add-Type -AssemblyName System.Drawing
Add-Type -Name GlassWindow -Namespace Capy -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);[DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr window,int[] point);'
function Center($element){$b=$element.Current.BoundingRectangle;@{x=[int]($b.X+$b.Width/2);y=[int]($b.Y+$b.Height/2)}}
function Tap($at,[string]$Device='mouse'){[CapyRowPointer]::Down($Device,$at.x,$at.y);Start-Sleep -Milliseconds 30;[CapyRowPointer]::Up()}
function Switch-Workspace([string]$Name,[string]$Id){
    $found=@{switch=$null;menu=$null}
    Wait-Until {
        $found.switch=Find ('workspace-switch-'+$Id);$found.menu=Find 'header-workspace-menu'
        $found.switch -or $found.menu
    } "No workspace switcher for $Name" 15
    $switch=$found.switch
    if(!$switch){Invoke 'header-workspace-menu';$switch=Control $Name -Name -Type $ControlType::MenuItem}
    Wait-Until {try{$switch.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle();$true}catch{$false}} "$Name switch stayed unavailable" 20
    Wait-Until {(Model).windows_workspace.id -eq $Id -and !(Model).windows_workspace.busy} "$Name did not open" 20
}
function Preference([int]$Index,[string]$Theme){
    Invoke 'settings-button'
    $dialog=Control 'Preferences' -Name -Type $ControlType::Window
    if($Theme){
        $null=[CapyRowPointer]::SetForegroundWindow($drawingWindow)
        $choice=Control 'preference-choice-theme' -Within $dialog;$choice.SetFocus()
        Wait-Until {$choice.Current.HasKeyboardFocus} 'Color theme did not take focus'
        [CapyRowPointer]::Key([uint32]$review.Id,[uint16]0x73)
        Wait-Until {Find 'Light' -Name -Type $ControlType::ListItem} 'Theme choices did not open'
        foreach($key in @(0x24)+@(@(0x28)*@('system','light','dark').IndexOf($Theme))+@(0x0D)){[CapyRowPointer]::Key([uint32]$review.Id,[uint16]$key)}
        Wait-Until {(Model).state.settings.theme -eq $Theme} "The $Theme theme did not apply"
    }
    $circle=Control ('preference-transparency-'+$Index)
    if($circle.Current.Name -ne @('Off','Low','Medium','High')[$Index]){throw "Transparency circle $Index is misnamed"}
    Invoke ('preference-transparency-'+$Index)
    Wait-Until {(Model).state.palette.glass.transparency -eq @('off','low','medium','high')[$Index]} "Transparency $Index did not apply"
    Wait-Until {$circle.Current.ItemStatus -eq 'Selected'} "Transparency $Index circle was not checked"
    $close=$dialog.FindFirst([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.AndCondition]::new(
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty,'CloseButton'),
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,$ControlType::Button)))
    $close.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Wait-Until {$null -eq (Find 'Preferences' -Name -Type $ControlType::Window)} 'Preferences did not close'
}
function Sample([string]$Name,$Points){
    Start-Sleep -Milliseconds 900;Capture $Name -Composed
    $origin=[int[]]@(0,0);$null=[Capy.GlassWindow]::ClientToScreen($drawingWindow,$origin)
    $image=[System.Drawing.Bitmap]::new((Join-Path $run ($Name+'.png')))
    try{@($Points|ForEach-Object {$image.GetPixel($_.x-$origin[0],$_.y-$origin[1]).G})}finally{$image.Dispose()}
}
function Tab-Shades([string]$Name){
    $origin=[int[]]@(0,0);$null=[Capy.GlassWindow]::ClientToScreen($drawingWindow,$origin)
    $image=[System.Drawing.Bitmap]::new((Join-Path $run ($Name+'.png')))
    try{
        foreach($group in @((Model).layout.groups|Where-Object {$_.tabs_visible})){
            $tab=(Control ('panel-tab-'+$group.active) -Arranged).Current.BoundingRectangle
            $x=[int]($tab.X+4*$scale-$origin[0])
            $shade,$body=@(($tab.Bottom-3*$scale),($tab.Bottom+3*$scale))|ForEach-Object {$image.GetPixel($x,[int]($_-$origin[1]))}
            $difference=[Math]::Max([Math]::Max([Math]::Abs($shade.R-$body.R),[Math]::Abs($shade.G-$body.G)),[Math]::Abs($shade.B-$body.B))
            $line="$Name $($group.active): tab $($shade.R),$($shade.G),$($shade.B) panel $($body.R),$($body.G),$($body.B)"
            $tabShades.Add($line);if($difference -gt 2){$tabMismatches.Add($line)}
        }
    }finally{$image.Dispose()}
}
function Tab-Feedback([string]$Name){
    $group=@((Model).layout.groups|Where-Object {$_.tabs_visible -and $_.panels.Count -ge 2})[0]
    if(!$group){throw 'No adjacent panel tabs to check'}
    $restore=$group.active
    foreach($index in 0,1){
        $active=$group.panels[$index];$next=$group.panels[1-$index]
        if(@((Model).layout.groups|Where-Object {$_.id -eq $group.id})[0].active -ne $active){Invoke ('panel-tab-'+$active)}
        Wait-Until {@((Model).layout.groups|Where-Object {$_.id -eq $group.id})[0].active -eq $active} 'Tab feedback setup did not select its tab'
        $park=Center (Control 'drawing-canvas');[CapyRowPointer]::Hover($park.x,$park.y)
        $tab=Control ('panel-tab-'+$next) -Arranged;$bounds=$tab.Current.BoundingRectangle
        $origin=[int[]]@(0,0);$null=[Capy.GlassWindow]::ClientToScreen($drawingWindow,$origin)
        $retained=$tab.GetRuntimeId() -join ':'
        $selectedBounds=(Control ('panel-tab-'+$active) -Arranged).Current.BoundingRectangle
        $foot=@(foreach($x in @(($selectedBounds.Left+3*$scale),($selectedBounds.Right-3*$scale))){
            foreach($y in @(($selectedBounds.Bottom-2*$scale),($selectedBounds.Bottom+2*$scale))){@{x=[int]($x-$origin[0]);y=[int]($y-$origin[1])}}
        })
        $interior=@(foreach($fraction in .2,.5,.8){@{x=[int]($bounds.Left+$bounds.Width*$fraction-$origin[0]);y=[int]($bounds.Top+4*$scale-$origin[1])}})
        $expectedFoot=$null;$expectedInterior=$null
        foreach($state in 'rest','hover','press','release'){
            if($state -eq 'hover'){$at=Center $tab;[CapyRowPointer]::Hover($at.x,$at.y)}
            if($state -eq 'press'){
                [CapyRowPointer]::Down('mouse',$at.x,$at.y)
                Wait-Until {@((Model).layout.groups|Where-Object {$_.id -eq $group.id})[0].active -eq $next} 'Primary down did not select the inactive tab before release'
            }
            if($state -eq 'release'){[CapyRowPointer]::Up()}
            Start-Sleep -Milliseconds 200
            $capture="$Name-feedback-$index-$state";Capture $capture -Composed
            $image=[System.Drawing.Bitmap]::new((Join-Path $run ($capture+'.png')))
            try{
                $pixels=(@($foot|ForEach-Object {$image.GetPixel($_.x,$_.y).ToArgb()}) -join ',')
                $fill=(@($interior|ForEach-Object {$image.GetPixel($_.x,$_.y).ToArgb()}) -join ',')
            }finally{$image.Dispose()}
            if($state -eq 'rest'){$expectedFoot=$pixels;$expectedInterior=$fill}
            if($state -eq 'hover'){
                if($pixels -ne $expectedFoot){throw "$capture changed the active tab's foot"}
                if($fill -eq $expectedInterior){throw "$capture has no visible inactive-tab hover"}
            }
            if($state -in 'press','release'){
                if(((Control ('panel-tab-'+$next) -Arranged).GetRuntimeId() -join ':') -ne $retained){throw "$capture replaced the native tab button"}
                if(@((Model).layout.groups|Where-Object {$_.id -eq $group.id})[0].active -ne $next){throw "$capture lost the selected tab"}
                if($null -ne (Model).state.customization.expanded){throw "$capture opened panel configuration"}
                Tab-Shades $capture
            }
        }
    }
    if(@((Model).layout.groups|Where-Object {$_.id -eq $group.id})[0].active -ne $restore){Invoke ('panel-tab-'+$restore)}
    Wait-Until {@((Model).layout.groups|Where-Object {$_.id -eq $group.id})[0].active -eq $restore} 'Tab feedback did not restore selection'
}
function Zoom([double]$Minimum){
    (Control 'drawing-canvas').SetFocus()
    Wait-Until {(Control 'drawing-canvas').Current.HasKeyboardFocus} 'Transparency canvas did not own keyboard focus'
    for($i=0;$i -lt 32 -and (Model).state.camera.zoom -le $Minimum;$i++){
        $before=(Model).state.camera.zoom
        [CapyRowPointer]::Chord([uint32]$review.Id,[uint16[]]@(0x11),0xBB)
        Wait-Until {(Model).state.camera.zoom -gt $before} 'Transparency zoom input did not change the camera'
    }
}
try {
    Enter-CapyEnvironment
    $env:CAPY_STORAGE_DIR=Join-Path $run 'profile';$env:CAPY_TRACE_UI='1';$env:CAPY_SMOKE_TEST='1'
    $stderr=Join-Path $run 'stderr.log'
    $review=Start-Process -FilePath $Executable -WorkingDirectory $directory -WindowStyle Hidden -PassThru -RedirectStandardError $stderr
    $null=$review.Handle
    Write-Output "Transparency review $($review.Id): $run"
    Wait-Until {$review.Refresh();$native=Owned-DrawingWindow $review;if(!$native){return $false};$script:drawingWindow=$native.Handle;$script:root=$native.Root;(Model).brush_ready -and (Model).windows_workspace.ready -and !(Model).windows_workspace.busy} 'Transparency review did not start' 90
    $root.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).SetWindowVisualState([System.Windows.Automation.WindowVisualState]::Maximized)
    Start-Sleep -Milliseconds 600
    $null=[CapyRowPointer]::SetForegroundWindow($drawingWindow)
    [CapyRowPointer]::Initialize([uint32]$review.Id)
    Switch-Workspace 'Paint' 'builtin:workspace:illustrator'
    if((Model).state.palette.glass.transparency -ne 'low'){throw 'Low is not the default transparency'}
    Invoke 'panel-tab-stats'
    Wait-Until {@((Model).layout.groups|Where-Object active -eq 'stats').Count -eq 1} 'Diagnostics did not become the active tab'
    Wait-Until {(Model).windows_glass.regions -ge 4} 'Paint did not publish its glass surfaces'
    $group=@((Model).layout.groups|Where-Object {$_.panels -contains 'layers'})[0]
    $scale=[Capy.GlassWindow]::GetDpiForWindow($drawingWindow)/96.;$client=[int[]]@(0,0);$null=[Capy.GlassWindow]::ClientToScreen($drawingWindow,$client)
    $bounds=$group.bounds
    $point=@{x=[int]($client[0]+($bounds.x+$bounds.width-24)*$scale);y=[int]($client[1]+($bounds.y+$bounds.height-72)*$scale)}
    $panel=[Convert]::ToInt32((Model).state.palette.panel.Substring(3,2),16)
    $tabShades=[Collections.Generic.List[string]]::new();$tabMismatches=[Collections.Generic.List[string]]::new()
    Fit-Canvas
    $surround=@{};$overPaper=@{}
    foreach($level in 0,3,2,1){
        Preference $level
        $surround[$level]=(Sample ('fit-'+$level) @($point))[0];Tab-Shades ('fit-'+$level);Tab-Feedback ('fit-'+$level)
        if([Math]::Abs($surround[$level]-$panel) -gt 1){throw "Level $level panel over the surround is $($surround[$level]), expected $panel"}
    }
    Zoom 2
    Wait-Until {$z=(Model).state.camera.zoom;$z -gt 2} 'The canvas did not zoom under the panels'
    foreach($level in 0,1,2,3){
        Preference $level
        $overPaper[$level]=(Sample ('paper-'+$level) @($point))[0];Tab-Shades ('paper-'+$level)
    }
    Write-Output ("Layers panel green over the surround: "+(($surround.GetEnumerator()|Sort-Object Key|ForEach-Object {$_.Value}) -join ',')+"; over paper: "+(0..3|ForEach-Object {$overPaper[$_]}) -join ',')
    if($overPaper[0] -ne $panel){throw "Off panel over paper is $($overPaper[0]), expected $panel"}
    if(!($overPaper[1] -gt $overPaper[0] -and $overPaper[2] -gt $overPaper[1] -and $overPaper[3] -gt $overPaper[2])){throw 'Higher transparency did not show more paper'}
    Preference 0 'light'
    Fit-Canvas
    foreach($level in 0,1,2,3){
        if($level){Preference $level}
        $null=Sample ('light-fit-'+$level) @();Tab-Shades ('light-fit-'+$level);Tab-Feedback ('light-fit-'+$level)
    }
    Zoom 2
    Wait-Until {$z=(Model).state.camera.zoom;$z -gt 2} 'The light canvas did not zoom under the panels'
    foreach($level in 0,1,2,3){
        Preference $level
        $null=Sample ('light-paper-'+$level) @();Tab-Shades ('light-paper-'+$level)
    }
    $tabShades|Write-Output
    if(!$tabShades.Count){throw 'Paint has no tabbed group to compare'}
    if($tabMismatches.Count){throw "Active tabs differ from their panels:`n"+($tabMismatches -join "`n")}
    Preference 3 'dark'
    Switch-Workspace 'Sketch' 'builtin:workspace:painter'
    $color=$null;foreach($zone in @((Model).header.model.zones)){foreach($entry in @($zone)){if($entry.item.control.kind -eq 'color'){$color=$entry.id}}}
    $settled=@{count=-1;since=[Diagnostics.Stopwatch]::StartNew()}
    Wait-Until {$count=(Model).windows_glass.regions;if($count -ne $settled.count){$settled.count=$count;$settled.since.Restart()};$settled.since.ElapsedMilliseconds -gt 1000} 'Sketch glass did not settle' 15
    $closed=$settled.count
    Tap (Center (Control ('header-item-'+$color)))
    Wait-Until {(Find 'tool-drawer') -and (Model).windows_glass.regions -ge $closed+2} 'The Sketch drawer and its connector did not publish glass'
    Capture 'sketch-drawer'
    [CapyRowPointer]::Verify();[CapyRowPointer]::Dispose()
    & (Join-Path $PSScriptRoot 'exercise-window.ps1') -ProcessId $review.Id -Action Close -DiscardUnsaved
    if(!$review.WaitForExit(8000)){throw 'Transparency review did not close'}
    if((Get-Item -LiteralPath $stderr).Length){throw 'Transparency review wrote to stderr'}
    [pscustomobject]@{default_low='passed';circles='passed';invisible_over_surround='passed';paper_by_level='passed';active_tab_matches_panel='passed';tab_feedback_preserves_join='passed';drawer_connector='passed';
        scope='Synthetic UIA and captured pixels; physical display and performance acceptance are separate'}|ConvertTo-Json|Tee-Object -FilePath (Join-Path $run 'results.json')
}catch{
    try{Capture 'failure'}catch{}
    [IO.File]::WriteAllText((Join-Path $run 'failure.txt'),($_|Out-String)+$_.ScriptStackTrace);throw
}finally{
    [CapyCanvasTouch]::Dispose();[CapyRowPointer]::Dispose()
    if($review -and !$review.HasExited){Stop-Process -Id $review.Id -Force -ErrorAction SilentlyContinue}
    Exit-CapyEnvironment
}
