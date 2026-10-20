param([Parameter(Mandatory)][string]$Executable,[ValidateSet('dark','light')][string]$Theme='dark')
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'CapyUia.ps1')
$CapyFind='prefer-visible';$CapyWaitSeconds=20;$CapyEach={[CapyRowPointer]::Verify()}
Add-Type -AssemblyName System.Drawing
Add-Type -Path (Join-Path $PSScriptRoot 'RowPointerDriver.cs')
$null=[CapyRowPointer]::SetThreadDpiAwarenessContext([IntPtr](-4))
$repo=(Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$Executable=(Resolve-Path -LiteralPath $Executable).Path
$run=Join-Path $repo ('artifacts/windows/vector-layers/'+$Theme+'-'+[Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($run)|Out-Null
$CapyTraceDirectory=$run
$checks=[ordered]@{theme=$Theme}
$observations=[Collections.Generic.List[object]]::new()
$findings=[Collections.Generic.List[string]]::new()
$review=$null;$projection=$null;$noticeFrame=$null
. (Join-Path $PSScriptRoot 'VectorFixture.ps1')
try{
 Enter-CapyEnvironment
 $env:CAPY_STORAGE_DIR=Join-Path $run 'profile';$env:CAPY_TRACE_UI='1'
 [IO.File]::WriteAllText((Settings-File),(@{language=@{Explicit='en'};theme=$Theme}|ConvertTo-Json -Depth 4))
 $stderr=Join-Path $run 'stderr.log'
 $review=Start-Process -FilePath $Executable -WorkingDirectory $run -WindowStyle Hidden -PassThru -RedirectStandardError $stderr
 $null=$review.Handle
 Write-Output "Owned vector layer review $($review.Id): $run"
 $native=@{window=$null}
 Wait-Until {$native.window=Owned-DrawingWindow $review;$m=Model;$native.window -and $m.canvas_ready -and $m.brush_ready -and $m.windows_workspace.ready -and !$m.windows_workspace.busy} 'The vector layer review did not start' 90
 $root=$native.window.Root;$drawingWindow=$native.window.Handle
 $root.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).SetWindowVisualState([System.Windows.Automation.WindowVisualState]::Maximized)
 if(![CapyRowPointer]::SetForegroundWindow($drawingWindow)){throw 'The owned drawing did not accept foreground input'}
 [CapyRowPointer]::Initialize([uint32]$review.Id)
 Wait-Until {(Control 'drawing-canvas' -Arranged).Current.BoundingRectangle.Width -gt 1200 -and !(Model).windows_workspace.busy} 'The maximized canvas did not settle'
 if((Model).state.theme -ne $Theme){throw "The review opened in $((Model).state.theme) instead of $Theme"}
 Use-PaintWorkspace
 Set-Color '#1A1A1F'
 Fit-Canvas;Park;Take-Projection
 Footer-Check 'start'

 $pencil=Select-Brush 'Pencil'
 $v1=New-VectorLayer
 $gpen=@{id=$null}
 Wait-Until {
  $m=Model;$tool=@($m.state.tool_set.subtools|Where-Object selected)[0]
  $gpen.id=$m.state.brush.preset
  $tool.label -eq 'G-Pen' -and $tool.action.type -eq 'select_brush' -and $tool.action.id -eq $m.state.brush.preset -and $m.brush_ready
 } 'The new vector layer did not switch the brush to G-Pen' 120
 $gpen=$gpen.id
 $revision=Revision
 Stroke (Line @(400,300) @(900,340))
 Wait-Until {(Revision) -ne $revision} 'The pen stroke did not reach the vector layer'
 Expect 'discover ink' @(650,320) 'ink'
 if((Active).id -ne $v1){throw 'Inking moved away from the vector layer'}
 Capture "1-discover-$Theme" -WithModel -Composed
 $checks.discover_and_ink=[ordered]@{vector_layer=$v1;pencil=$pencil;g_pen=$gpen}
 Write-Output 'PASS: New vector layer chooses G-Pen and inks a line'

 $null=Select-Brush 'Pencil'
 Stroke (Line @(400,500) @(900,540) 4) -Refused
 $notice=Accept-Notice 'use_vector_pen' @('use_vector_pen','new_paint_layer','rasterize_layer') 'wrong-brush' 'Use G-Pen'
 $offer=@($notice.actions|Where-Object id -eq 'use_vector_pen')[0].name
 Wait-Until {Brush-Ready $gpen} "$offer did not choose G-Pen" 120
 Expect 'the refused pencil left the paths alone' @(650,520) 'ink' $false 5
 $revision=Revision
 Stroke (Line @(400,500) @(900,540))
 Wait-Until {(Revision) -ne $revision} 'The chosen pen did not reach the vector layer'
 Expect 'pen line after the notice' @(650,520) 'ink'
 Capture "2-use-pen-$Theme" -WithModel -Composed
 $null=Select-Brush 'Pencil'
 $before=@(Rows).Count
 Stroke (Line @(400,700) @(900,740) 4) -Refused
 $null=Accept-Notice 'new_paint_layer' @('use_vector_pen','new_paint_layer','rasterize_layer') 'paint-layer' 'New Paint Layer'
 Wait-Until {$a=Active;@(Rows).Count -eq $before+1 -and $a -and !(Vector $a)} 'New Paint Layer did not add an active paint layer'
 $p2=(Active).id
 if((Position $p2)+1 -ne (Position $v1)){throw 'The new paint layer is not directly above the vector layer'}
 if((Model).state.brush.preset -ne $pencil){throw 'New Paint Layer changed the brush'}
 Expect 'the refused pencil left nothing' @(650,720) 'mark' $false 5
 $paint=(Active).paint_revision
 Stroke (Line @(400,700) @(900,740))
 Wait-Until {$a=Active;$a.id -eq $p2 -and $a.paint_revision -ne $paint} 'The pencil did not paint the new paint layer' 60
 Expect 'pencil on the new paint layer' @(650,720) 'mark'
 Capture "2-paint-layer-$Theme" -WithModel -Composed
 $checks.wrong_brush=[ordered]@{use_pen=$offer;paint_layer=$p2}
 Write-Output 'PASS: Wrong-brush notice offers the pen and a paint layer'

 $v2=New-VectorLayer
 Wait-Until {Brush-Ready $gpen} 'The second vector layer did not switch the brush to G-Pen' 120
 Select-Tool 'figure'
 $null=Choose-Tool-Item (Tool-Item 'groups' {param($item) $item.icon -eq 'ellipse'} 'Ellipse') -Acknowledged
 $null=Choose-Tool-Item (Tool-Item 'subtools' {param($item) $item.action.action.tool.figure.shape -eq 'ellipse' -and $item.action.action.tool.figure.paint -eq 'outline'} 'an ellipse outline') -Acknowledged
 $revision=Revision
 Stroke (Line @(1100,300) @(1400,500) 8)
 Wait-Until {(Revision) -ne $revision} 'The ellipse outline did not reach the vector layer'
 Expect 'ellipse outline' @(1250,300) 'ink'
 Set-Color '#26B340'
 Select-Tool 'fill'
 $null=Choose-Tool-Item (Tool-Item 'subtools' {param($item) $item.action.action.tool.region.fill -and $item.action.action.tool.region.source -eq 'visible'} 'Visible') -Acknowledged
 $before=@(Rows).Count
 Stroke (Line @(1250,400) @(1250,400) 1)
 Expect 'filled inside the outline' @(1250,400) 'green' $true 60
 if(@(Rows).Count -ne $before -or (Active).id -ne $v2){throw 'Filling inside the outline changed the layers'}
 Stroke (Line @(1600,900) @(1600,900) 1)
 Wait-Until {$a=Active;@(Rows).Count -eq $before+1 -and $a -and !(Vector $a)} 'Filling outside the lines did not add a paint layer' 60
 $p3=(Active).id
 if((Position $p3) -ne (Position $v2)+1){throw 'The filled paint layer is not directly below the lines'}
 Expect 'filled below the lines' @(1600,900) 'green' $true 90
 Expect 'the outline keeps its fill' @(1250,400) 'green'
 Expect 'the outline stays above the fill' @(1250,300) 'ink'
 Capture "3-color-$Theme" -WithModel -Composed
 Set-Color '#1A1A1F'
 $checks.color_lines=[ordered]@{vector_layer=$v2;paint_below=$p3}
 Write-Output 'PASS: Fill colors inside the outline and below the lines'

 $v3=New-VectorLayer
 Select-Tool 'pen'
 Wait-Until {Brush-Ready $gpen} 'The pen tool did not return to G-Pen' 120
 foreach($segment in @(@(@(300,1000),@(1100,1000)),@(@(500,850),@(500,1150)),@(@(900,850),@(900,1150)))){
  $revision=Revision
  Stroke (Line $segment[0] $segment[1])
  Wait-Until {(Revision) -ne $revision} 'A crossing line did not reach the vector layer'
 }
 foreach($point in @(@(400,1000),@(700,1000),@(1000,1000),@(500,900),@(900,900))){Expect 'crossing lines' $point 'ink'}
 Capture "4-crossing-lines-$Theme" -WithModel -Composed
 $known=$findings.Count
 Select-Tool 'eraser'
 $modes=@(foreach($mode in 'erase_touched_part','erase_whole_paths','erase_to_intersection'){
  $match={param($item) $item.action.command -eq $mode}.GetNewClosure()
  $choice=Tool-Item 'subtools' $match (Label $mode)
  if($choice.label -ne (Label $mode)){throw "The Tools list labels $mode '$($choice.label)' instead of '$(Label $mode)'"}
  $choice
 })
 if(($modes.index -join ',') -ne '0,1,2'){throw "The Tools list does not start with the vector eraser modes: $($modes.index -join ',')"}
 $chosen=Choose-Tool-Item $modes[2]
 if(!$chosen){$findings.Add("Choosing $($modes[2].label) in the Tools list did not take effect")}
 $revision=Revision
 Stroke (Line @(700,980) @(700,1020) 4)
 Wait-Until {(Revision) -ne $revision} 'The eraser did not edit the vector layer'
 Expect 'span between the crossings' @(700,1000) 'ink' $false 60
 foreach($point in @(@(560,1000),@(840,1000))){Expect 'erased up to the crossings, beyond the eraser' $point 'ink' $false -Deferred}
 foreach($point in @(@(400,1000),@(1000,1000),@(500,900),@(900,900))){Expect 'kept line parts' $point 'ink'}
 Capture "4-erase-to-intersection-$Theme" -WithModel -Composed
 $checks.erase_to_intersection=[ordered]@{vector_layer=$v3;modes=@($modes.label)}
 Write-Output $(if($findings.Count -eq $known){'PASS: Erase up to intersection removes only the span between crossings'}else{'FAIL: Erase up to intersection: '+(@($findings)[$known..($findings.Count-1)] -join '; ')})

 Select-Tool 'gradient'
 $before=@(Rows).Count
 Stroke (Line @(1500,700) @(1800,700) 4) -Refused
 $null=Accept-Notice 'new_paint_layer_below' @('new_paint_layer_below','rasterize_layer') 'gradient' 'New Paint Layer Below'
 Wait-Until {$a=Active;@(Rows).Count -eq $before+1 -and $a -and !(Vector $a)} 'New Paint Layer Below did not add an active paint layer'
 $p4=(Active).id
 if((Position $p4) -ne (Position $v3)+1){throw 'The new paint layer is not directly below the crossing lines'}
 Invoke "layer-$v3-name"
 Wait-Until {(Active).id -eq $v3} 'The crossing lines did not become the active layer again'
 $vectors=@(Vectors).Count
 Shortcut-Command 'merge_down'
 $null=Accept-Notice 'merge' @('merge') 'merge' (Label 'merge_down')
 Wait-Until {@(Vectors).Count -eq $vectors-1 -and !@(Vectors|Where-Object id -eq $v3).Count -and !(Vector (Active))} 'Merge Down did not turn the vector layer into pixels' 120
 foreach($point in @(@(400,1000),@(1000,1000),@(500,900),@(900,900))){Expect 'merged line parts' $point 'ink' $true 60}
 Expect 'merged span between the crossings' @(700,1000) 'ink' $false
 Capture "5-merged-$Theme" -WithModel -Composed
 $checks.merge=[ordered]@{paint_below=$p4;merged_into=(Active).id;vector_layers=@(Vectors).Count}
 Write-Output 'PASS: Merge Down rasterizes the lines through its notice'

 Footer-Check 'end'
 [CapyRowPointer]::Dispose()
 & (Join-Path $PSScriptRoot 'exercise-window.ps1') -ProcessId $review.Id -WindowHandle $drawingWindow.ToInt64() -Action Close -DiscardUnsaved -StateDirectory $run
 if((Get-Item -LiteralPath $stderr).Length){throw 'Native stderr requires inspection'}
 if($findings.Count){throw "Journey findings: $($findings -join '; ')"}
 $checks.evidence=$run
 $checks|ConvertTo-Json -Depth 6|Tee-Object -FilePath (Join-Path $run 'results.json')
}catch{
 if($review -and !$review.HasExited -and $root){try{Capture 'failure' -WithModel -Composed}catch{}}
 [IO.File]::WriteAllText((Join-Path $run 'failure.txt'),($_|Out-String)+$_.ScriptStackTrace);throw
}finally{
 try{[CapyRowPointer]::Dispose()}catch{}
 ConvertTo-Json -InputObject @($observations) -Depth 6|Set-Content -LiteralPath (Join-Path $run 'pixels.json')
 ConvertTo-Json -InputObject @($findings)|Set-Content -LiteralPath (Join-Path $run 'findings.json')
 Exit-CapyEnvironment
}
