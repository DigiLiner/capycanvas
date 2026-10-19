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
function Key([uint16]$Code){[CapyRowPointer]::Key([uint32]$review.Id,$Code)}
function Snapshot{$hit=@{model=$null};Wait-Until {$hit.model=Model;$null -ne $hit.model} 'The UI state is unavailable';$hit.model}
function Rows{@((Snapshot).state.layers)}
function Active{@((Snapshot).state.layers|Where-Object selected)[0]}
function Vector($Row){[bool]($Row -and $Row.content_icon -eq 'layer-path-symbolic')}
function Vectors{@((Snapshot).state.layers|Where-Object content_icon -eq 'layer-path-symbolic')}
function Position([long]$Id){$rows=Rows;for($i=0;$i -lt $rows.Count;$i++){if($rows[$i].id -eq $Id){return $i}};-1}
function Revision{(Snapshot).state.document_file.revision}
function Label([string]$Id){@((Snapshot).state.commands|Where-Object id -eq $Id)[0].label}
function Selected([string]$Id){[bool]@((Snapshot).state.commands|Where-Object id -eq $Id)[0].selected}
function Brush-Ready([long]$Preset){$m=Snapshot;$m.state.brush.preset -eq $Preset -and $m.brush_ready}
function Center($Element){$r=$Element.Current.BoundingRectangle;@([int]($r.X+$r.Width/2),[int]($r.Y+$r.Height/2))}
function Click($Element){
 $at=Center $Element
 [CapyRowPointer]::Down('mouse',$at[0],$at[1]);Start-Sleep -Milliseconds 40;[CapyRowPointer]::Up();Start-Sleep -Milliseconds 120
}
function Park{$at=(Control 'settings-button' -Arranged).Current.BoundingRectangle;[CapyRowPointer]::Hover([int]$at.X+2,[int]$at.Y+2);Start-Sleep -Milliseconds 300}
function Grab{
 $b=$root.Current.BoundingRectangle;$x=[Math]::Max(0,[int]$b.X);$y=[Math]::Max(0,[int]$b.Y)
 $bitmap=[Drawing.Bitmap]::new([int]($b.Right-$x),[int]($b.Bottom-$y));$graphics=[Drawing.Graphics]::FromImage($bitmap)
 try{$graphics.CopyFromScreen($x,$y,0,0,$bitmap.Size)}finally{$graphics.Dispose()}
 $bitmap
}
function Save-Grab($Bitmap,[string]$Name){try{$Bitmap.Save((Join-Path $run ($Name+'.png')),[Drawing.Imaging.ImageFormat]::Png)}finally{$Bitmap.Dispose()}}
function Camera-Key($Camera){@($Camera.viewport,$Camera.translation,$Camera.zoom,$Camera.rotation,$Camera.flipped)|ConvertTo-Json -Depth 5 -Compress}
function Take-Projection{
 $seen=@{key=$null;stable=0}
 Wait-Until {$next=Camera-Key (Model).state.camera;if($next -eq $seen.key){$seen.stable++}else{$seen.key=$next;$seen.stable=0};$seen.stable -ge 4} 'The fitted camera did not settle'
 $camera=(Model).state.camera;$bounds=(Control 'drawing-canvas' -Arranged).Current.BoundingRectangle
 if($camera.rotation -ne 0 -or $camera.flipped[0] -or $camera.flipped[1]){throw 'The journey requires an unrotated canvas'}
 $script:projection=@{camera=$camera;bounds=$bounds;key=(Camera-Key $camera)}
 $projection|ConvertTo-Json -Depth 6|Set-Content -LiteralPath (Join-Path $run 'projection.json')
}
function Check-Projection{
 if((Camera-Key (Model).state.camera) -ne $projection.key -or (Control 'drawing-canvas' -Arranged).Current.BoundingRectangle -ne $projection.bounds){throw 'The canvas or camera moved during the journey'}
}
function Screen-Point($At){
 $c=$projection.camera;$b=$projection.bounds;$w=$c.work_area
 $vx=$At[0]*$c.zoom+$c.translation[0];$vy=$At[1]*$c.zoom+$c.translation[1]
 if($w -and ($vx -lt $w[0] -or $vy -lt $w[1] -or $vx -gt $w[0]+$w[2] -or $vy -gt $w[1]+$w[3])){throw "Document point $($At -join ',') lies outside the canvas work area"}
 @([int][Math]::Round($b.X+$vx*$b.Width/$c.viewport[0]),[int][Math]::Round($b.Y+$vy*$b.Height/$c.viewport[1]))
}
function Patch($At,[int]$Radius=4){
 $p=Screen-Point $At;$size=2*$Radius+1
 $bitmap=[Drawing.Bitmap]::new($size,$size);$graphics=[Drawing.Graphics]::FromImage($bitmap)
 try{
  $graphics.CopyFromScreen($p[0]-$Radius,$p[1]-$Radius,0,0,$bitmap.Size)
  $level=256;$darkest=$null;$sum=@(0,0,0)
  for($y=0;$y -lt $size;$y++){for($x=0;$x -lt $size;$x++){
   $c=$bitmap.GetPixel($x,$y);$top=[Math]::Max([int]$c.R,[Math]::Max([int]$c.G,[int]$c.B))
   if($top -lt $level){$level=$top;$darkest=@([int]$c.R,[int]$c.G,[int]$c.B)}
   $sum[0]+=$c.R;$sum[1]+=$c.G;$sum[2]+=$c.B
  }}
  [pscustomobject]@{screen=$p;darkest=$darkest;level=$level;mean=@($sum|ForEach-Object {[Math]::Round($_/($size*$size),1)})}
 }finally{$graphics.Dispose();$bitmap.Dispose()}
}
function Classified($Sample,[string]$Kind){
 switch($Kind){
  'ink'{$Sample.level -lt 110}
  'mark'{$Sample.darkest[0]+$Sample.darkest[1]+$Sample.darkest[2] -lt 450}
  'green'{$Sample.mean[1]-[Math]::Max($Sample.mean[0],$Sample.mean[2]) -gt 40}
 }
}
function Expect([string]$Check,$At,[ValidateSet('ink','mark','green')][string]$Kind,[bool]$Wanted=$true,[int]$Limit=30,[switch]$Deferred){
 Check-Projection
 $seen=@{sample=$null}
 try{Wait-Until {$seen.sample=Patch $At;[bool](Classified $seen.sample $Kind) -eq $Wanted} $Check $Limit}
 catch{
  $failure="$Check at $($At -join ','): wanted $Kind=$Wanted, darkest $($seen.sample.darkest -join ','), mean $($seen.sample.mean -join ',')"
  if(!$Deferred){throw "$failure; $($_.Exception.Message)"}
  $findings.Add($failure)
 }
 $observations.Add([ordered]@{check=$Check;kind=$Kind;wanted=$Wanted;at=$At;screen=$seen.sample.screen;darkest=$seen.sample.darkest;mean=$seen.sample.mean})
}
function Line($From,$To,[int]$Steps=20){@(for($i=0;$i -le $Steps;$i++){,@(($From[0]+($To[0]-$From[0])*$i/$Steps),($From[1]+($To[1]-$From[1])*$i/$Steps))})}
function Stroke($Points,[switch]$Refused){
 Check-Projection
 $screen=@(foreach($point in $Points){,(Screen-Point $point)})
 if(![CapyRowPointer]::SetForegroundWindow($drawingWindow)){throw 'The owned drawing did not accept foreground input'}
 $script:contact=[Diagnostics.Stopwatch]::StartNew()
 [CapyRowPointer]::Down('pen',$screen[0][0],$screen[0][1])
 try{
  for($i=1;$i -lt $screen.Count;$i++){Start-Sleep -Milliseconds 16;[CapyRowPointer]::Move($screen[$i][0],$screen[$i][1])}
  Start-Sleep -Milliseconds 30;[CapyRowPointer]::Up()
 }finally{[CapyRowPointer]::Cancel()}
 if(!$Refused){Park}
}
function Search([string]$Query,[scriptblock]$Pick,[string]$Wanted){
 if(![CapyRowPointer]::SetForegroundWindow($drawingWindow)){throw 'The owned drawing did not accept foreground input'}
 (Control 'drawing-canvas').SetFocus()
 [CapyRowPointer]::Chord([uint32]$review.Id,[uint16[]]@(0x11),[uint16]0x4B)
 Wait-Until {Find 'command-search' -Visible} 'Command search did not open'
 (Control 'command-search').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($Query)
 $hit=@{index=-1;result=$null}
 Wait-Until {
  $view=(Model).state.command_search
  if(!$view -or $view.query -ne $Query){return $false}
  $results=@($view.results)
  for($i=0;$i -lt $results.Count;$i++){if(& $Pick $results[$i]){$hit.index=$i;$hit.result=$results[$i];return $true}}
  $false
 } "Command search did not offer $Wanted"
 $row=Control ('command-result-'+$hit.index) -Arranged
 if($row.Current.Name -ne $hit.result.label){throw "Command search shows '$($row.Current.Name)' where Rust offers '$($hit.result.label)'"}
 $row.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
 Wait-Until {!(Find 'command-search' -Visible) -and !(Model).state.command_search} 'Command search did not close'
 $hit.result
}
function Select-Brush([string]$Brush){
 $pick={param($result) $result.label -in @($Brush,"$Brush brush") -and $result.id -like 'action:*"select_brush"*'}.GetNewClosure()
 $result=Search $Brush $pick "the $Brush brush"
 $preset=[long](($result.id -replace '^action:')|ConvertFrom-Json).id
 Wait-Until {Brush-Ready $preset} "$Brush did not become the ready brush" 120
 $preset
}
function Select-Tool([string]$Id){
 if(!(Selected $Id)){
  Invoke (Tool-Tile $Id)
  Wait-Until {(Model).canvas_ready -and (Selected $Id)} "$Id did not become the tool"
 }
 if((Model).state.customization.drawer){
  (Control 'drawing-canvas').SetFocus();Key 0x1B
  Wait-Until {!(Model).state.customization.drawer} 'The tool drawer did not close'
 }
 Wait-Until {(Model).brush_ready} "$Id did not become ready" 120
 Park
}
function Tool-Item([string]$List,[scriptblock]$Match,[string]$Wanted){
 $hit=@{index=-1;item=$null}
 Wait-Until {
  $items=@((Model).state.tool_set.$List)
  for($i=0;$i -lt $items.Count;$i++){if(& $Match $items[$i]){$hit.index=$i;$hit.item=$items[$i];return $true}}
  $false
 } "The Tools list did not offer $Wanted"
 $id=$(if($List -eq 'groups'){'tool-group-'}else{'tool-subtool-'})+$hit.index
 $native=@{name=$null}
 try{Wait-Until {$button=Find $id;$native.name=if($button){$button.Current.Name};$native.name -eq $hit.item.label} "The Tools list button $id is not labelled $($hit.item.label)"}
 catch{throw "The Tools list button $id is labelled '$($native.name)' where Rust offers '$($hit.item.label)'"}
 [pscustomobject]@{id=$id;list=$List;index=$hit.index;label=$hit.item.label;selected=[bool]$hit.item.selected}
}
function Reveal([string]$Id){
 $item=Control $Id
 $walker=[System.Windows.Automation.TreeWalker]::ControlViewWalker;$node=$walker.GetParent($item);$scroll=$null;$viewport=$null
 while($node -and !$scroll){
  $pattern=$null
  if($node.TryGetCurrentPattern([System.Windows.Automation.ScrollPattern]::Pattern,[ref]$pattern) -and $pattern.Current.VerticallyScrollable){$scroll=$pattern;$viewport=$node}
  $node=$walker.GetParent($node)
 }
 for($step=0;$step -lt 40;$step++){
  $item=Control $Id;$bounds=$item.Current.BoundingRectangle
  $clip=if($viewport){$viewport.Current.BoundingRectangle}else{$root.Current.BoundingRectangle};$clip.Inflate(1,1)
  if(!$item.Current.IsOffscreen -and !$bounds.IsEmpty -and $clip.Contains($bounds)){return (Control $Id -Arranged)}
  if(!$scroll){throw "$Id is clipped outside any scroll container"}
  $before=$scroll.Current.VerticalScrollPercent
  $amount=if(!$bounds.IsEmpty -and $bounds.Bottom -le $clip.Top){[System.Windows.Automation.ScrollAmount]::SmallDecrement}else{[System.Windows.Automation.ScrollAmount]::SmallIncrement}
  $scroll.Scroll([System.Windows.Automation.ScrollAmount]::NoAmount,$amount)
  Wait-Until {[Math]::Abs($scroll.Current.VerticalScrollPercent-$before) -gt .01} "$Id did not scroll into view"
 }
 throw "$Id stayed clipped in its scroll container"
}
function Choose-Tool-Item($Choice,[switch]$Acknowledged){
 if($Choice.selected){return $true}
 Click (Reveal $Choice.id);Park
 $ack=@{native=$false;shared=$false}
 try{Wait-Until {$button=Find $Choice.id;$ack.native=[bool]($button -and $button.Current.ItemStatus);$ack.shared=[bool]@((Model).state.tool_set.($Choice.list))[$Choice.index].selected;$ack.native -or $ack.shared} "$($Choice.label) did not become selected"}
 catch{if($Acknowledged){throw}}
 $ack.native -or $ack.shared
}
function Set-Color([string]$Hex){
 Invoke-Id 'color-edit'
 Wait-Until {Find 'edit-color-apply'} 'Edit Color did not open'
 Invoke 'edit-color-hex';$entry=Control 'edit-color-hex-entry';$entry.SetFocus()
 $entry.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($Hex);Key 0x0D
 Invoke 'edit-color-apply';Wait-Until {!(Find 'edit-color-apply')} 'Edit Color did not close'
 $rgb=@(1,3,5|ForEach-Object {[Convert]::ToInt32($Hex.Substring($_,2),16)/255})
 Wait-Until {$c=(Model).state.brush.color;$c -and [Math]::Abs($c[0]-$rgb[0]) -lt .01 -and [Math]::Abs($c[1]-$rgb[1]) -lt .01 -and [Math]::Abs($c[2]-$rgb[2]) -lt .01} "The current color did not become $Hex"
}
function New-VectorLayer{
 $before=@(Rows).Count;$button=Control 'layer-new-vector' -Arranged
 if(!$button.Current.IsEnabled){throw 'New vector layer is disabled'}
 Click $button;Park
 Wait-Until {@(Rows).Count -eq $before+1 -and (Vector (Active))} 'The New vector layer button did not add an active vector layer'
 $id=(Active).id
 $null=Control "layer-$id-name"
 $id
}
function Footer-Check([string]$Name){
 $scale=[CapyRowPointer]::GetDpiForWindow($drawingWindow)/96
 $frame=(Control 'layer-footer' -Arranged).Current.BoundingRectangle;$inside=$frame;$inside.Inflate(1,1)
 $record=[ordered]@{scale=$scale;footer=@($frame.X,$frame.Y,$frame.Width,$frame.Height);buttons=@()}
 $previous=$null
 foreach($id in 'layer-new','layer-new-vector','layer-new-group','layer-new-selection','layer-add-mask','layer-add-filter','layer-import','layer-delete','layer-actions'){
  $button=Find $id -Visible
  if(!$button){$findings.Add("The Layers footer does not show $id");$record.buttons+=[ordered]@{id=$id;shown=$false};continue}
  $b=$button.Current.BoundingRectangle
  $record.buttons+=[ordered]@{id=$id;shown=$true;name=$button.Current.Name;enabled=$button.Current.IsEnabled;bounds=@($b.X,$b.Y,$b.Width,$b.Height)}
  if(!$button.Current.Name){$findings.Add("The Layers footer button $id has no accessible name")}
  if(!$inside.Contains($b) -or [Math]::Abs($b.Width-24*$scale) -gt 2 -or [Math]::Abs($b.Height-24*$scale) -gt 2){
   $findings.Add("The Layers footer clips $id at $([Math]::Round($b.X)),$([Math]::Round($b.Y)) $([Math]::Round($b.Width))x$([Math]::Round($b.Height)) within $([Math]::Round($frame.X)),$([Math]::Round($frame.Y)) $([Math]::Round($frame.Width))x$([Math]::Round($frame.Height))")
  }
  if($previous -and $b.Left -lt $previous.Right-1){$findings.Add("The Layers footer overlaps $id with the button before it")}
  $previous=$b
 }
 $record|ConvertTo-Json -Depth 6|Set-Content -LiteralPath (Join-Path $run "layers-footer-$Name.json")
}
function Notice-Parts{
 if(!$script:noticeFrame){
  $script:noticeFrame=$root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty,'canvas-notice'))
  if(!$script:noticeFrame){return [ordered]@{}}
 }
 $request=[System.Windows.Automation.CacheRequest]::new()
 foreach($property in [System.Windows.Automation.AutomationElement]::AutomationIdProperty,[System.Windows.Automation.AutomationElement]::NameProperty,[System.Windows.Automation.AutomationElement]::BoundingRectangleProperty,[System.Windows.Automation.AutomationElement]::IsOffscreenProperty){$request.Add($property)}
 $active=$request.Activate()
 try{$items=$script:noticeFrame.FindAll([System.Windows.Automation.TreeScope]::Subtree,[System.Windows.Automation.Condition]::TrueCondition)}finally{$active.Dispose()}
 $parts=[ordered]@{}
 foreach($item in $items){$c=$item.Cached;if($c.AutomationId -like 'canvas-notice*' -and !$c.IsOffscreen){$parts[$c.AutomationId]=[pscustomobject]@{name=$c.Name;bounds=$c.BoundingRectangle}}}
 $parts
}
function Accept-Notice([string]$Action,[string[]]$Offers,[string]$Name,[string]$Label){
 $hit=@{parts=$null;polls=[Collections.Generic.List[long]]::new()}
 $key="canvas-notice-action-$Action"
 try{
  Wait-Until {
   $poll=[Diagnostics.Stopwatch]::StartNew();$parts=Notice-Parts;$hit.polls.Add($poll.ElapsedMilliseconds)
   if($parts.Contains('canvas-notice') -and $parts.Contains($key)){$hit.parts=$parts;$hit.native_ms=$contact.ElapsedMilliseconds}
   $null -ne $hit.parts
  } "No notice offered $Label" 30
 }catch{throw "No notice offered $Label ($($contact.ElapsedMilliseconds) ms after the contact; native frame found: $([bool]$script:noticeFrame); slowest notice query $(($hit.polls|Measure-Object -Maximum).Maximum) ms): $($_.Exception.Message)"}
 $picture=Grab
 $clicked=$contact.ElapsedMilliseconds
 $parts=$hit.parts;$target=$parts[$key].bounds
 [CapyRowPointer]::Down('mouse',[int]($target.X+$target.Width/2),[int]($target.Y+$target.Height/2));Start-Sleep -Milliseconds 40;[CapyRowPointer]::Up()
 Save-Grab $picture "$Name-notice-$Theme";Park
 $frame=$parts['canvas-notice'].bounds;$contained=$frame;$contained.Inflate(1,1)
 $actions=@($parts.Keys|Where-Object {$_ -like 'canvas-notice-action-*'})
 $presented=[ordered]@{name=$Name;text=$(if($parts.Contains('canvas-notice-text')){$parts['canvas-notice-text'].name});frame=@($frame.X,$frame.Y,$frame.Width,$frame.Height);native_ms=$hit.native_ms;clicked_ms=$clicked;slowest_query_ms=($hit.polls|Measure-Object -Maximum).Maximum;actions=@()}
 foreach($id in $actions){
  $button=$parts[$id]
  $presented.actions+=[ordered]@{id=($id -replace '^canvas-notice-action-');name=$button.name;bounds=@($button.bounds.X,$button.bounds.Y,$button.bounds.Width,$button.bounds.Height)}
  if(!$button.name){$findings.Add("The $Name notice's $id has no label")}
  if(!$contained.Contains($button.bounds)){$findings.Add("The $Name notice clips its $($button.name) button")}
 }
 $presented|ConvertTo-Json -Depth 6|Set-Content -LiteralPath (Join-Path $run "$Name-notice.json")
 if(!$presented.text){$findings.Add("The $Name notice shows no text")}
 $ids=@($presented.actions|ForEach-Object id) -join ','
 if($ids -ne ($Offers -join ',')){throw "The $Name notice offered $ids instead of $($Offers -join ',')"}
 if($parts[$key].name -ne $Label){throw "The $Name notice labels $Action '$($parts[$key].name)' instead of '$Label'"}
 Wait-Until {!(Notice-Parts).Contains('canvas-notice')} "Choosing $($parts[$key].name) did not close the $Name notice"
 $presented
}
function Shortcut-Command([string]$Id){
 $binding=@(@((Snapshot).state.commands|Where-Object id -eq $Id)[0].bindings|Where-Object {$_.key.Length -eq 1})[0]
 if(!$binding){throw "$Id has no single-key shortcut"}
 $modifiers=[uint16[]]@($(if($binding.command){0x11}),$(if($binding.shift){0x10}),$(if($binding.alt){0x12})|Where-Object {$_})
 if(![CapyRowPointer]::SetForegroundWindow($drawingWindow)){throw 'The owned drawing did not accept foreground input'}
 (Control 'drawing-canvas').SetFocus()
 $script:contact=[Diagnostics.Stopwatch]::StartNew()
 [CapyRowPointer]::Chord([uint32]$review.Id,$modifiers,[uint16][char]$binding.key.ToUpperInvariant())
}
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
