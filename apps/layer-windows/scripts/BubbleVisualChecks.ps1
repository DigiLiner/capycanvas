Add-Type -AssemblyName System.Drawing

function Read-BubbleVisualPatch($Point,[string]$Path){
    $bitmap=[Drawing.Bitmap]::new(9,9);$graphics=[Drawing.Graphics]::FromImage($bitmap)
    try{
        $graphics.CopyFromScreen($Point.x-4,$Point.y-4,0,0,$bitmap.Size)
        $pixels=@(foreach($y in 0..8){foreach($x in 0..8){$color=$bitmap.GetPixel($x,$y);@{r=[int]$color.R;g=[int]$color.G;b=[int]$color.B}}})
        $bitmap.Save($Path)
        return ,$pixels
    }finally{$graphics.Dispose();$bitmap.Dispose()}
}
function Measure-BubbleVisualOverlay($Baseline,$Actual,$Ink,[double]$Alpha){
    $errors=@();$changes=@()
    for($i=0;$i -lt $Baseline.Count;$i++){
        foreach($channel in @('r','g','b')){
            $expected=(1-$Alpha)*$Baseline[$i][$channel]+$Alpha*$Ink[$channel]
            $errors+=[Math]::Abs($Actual[$i][$channel]-$expected)
            $changes+=[Math]::Abs($Actual[$i][$channel]-$Baseline[$i][$channel])
        }
    }
    @{maximum_error=($errors|Measure-Object -Maximum).Maximum;mean_change=($changes|Measure-Object -Average).Average;alpha=$Alpha}
}
function Bubble-VisualBounds($Element){
    $bounds=$Element.Current.BoundingRectangle
    return ,@([double]$bounds.X,[double]$bounds.Y,[double]$bounds.Width,[double]$bounds.Height)
}
function Bubble-VisualState([long]$Id){
    $model=Model
    [ordered]@{
        view=(View $Id);layout=$model.state.workspace.layout;revision=$model.state.document_file.revision
        brush=$model.state.brush.preset;colors=$model.state.colors
        checkpoint=@($model.windows_tabs.session_stamps|Where-Object id -eq $model.windows_tabs.selected)[0].stamp.checkpoint
    }|ConvertTo-Json -Depth 90 -Compress
}
function Bubble-VisualTags([long]$Id,[string]$Label){
    $center=Center $Id;$reach=((Packet $Id).reach+16)*$scale
    foreach($element in $root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)){
        $current=$element.Current;$bounds=$current.BoundingRectangle
        if($current.ControlType -ne [System.Windows.Automation.ControlType]::Text -or $current.Name -ne $Label -or $current.IsOffscreen -or $bounds.IsEmpty -or $bounds.Width -le 0 -or $bounds.Height -le 0 -or $bounds.Width -gt 78*$scale+2){continue}
        if([Math]::Abs($bounds.X+$bounds.Width*.5-$center.x) -le $reach -and [Math]::Abs($bounds.Y+$bounds.Height*.5-$center.y) -le $reach){
            @{name=$current.Name;bounds=@($bounds.X,$bounds.Y,$bounds.Width,$bounds.Height)}
        }
    }
}
function Assert-BubbleVisualChooserTags([long]$Id){
    $view=View $Id;$items=@($view.chooser)
    $hidden=@($items|Where-Object {$_.kind -ne 'brush' -and $_.enabled -and !$_.shown})[0]
    $original=@($items|Where-Object {$_.kind -eq 'medium' -and $_.shown})[0]
    if(!$hidden -or ($hidden.kind -eq 'medium' -and !$original)){throw 'Chooser tag check needs a hidden enabled choice and its original shown medium'}
    $before=Bubble-VisualState $Id;$away=@{x=[int]($w.X+20);y=[int]($w.Y+20)};$point=Relative $Id $hidden.center
    $evidence=[ordered]@{id=$Id;label=$hidden.label;original_medium=$original.label;point=$point;baseline=@();hover=@();pressed=@();restored=@()};$down=$false
    try{
        [CapyRowPointer]::Hover($away.x,$away.y);Start-Sleep -Milliseconds 250
        $evidence.baseline=@(Bubble-VisualTags $Id $hidden.label)
        if($evidence.baseline.Count){throw 'Unshown chooser tag is visible before hover'}
        [CapyRowPointer]::Hover($point.x,$point.y);Start-Sleep -Milliseconds 250
        $evidence.hover=@(Bubble-VisualTags $Id $hidden.label)
        if($evidence.hover.Count){throw 'Passive mouse hover revealed an unshown chooser tag'}
        [CapyRowPointer]::Down('mouse',$point.x,$point.y);$down=$true
        Wait-Until {$evidence.pressed=@(Bubble-VisualTags $Id $hidden.label);$evidence.pressed.Count -gt 0} 'Held chooser choice did not expose its native text tag'
    }finally{
        try{
            if($down){
                try{
                    if($hidden.kind -eq 'medium'){
                        $restore=@((View $Id).chooser|Where-Object {$_.kind -eq $original.kind -and $_.label -eq $original.label})[0]
                        if(!$restore){throw 'Original chooser medium disappeared during tag check'}
                        $at=Relative $Id $restore.center;[CapyRowPointer]::Move($at.x,$at.y)
                        Wait-Until {@((View $Id).chooser|Where-Object {$_.kind -eq $original.kind -and $_.label -eq $original.label -and $_.shown}).Count -eq 1} 'Held chooser contact did not restore its original medium'
                    }
                }finally{
                    try{Escape}finally{[CapyRowPointer]::Cancel()}
                }
            }
        }finally{
            [CapyRowPointer]::Hover($away.x,$away.y)
            $evidence|ConvertTo-Json -Depth 10|Set-Content (Join-Path $run "bubble-$Id-chooser-tags.json")
        }
    }
    Wait-BubbleVisualLayout
    Wait-Until {!@(Bubble-VisualTags $Id $hidden.label).Count} 'Cancelled chooser retained its unshown tag'
    $evidence.restored=@(Bubble-VisualTags $Id $hidden.label)
    $after=Bubble-VisualState $Id
    $evidence|ConvertTo-Json -Depth 10|Set-Content (Join-Path $run "bubble-$Id-chooser-tags.json")
    if($after -ne $before){@{before=($before|ConvertFrom-Json);after=($after|ConvertFrom-Json)}|ConvertTo-Json -Depth 90|Set-Content (Join-Path $run "bubble-$Id-chooser-tag-state-change.json");throw 'Cancelled chooser tag check changed the bubble, workspace or artwork'}
    $checks.Add('chooser-tags-hidden-on-hover-visible-during-choice-and-restored')
}
function Assert-BubbleVisualStates([long]$Id){
    $view=View $Id
    if(!$view.selected -or !$view.enabled -or $view.kind -notin @('dial','brush','medium','controls')){throw 'Visual state check requires an enabled selected tool face'}
    $point=Relative $Id @(-18,-7);$away=@{x=[int]($w.X+20);y=[int]($w.Y+20)}
    $face=Control "tool-bubble-$Id" -Arranged;$prefix=Join-Path $run "bubble-$Id-visual"
    $color=[Drawing.ColorTranslator]::FromHtml((Model).state.palette.text)
    $ink=@{r=[int]$color.R;g=[int]$color.G;b=[int]$color.B}
    $before=Bubble-VisualState $Id;$evidence=[ordered]@{id=$Id;point=$point;ink=$ink;scale=$scale}
    $down=$false;$visualFailure=$null
    try{
        [CapyRowPointer]::Hover($away.x,$away.y)
        $null=Wait-StablePixels {Patch-Pixels $point}
        $baseline=Read-BubbleVisualPatch $point ($prefix+'-idle-patch.png')
        $idleBounds=Bubble-VisualBounds $face
        $null=Screen-Pixels $point.x $point.y ($prefix+'-idle.png')
        [CapyRowPointer]::Hover($point.x,$point.y)
        $null=Wait-StablePixels {Patch-Pixels $point}
        $hover=Read-BubbleVisualPatch $point ($prefix+'-hover-patch.png')
        $evidence.hover=Measure-BubbleVisualOverlay $baseline $hover $ink .07
        $null=Screen-Pixels $point.x $point.y ($prefix+'-hover.png')
        [CapyRowPointer]::Down('mouse',$point.x,$point.y);$down=$true
        Start-Sleep -Milliseconds 100
        $pressed=Read-BubbleVisualPatch $point ($prefix+'-pressed-patch.png')
        $pressedBounds=Bubble-VisualBounds $face
        $evidence.pressed=Measure-BubbleVisualOverlay $baseline $pressed $ink .14
        $evidence.idle_bounds=$idleBounds;$evidence.pressed_bounds=$pressedBounds
        $null=Screen-Pixels $point.x $point.y ($prefix+'-pressed.png')
        $origin=Center $Id
        foreach($cycle in 1..3){
            [CapyRowPointer]::Move(($point.x+[int](20*$scale)),$point.y)
            Wait-Until {[Math]::Abs((Center $Id).x-$origin.x) -gt 8*$scale} 'Held bubble did not move beyond its press slop'
            [CapyRowPointer]::Move($point.x,$point.y)
            Wait-Until {$at=Center $Id;[Math]::Abs($at.x-$origin.x) -lt 2 -and [Math]::Abs($at.y-$origin.y) -lt 2} 'Held bubble did not return to its starting position'
            $null=Wait-StablePixels {Patch-Pixels $point}
            $returned=Read-BubbleVisualPatch $point ($prefix+"-returned-$cycle-patch.png")
            $evidence["returned-$cycle"]=Measure-BubbleVisualOverlay $baseline $returned $ink .07
        }
        $null=Screen-Pixels $point.x $point.y ($prefix+'-returned.png')
        Capture 'face-drag-return-held' -Composed -WithModel
        $heldBounds=Bubble-VisualBounds $face;$evidence.held_bounds=$heldBounds
        for($i=2;$i -lt 4;$i++){if([Math]::Abs($heldBounds[$i]-$idleBounds[$i]*1.04) -gt 1){throw 'Held bubble did not retain its native 104 percent drag size'}}
    }catch{
        $visualFailure=$_;$evidence.failure=$_.ToString();throw
    }finally{
        try{
            if($down){
                try{
                    [CapyRowPointer]::Key([uint32]$review.Id,0x1b)
                    Wait-Until {
                        $bounds=Bubble-VisualBounds $face;$evidence.cancelled_bounds=$bounds
                        foreach($i in 0..3){if([Math]::Abs($bounds[$i]-$idleBounds[$i]) -gt .75){return $false}}
                        $true
                    } 'Escape did not restore idle native bounds before mouse release'
                    $cancelled=Read-BubbleVisualPatch $point ($prefix+'-cancelled-patch.png')
                    $evidence.after_escape_idle=Measure-BubbleVisualOverlay $baseline $cancelled $ink 0
                    $evidence.after_escape_hover=Measure-BubbleVisualOverlay $baseline $cancelled $ink .07
                }finally{[CapyRowPointer]::Cancel()}
            }
        }catch{
            $evidence.cleanup_failure=$_.ToString();if(!$visualFailure){$visualFailure=$_;throw}
        }finally{
            try{[CapyRowPointer]::Hover($away.x,$away.y)}catch{$evidence.hover_cleanup_failure=$_.ToString();if(!$visualFailure){throw}}finally{$evidence|ConvertTo-Json -Depth 10|Set-Content ($prefix+'.json')}
        }
    }
    $null=Wait-StablePixels {Patch-Pixels $point}
    $after=Bubble-VisualState $Id
    if($after -ne $before){@{before=($before|ConvertFrom-Json);after=($after|ConvertFrom-Json)}|ConvertTo-Json -Depth 90|Set-Content ($prefix+'-state-change.json');throw 'Cancelled visual state checks changed the bubble, workspace or artwork'}
    foreach($phase in @('hover','pressed','returned-1','returned-2','returned-3')){
        $measurement=$evidence[$phase]
        if($measurement.maximum_error -gt 4 -or $measurement.mean_change -lt 2){throw "Selected bubble $phase pixels do not match its text-color overlay: $($measurement|ConvertTo-Json -Compress)"}
    }
    for($i=0;$i -lt 4;$i++){if([Math]::Abs($idleBounds[$i]-$pressedBounds[$i]) -gt .75){throw 'Pressing the ordinary bubble face changed its native bounds'}}
    $checks.Add('selected-face-hover-pressed-pixels-and-fixed-size')
    $checks.Add('three-held-drag-returns-retain-hover-fill')
}
function Wait-BubbleVisualLayout{
    $settled=@{last='';same=0}
    Wait-Until {
        $bounds=@(foreach($packet in Packets){$center=Center $packet.view.id;@($packet.view.id,$center.x,$center.y)})|ConvertTo-Json -Compress
        if($bounds -eq $settled.last){$settled.same++}else{$settled.last=$bounds;$settled.same=0}
        $settled.same -ge 5
    } 'Bubble edge scene did not settle'
}
function Bubble-VisualBead([long]$Id,$Bead){
    $point=Relative $Id $Bead.placement.center;$size=$Bead.placement.size*$scale
    $matches=@(foreach($element in $root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)){
        $current=$element.Current;$bounds=$current.BoundingRectangle
        if($current.ControlType -ne [System.Windows.Automation.ControlType]::Button -or $current.Name -ne $Bead.label -or $current.IsOffscreen -or $bounds.IsEmpty){continue}
        if([Math]::Abs($bounds.Width-$size) -le 2 -and [Math]::Abs($bounds.Height-$size) -le 2 -and [Math]::Abs($bounds.X+$bounds.Width*.5-$point.x) -le 2 -and [Math]::Abs($bounds.Y+$bounds.Height*.5-$point.y) -le 2){$element}
    })
    if($matches.Count -ne 1){throw "Expected one native bead matching its shared bounds: $($Bead.label)"}
    $matches[0]
}
function Assert-BubbleVisualOrbitReturn($Samples,$Turned){
    $difference={param($a,$b) $maximum=0.;foreach($i in 0..3){$maximum=[Math]::Max($maximum,[Math]::Abs($a[$i]-$b[$i]))};$maximum}
    $changed=-1;$gap=0.;$previous=0.
    for($i=0;$i -lt $Samples.Count;$i++){
        $gap=[Math]::Max($gap,$Samples[$i].ms-$previous);$previous=$Samples[$i].ms
        if((& $difference $Samples[$i].bounds $Turned) -gt 2){$changed=$i;break}
    }
    if($changed -lt 0){throw 'Native orbit return did not acknowledge movement'}
    if($Samples[$changed].ms -ge 220 -or $gap -gt 75){throw 'Native orbit samples did not resolve movement before a 220 ms tween could complete'}
    if($Samples.Count-$changed -lt 3){throw 'Native orbit return needs at least two samples after its first movement'}
    $final=$Samples[$Samples.Count-1].bounds;$firstError=& $difference $Samples[$changed].bounds $final;$maximum=$firstError
    for($i=$changed+1;$i -lt $Samples.Count;$i++){$maximum=[Math]::Max($maximum,(& $difference $Samples[$i].bounds $final))}
    if($maximum -gt 2){throw "Returning inside press slop animated native bead bounds instead of snapping: $maximum physical pixels"}
    @{first_change_ms=$Samples[$changed].ms;maximum_ack_sample_gap_ms=$gap;first_change_final_error_px=$firstError;maximum_post_change_error_px=$maximum;post_change_samples=($Samples.Count-$changed-1);geometry_claim='first observed movement already at final bounds'}
}
function Assert-BubbleVisualBeadTransitions([long]$Id){
    $original=View $Id
    if($original.kind -ne 'color' -or !$original.pill -or !$original.options.palette){throw 'Bead transition check requires the open palette pill'}
    $before=Bubble-VisualState $Id;$paletteName=$original.pill;$layout=Workspace-Key;$resized=$false;$down=$false
    $evidence=[ordered]@{id=$Id;palette=$paletteName;alpha_curve='not compared';orbit=@{};revival=@{}}
    $prefix=Join-Path $run "bubble-$Id-bead-transitions"
    try{
        if($original.options.size -ne 136){
            $at=Center $Id;[CapyRowPointer]::RightClick($at.x,$at.y);Invoke 'Small' -Name;$resized=$true
            Wait-Until {(View $Id).options.size -eq 136} 'Native Small option did not resize the wheel';Escape
        }
        Wait-BubbleVisualLayout;Start-Sleep -Milliseconds 300
        $small=View $Id
        $bead=@($small.beads|Where-Object {$_.placement.bead.kind -eq 'swatch' -and $_.placement.bead.index -eq 6})[0]
        $palette=@((Model).state.color_library.palettes|Where-Object name -eq $paletteName)[0]
        if(!$bead -or $palette.swatches.Count -le @($small.beads|Where-Object {$_.placement.bead.kind -eq 'swatch'}).Count){throw 'Small wheel fixture does not overflow its palette orbit'}
        $peer=Bubble-VisualBead $Id $bead;$from=Relative $Id $bead.placement.center
        $angle=[Math]::Atan2($bead.placement.center[1],$bead.placement.center[0]);$radius=[Math]::Sqrt([Math]::Pow($bead.placement.center[0],2)+[Math]::Pow($bead.placement.center[1],2))
        $turn=Relative $Id @(($radius*[Math]::Cos($angle+.32)),($radius*[Math]::Sin($angle+.32)))
        $back=Relative $Id @(($radius*[Math]::Cos($angle+.04)),($radius*[Math]::Sin($angle+.04)))
        $distance={param($a,$b) [Math]::Sqrt([Math]::Pow($a.x-$b.x,2)+[Math]::Pow($a.y-$b.y,2))/$scale}
        $evidence.orbit=@{from=$from;turned=$turn;returned=$back;outward_dip=(& $distance $from $turn);returned_dip=(& $distance $from $back);initial_bounds=(Bubble-VisualBounds $peer);samples=@()}
        if($evidence.orbit.outward_dip -le 8 -or $evidence.orbit.returned_dip -ge 8){throw 'Orbit fixture does not cross and return inside press slop'}
        $null=Screen-Pixels $from.x $from.y ($prefix+'-orbit-before.png')
        [CapyRowPointer]::Down('mouse',$from.x,$from.y);$down=$true;[CapyRowPointer]::Move($turn.x,$turn.y)
        Wait-Until {
            $moved=@((View $Id).beads|Where-Object {$_.placement.bead.kind -eq 'swatch' -and $_.placement.bead.index -eq 6})[0]
            $moved -and [Math]::Abs($moved.placement.center[0]-$bead.placement.center[0]) -gt 8
        } 'Held palette orbit did not move beyond its original position'
        Start-Sleep -Milliseconds 300
        $evidence.orbit.turned_bounds=Bubble-VisualBounds $peer
        $returnClock=[Diagnostics.Stopwatch]::StartNew();[CapyRowPointer]::Move($back.x,$back.y)
        $samples=[Collections.Generic.List[object]]::new()
        do{$bounds=Bubble-VisualBounds $peer;$samples.Add(@{ms=$returnClock.Elapsed.TotalMilliseconds;bounds=$bounds});Start-Sleep -Milliseconds 10}while($returnClock.ElapsedMilliseconds -lt 300)
        $evidence.orbit.samples=@($samples.ToArray())
        $returned=@((View $Id).beads|Where-Object {$_.placement.bead.kind -eq 'swatch' -and $_.placement.bead.index -eq 6})[0]
        $null=Bubble-VisualBead $Id $returned
        $last=$samples[$samples.Count-1].bounds;$turned=$evidence.orbit.turned_bounds
        if([Math]::Sqrt([Math]::Pow($last[0]-$turned[0],2)+[Math]::Pow($last[1]-$turned[1],2)) -le 8*$scale){throw 'Native bead did not follow the held orbit back inside the starting slop'}
        $evidence.orbit.return_motion=Assert-BubbleVisualOrbitReturn $samples $turned
        $evidence.orbit.returned_shared=$returned
        Capture 'orbit-return-held' -Composed -WithModel
    }finally{
        try{if($down){try{Escape}finally{[CapyRowPointer]::Cancel()};$down=$false}}finally{
            try{if($resized){Restore-Bubble $Id;Wait-Until {(Workspace-Key) -eq $layout} 'Workspace Undo did not restore the original wheel size and layout'}}finally{
                $evidence|ConvertTo-Json -Depth 20|Set-Content ($prefix+'.json')
            }
        }
    }
    Wait-BubbleVisualLayout;Start-Sleep -Milliseconds 300
    if((Bubble-VisualState $Id) -ne $before){throw 'Cancelled orbit and size restoration changed the bubble, colors, artwork or workspace'}
    $model=Model;$palettes=@($model.state.color_library.palettes);$index=[Array]::IndexOf(@($palettes.name),$paletteName)
    $choices=@(foreach($direction in @(-1,1)){
        $neighbor=$palettes[($index+$direction+$palettes.Count)%$palettes.Count]
        $retiring=@((View $Id).beads|Where-Object {$_.placement.bead.kind -eq 'swatch' -and $_.placement.bead.index -ge $neighbor.swatches.Count}|Sort-Object {$_.placement.bead.index} -Descending)[0]
        if($retiring){@{direction=$direction;neighbor=$neighbor;bead=$retiring}}
    })
    if(!$choices.Count){throw 'Palette fixture has no neighboring palette that retires a visible bead'}
    $choice=$choices[0];$bead=$choice.bead;$peer=Bubble-VisualBead $Id $bead;$face=Control "tool-bubble-$Id" -Arranged;$pill=(Packet $Id).pill
    $evidence.revival=@{label=$bead.label;index=$bead.placement.bead.index;neighbor=$choice.neighbor.name;direction=$choice.direction;initial_bounds=(Bubble-VisualBounds $peer);native_retirement=$false;native_revival=$false}
    $point=Relative $Id $bead.placement.center;$null=Screen-Pixels $point.x $point.y ($prefix+'-revival-before.png')
    $stepPoint={param($direction) $bounds=$face.Current.BoundingRectangle;@{x=[int]($bounds.X+$bounds.Width*.5+($pill.x+$(if($direction -lt 0){16}else{$pill.width-16}))*$scale);y=[int]($bounds.Y+$bounds.Height*.5+($pill.y+15)*$scale)}}
    try{
        $next=& $stepPoint $choice.direction;$previous=& $stepPoint (-$choice.direction)
        $observer=[CapyBubbleFoldObserver]::new((State-File),$Id,$peer)
        try{
            $evidence.revival.injected_pair_ms=[CapyRowPointer]::RapidReopen('mouse',$next.x,$next.y,$previous.x,$previous.y,$observer.NativeFoldSignal)
            $evidence.revival.native_retirement=$observer.NativeFoldDetectedTicks -gt 0
            $evidence.revival.native_last_visible_ticks=$observer.NativeLastOpenTicks;$evidence.revival.native_retired_ticks=$observer.NativeFoldDetectedTicks
            $evidence.revival.reverse_down_ticks=[CapyRowPointer]::RapidReopenDownTicks;$evidence.revival.reverse_up_ticks=[CapyRowPointer]::RapidReopenUpTicks
            $evidence.revival.native_to_reverse_up_upper_ms=([CapyRowPointer]::RapidReopenUpTicks-$observer.NativeLastOpenTicks)*1000./[Diagnostics.Stopwatch]::Frequency
            $evidence.revival.retirement_to_reverse_up_ms=([CapyRowPointer]::RapidReopenUpTicks-$observer.NativeFoldDetectedTicks)*1000./[Diagnostics.Stopwatch]::Frequency
            $evidence.revival.observer_samples=$observer.Samples
        }finally{$observer.Dispose()}
        $null=Screen-Pixels $point.x $point.y ($prefix+'-reviving.png')
        if(!$evidence.revival.native_retirement -or $evidence.revival.retirement_to_reverse_up_ms -lt 0 -or $evidence.revival.native_to_reverse_up_upper_ms -ge 220){throw "Palette revival was not exercised within 220 ms of native retirement: $($evidence.revival.native_to_reverse_up_upper_ms) ms upper bound"}
        Wait-Until {(View $Id).pill -eq $paletteName} 'Rapid reverse step did not restore the original palette'
        Wait-Until {try{!$peer.Current.IsOffscreen -and $null -ne [System.Windows.Automation.TreeWalker]::RawViewWalker.GetParent($peer)}catch [System.Windows.Automation.ElementNotAvailableException]{$false}} 'Retiring native bead was replaced instead of revived within its transition'
        $evidence.revival.native_revival=$true
        Settled-Capture 'palette-rapid-revival'
        $restored=@((View $Id).beads|Where-Object {$_.placement.bead.kind -eq 'swatch' -and $_.placement.bead.index -eq $bead.placement.bead.index})[0]
        $null=Bubble-VisualBead $Id $restored;$evidence.revival.final_bounds=Bubble-VisualBounds $peer
    }finally{
        try{if($down){[CapyRowPointer]::Cancel();$down=$false}
            if((View $Id).pill -eq $choice.neighbor.name){$restore=& $stepPoint (-$choice.direction);[CapyRowPointer]::Down('mouse',$restore.x,$restore.y);Start-Sleep -Milliseconds 10;[CapyRowPointer]::Up();Wait-Until {(View $Id).pill -eq $paletteName} 'Palette cleanup did not restore its original selection'}
        }finally{$evidence|ConvertTo-Json -Depth 20|Set-Content ($prefix+'.json')}
    }
    Wait-BubbleVisualLayout
    if((Bubble-VisualState $Id) -ne $before){throw 'Bead transitions changed the bubble, colors, artwork or workspace'}
    $checks.Add('orbit-return-inside-slop-snaps-native-geometry')
    $checks.Add('native-palette-bead-revival-within-220ms-and-exact-state-restoration')
}
function Move-BubbleVisual([long]$Id,[double]$X,[double]$Y){
    $view=View $Id;$center=Center $Id;$target=Screen $X $Y
    $from=if($view.kind -eq 'color'){Relative $Id @((-$view.diameter*.5+3),0)}else{$center}
    Drag $from @{x=[int]($from.x+$target.x-$center.x);y=[int]($from.y+$target.y-$center.y)}
    Wait-BubbleVisualLayout
}
function Set-BubbleVisualDiameter([long]$Id,[double]$Diameter){
    $area=(Model).layout.work_area
    Move-BubbleVisual $Id ($area.x+$area.width*.5) ($area.y+$area.height*.5)
    $size=(View $Id).diameter;$center=Center $Id;$spread=[int][Math]::Round(($size*.5-4)*$scale)
    $factor=if($Diameter -eq 128){($Diameter-8)/$size}elseif($Diameter -eq 352){($Diameter+8)/$size}else{$Diameter/$size}
    Touch-Pair ($center.x-$spread) $center.y ($center.x+$spread) $center.y 0 0 $factor 16 "visual-size-$Diameter"
    Wait-Until {[Math]::Abs((View $Id).diameter-$Diameter) -le $(if($Diameter -in @(128,352)){.01}else{2})} "Wheel did not reach its requested $Diameter DIP size"
    Wait-BubbleVisualLayout
}
function Assert-BubbleVisualGeometry([long]$Id,[string]$Stage){
    $view=View $Id;$packet=Packet $Id;$center=Center $Id;$area=(Model).layout.work_area
    $left=$w.X+$area.x*$scale;$top=$w.Y+$area.y*$scale;$right=$left+$area.width*$scale;$bottom=$top+$area.height*$scale
    $regions=[Collections.Generic.List[object]]::new()
    $regions.Add(@{kind='face';x=$center.x-$view.face[0]*$scale*.5;y=$center.y-$view.face[1]*$scale*.5;width=$view.face[0]*$scale;height=$view.face[1]*$scale})
    foreach($bead in @($view.beads)){
        $p=$bead.placement;if($p.alpha -le 0){continue}
        $regions.Add(@{kind=('bead-'+$p.bead.kind);x=$center.x+($p.center[0]-$p.size*.5)*$scale;y=$center.y+($p.center[1]-$p.size*.5)*$scale;width=$p.size*$scale;height=$p.size*$scale})
    }
    foreach($item in @($view.chooser)){
        $size=if($item.kind -eq 'brush'){@(152,48)}else{@(44,44)}
        $regions.Add(@{kind=('chooser-'+$item.label);x=$center.x+($item.center[0]-$size[0]*.5)*$scale;y=$center.y+($item.center[1]-$size[1]*.5)*$scale;width=$size[0]*$scale;height=$size[1]*$scale})
    }
    if($view.pill){$pill=$packet.pill;$regions.Add(@{kind='palette-pill';x=$center.x+$pill.x*$scale;y=$center.y+$pill.y*$scale;width=$pill.width*$scale;height=$pill.height*$scale})}
    $face=Bubble-VisualBounds (Control "tool-bubble-$Id" -Arranged)
    $evidence=@{stage=$Stage;id=$Id;diameter=$view.diameter;work_area=@($left,$top,$right,$bottom);shared_regions=@($regions.ToArray());native_face=$face}
    if($view.kind -eq 'color'){
        $image=Bubble-VisualBounds (Control "bubble-wheel-$Id" -Arranged)
        $evidence.native_wheel=$image;$expected=($view.diameter-14)*$scale
        for($i=2;$i -lt 4;$i++){if([Math]::Abs($image[$i]-$expected) -gt 2){throw 'Native wheel image does not preserve its shared 7 DIP rim'}}
        if([Math]::Abs($image[0]+$image[2]*.5-$center.x) -gt 2 -or [Math]::Abs($image[1]+$image[3]*.5-$center.y) -gt 2){throw 'Native wheel image shifted away from its face center'}
    }
    $evidence|ConvertTo-Json -Depth 12 -Compress|Add-Content (Join-Path $run 'bubble-edge-geometry.jsonl')
    for($i=2;$i -lt 4;$i++){if([Math]::Abs($face[$i]-$view.face[$i-2]*$scale) -gt 2){throw 'Native bubble face size differs from its shared presentation'}}
    foreach($region in $regions){
        if($region.x -lt $left-2 -or $region.y -lt $top-2 -or $region.x+$region.width -gt $right+2 -or $region.y+$region.height -gt $bottom+2){throw "Bubble $Id $($region.kind) exceeds its work area during $Stage"}
    }
    return $evidence
}
function Set-BubbleVisualTransparency([ValidateRange(0,3)][int]$Index){
    $types=[System.Windows.Automation.ControlType];$modes=@('off','low','medium','high')
    (Control 'drawing-canvas').SetFocus()
    [CapyRowPointer]::Chord([uint32]$review.Id,[uint16[]]@(0x11),[uint16]0xBC)
    $dialog=Control 'Preferences' -Name -Type $types::Window
    $circle=Control "preference-transparency-$Index" -Within $dialog
    if($circle.Current.Name -ne @('Off','Low','Medium','High')[$Index]){throw 'Transparency preference has the wrong accessible name'}
    Invoke "preference-transparency-$Index" -Within $dialog
    Wait-Until {$state=(Model).state;$state.settings.transparency -eq $modes[$Index] -and $state.palette.glass.transparency -eq $modes[$Index] -and $circle.Current.ItemStatus -eq 'Selected'} 'Native transparency preference did not update its shared setting'
    $close=$dialog.FindFirst([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.AndCondition]::new(
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty,'CloseButton'),
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,$types::Button)))
    $close.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Wait-Until {!(Find 'Preferences' -Name -Type $types::Window)} 'Native transparency Preferences did not close'
    Wait-BubbleVisualLayout
}
function Open-BubbleVisualChooser([long]$Id){
    for($i=0;$i -lt 4 -and !@((View $Id).chooser).Count;$i++){Tap (Center $Id);Start-Sleep -Milliseconds 250}
    if(!@((View $Id).chooser).Count){throw 'Visual scene dial did not open its chooser'}
    Wait-BubbleVisualLayout
}
function Assert-BubbleVisualEdges([long]$Dial,[long]$Color){
    if((View $Color).kind -ne 'color'){throw 'Visual edge matrix requires a color wheel'}
    $model=Model;$revision=$model.state.document_file.revision;$colors=$model.state.colors|ConvertTo-Json -Depth 30 -Compress
    foreach($diameter in @(128,216,352)){
        Set-BubbleVisualDiameter $Color $diameter
        foreach($corner in @(@{name='top-left';x=0;y=0},@{name='top-right';x=1;y=0},@{name='bottom-right';x=1;y=1},@{name='bottom-left';x=0;y=1})){
            $area=(Model).layout.work_area;$radius=(View $Color).diameter*.5
            $x=$area.x+$(if($corner.x){$area.width-$radius-12}else{$radius+12})
            $y=$area.y+$(if($corner.y){$area.height-$radius-12}else{$radius+12})
            Move-BubbleVisual $Color $x $y
            $palette=@((View $Color).beads|Where-Object {$_.placement.bead.kind -eq 'palettes'})[0]
            if($palette -and !(View $Color).pill){Tap (Relative $Color $palette.placement.center);Wait-Until {(View $Color).pill} 'Palette bead did not reopen at the work-area corner';Wait-BubbleVisualLayout}
            $stage="wheel-$diameter-$($corner.name)"
            $null=Assert-BubbleVisualGeometry $Color $stage
            Settled-Capture $stage
        }
    }
    $area=(Model).layout.work_area
    Move-BubbleVisual $Color ($area.x+$area.width*.5) ($area.y+$area.height*.55)
    $center=Center $Color
    Move-BubbleVisual $Dial (($center.x-$w.X)/$scale+120) (($center.y-$w.Y)/$scale)
    Open-BubbleVisualChooser $Dial
    $null=Assert-BubbleVisualGeometry $Dial 'crowded-chooser';$null=Assert-BubbleVisualGeometry $Color 'crowded-wheel'
    $dialCenter=Center $Dial;$colorCenter=Center $Color
    $distance=[Math]::Sqrt([Math]::Pow($dialCenter.x-$colorCenter.x,2)+[Math]::Pow($dialCenter.y-$colorCenter.y,2))
    if($distance -lt ((View $Dial).face[0]+(View $Color).face[0])*$scale*.5-2){throw 'Crowded native dial and wheel faces overlap after settling'}
    Settled-Capture 'crowded-chooser-and-wheel'
    $modes=@('off','low','medium','high');$original=(Model).state.palette.glass.transparency
    try{
        foreach($index in 0..3){
            if(@((View $Dial).chooser).Count){Tap (Center $Dial);Wait-Until {!@((View $Dial).chooser).Count} 'Chooser did not close before changing transparency'}
            Set-BubbleVisualTransparency $index
            Settled-Capture "transparency-$($modes[$index])-all"
            Open-BubbleVisualChooser $Dial
            $null=Assert-BubbleVisualGeometry $Dial "transparency-$($modes[$index])-chooser"
            Settled-Capture "transparency-$($modes[$index])-chooser"
            Escape
        }
    }finally{
        if((Model).state.palette.glass.transparency -ne $original){Set-BubbleVisualTransparency ([Array]::IndexOf($modes,$original))}
    }
    $after=Model
    if($after.state.document_file.revision -ne $revision -or ($after.state.colors|ConvertTo-Json -Depth 30 -Compress) -ne $colors){throw 'Native bubble edge matrix changed artwork or selected colors'}
    $checks.Add('wheel-minimum-medium-maximum-four-corners-and-crowded-chooser')
    $checks.Add('all-transparency-levels-with-live-chooser')
}
