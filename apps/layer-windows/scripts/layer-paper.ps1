Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Security.Cryptography;
public static class CapyPaperFixture {
    static void Big(Stream s,uint v){s.WriteByte((byte)(v>>24));s.WriteByte((byte)(v>>16));s.WriteByte((byte)(v>>8));s.WriteByte((byte)v);}
    static uint Crc(byte[] bytes){
        uint crc=0xffffffff;
        foreach(var b in bytes){crc^=b;for(int k=0;k<8;k++)crc=(crc&1)!=0?(crc>>1)^0xEDB88320u:crc>>1;}
        return crc^0xffffffff;
    }
    static void Chunk(Stream s,string type,byte[] data){
        var body=new byte[4+data.Length];System.Text.Encoding.ASCII.GetBytes(type).CopyTo(body,0);data.CopyTo(body,4);
        Big(s,(uint)data.Length);s.Write(body,0,body.Length);Big(s,Crc(body));
    }
    public static string Write(string directory){
        const int side=64;var rows=new byte[side*side];
        for(int y=0;y<side;y++)for(int x=0;x<side;x++)rows[y*side+x]=(byte)(128+90*Math.Sin(x*Math.PI/8)*Math.Cos(y*Math.PI/16));
        var head=new byte[24];BitConverter.GetBytes((uint)side).CopyTo(head,0);BitConverter.GetBytes((uint)side).CopyTo(head,4);
        BitConverter.GetBytes(300.0).CopyTo(head,8);BitConverter.GetBytes(300.0).CopyTo(head,16);
        string id;using(var sha=SHA256.Create()){sha.TransformBlock(head,0,head.Length,null,0);sha.TransformFinalBlock(rows,0,rows.Length);id=BitConverter.ToString(sha.Hash).Replace("-","").ToLowerInvariant();}
        var raw=new MemoryStream();for(int y=0;y<side;y++){raw.WriteByte(0);raw.Write(rows,y*side,side);}
        var filtered=raw.ToArray();uint a=1,b=0;foreach(var v in filtered){a=(a+v)%65521;b=(b+a)%65521;}
        var zlib=new MemoryStream();zlib.WriteByte(0x78);zlib.WriteByte(0x01);zlib.WriteByte(1);
        zlib.WriteByte((byte)filtered.Length);zlib.WriteByte((byte)(filtered.Length>>8));zlib.WriteByte((byte)~filtered.Length);zlib.WriteByte((byte)(~filtered.Length>>8));
        zlib.Write(filtered,0,filtered.Length);Big(zlib,(b<<16)|a);
        using(var png=File.Create(Path.Combine(directory,id+".png"))){
            png.Write(new byte[]{137,80,78,71,13,10,26,10},0,8);
            var header=new MemoryStream();Big(header,side);Big(header,side);header.Write(new byte[]{8,0,0,0,0},0,5);
            var density=new MemoryStream();Big(density,11811);Big(density,11811);density.WriteByte(1);
            Chunk(png,"IHDR",header.ToArray());Chunk(png,"pHYs",density.ToArray());Chunk(png,"IDAT",zlib.ToArray());Chunk(png,"IEND",new byte[0]);
        }
        var preview=new byte[96*96];for(int i=0;i<preview.Length;i++)preview[i]=rows[(i/96)%side*side+(i%96)%side];
        File.WriteAllBytes(Path.Combine(directory,id+".preview"),preview);
        return id;
    }
}
'@
function Seed-PaperLibrary{
    $papers=Join-Path $env:CAPY_STORAGE_DIR 'data/papers';[IO.Directory]::CreateDirectory($papers)|Out-Null
    $script:TextureLibraryId=[CapyPaperFixture]::Write($papers)
    [IO.File]::WriteAllText((Join-Path $papers 'index.json'),(@{papers=@(@{id=$TextureLibraryId;name='Fixture scan'})}|ConvertTo-Json -Depth 4 -Compress))
}
function Texture-Visible([int]$Index,$Edit,[double]$Scale){
    $tile=(Control "texture-tile-$Index" -Arranged).Current;$cancel=(Control 'texture-cancel' -Arranged).Current.BoundingRectangle;$r=$tile.BoundingRectangle
    !$tile.IsOffscreen -and $r.Top -ge $Edit.top-1 -and $r.Bottom -le $cancel.Top-17*$Scale+1
}
function Texture-Paper{@((Model).state.layers|Where-Object {$_.paper})[-1]}
function Texture-Control{@((Model).state.layer_properties.controls|Where-Object {$_.paper})[0]}
function Texture-Tiles{$control=Texture-Control;if($control){,@($control.paper.tiles)}else{,@()}}
function Texture-Open([string]$Id){
    Invoke $Id
    Wait-Until {$apply=Find 'texture-apply';$apply -and !$apply.Current.IsOffscreen -and (Find 'texture-tile-0')} "Choose Texture did not open from $Id"
}
function Texture-Click([string]$Id){
    $b=(Control $Id -Arranged).Current.BoundingRectangle
    [CapyRowPointer]::Down('mouse',[int]($b.X+$b.Width/2),[int]($b.Y+$b.Height/2));Start-Sleep -Milliseconds 35;[CapyRowPointer]::Up()
    Wait-Until {$apply=Find 'texture-apply';$apply -and !$apply.Current.IsOffscreen -and (Find 'texture-tile-0')} "Choose Texture did not open from a click on $Id"
}
function Texture-Closed([string]$Message){Wait-Until {!(Find 'texture-apply')} $Message}
function Texture-Pending{
    $grid=Find 'texture-grid' -Visible
    if(!$grid){return -1}
    $selected=[System.Windows.Automation.AndCondition]::new(
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Button),
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ItemStatusProperty,'Selected'))
    foreach($tile in $grid.FindAll([System.Windows.Automation.TreeScope]::Descendants,$selected)){
        if($tile.Current.AutomationId -match '^texture-tile-([0-9]+)$'){return [int]$Matches[1]}
    }
    -1
}
function Texture-Ready{
    Wait-Until {$tiles=Texture-Tiles;$tiles.Count -and !@(0..($tiles.Count-1)|Where-Object {$tiles[$_].id -and (Find "texture-preview-$_").Current.ItemStatus -ne 'Ready'}).Count} 'Texture previews did not load' 30
}
function Texture-Span([string]$Prefix,$Expected){
    $frame=Control "$Prefix-dialog-frame" -Arranged
    $null=Control "$Prefix-title" -Within $frame
    $bounds=$frame.Current.BoundingRectangle;$window=$root.Current.BoundingRectangle
    $apply=(Control "$Prefix-apply" -Within $frame -Arranged).Current.BoundingRectangle
    if($frame.Current.ClassName -eq 'Popup' -or !$bounds.Contains($apply) -or $bounds.Width -ge $window.Width -or $bounds.Height -ge $window.Height){throw 'Picker geometry requires the dialog frame inside its owned window'}
    $span=[ordered]@{top=$bounds.Top;bottom=$bounds.Bottom;right=$bounds.Right;height=$bounds.Height;button=$apply.Height;class=$frame.Current.ClassName}
    if($Expected -and ([Math]::Abs($span.height-$Expected.height) -gt 1 -or [Math]::Abs($span.right-$Expected.right) -gt 1)){throw "Choose Texture is not the size of Edit Color: $($span.height) vs $($Expected.height)"}
    $span
}
function Texture-Menu([int]$Index,[string]$Op){
    $tile=(Control "texture-tile-$Index" -Arranged).Current.BoundingRectangle
    [CapyRowPointer]::RightClick([int]($tile.X+$tile.Width/2),[int]($tile.Y+$tile.Width/3))
    Invoke "paper-$Op-$Index"
}
function Test-PaperTexture{
    Relationship-Layers
    $theme=(Model).state.theme;$paper=(Texture-Paper).id;$row=Relationship-Layer $paper
    if(!$row.paper -or !$row.paper.editable){throw 'The Paper row did not publish an editable texture'}
    $original=$row.paper.id
    Relationship-Reveal "layer-$paper-name"
    $texture=Control "layer-texture-$paper" -Arranged
    if($texture.Current.Name -ne 'Choose Texture' -or $texture.Current.HelpText -ne $row.paper.label){throw 'The texture thumbnail name or description differs from shared state'}
    Wait-Until {(Find "layer-texture-$paper-preview").Current.ItemStatus -eq 'Ready'} 'The row texture preview did not load' 30
    $scale=[CapyRowPointer]::GetDpiForWindow($drawingWindow)/96.
    $t=$texture.Current.BoundingRectangle;$c=(Control "layer-$paper-content" -Arranged).Current.BoundingRectangle;$n=(Control "layer-$paper-name" -Arranged).Current.BoundingRectangle
    if([Math]::Abs($t.Width/$scale-30) -gt .5 -or $t.Left -lt $c.Right -or $n.Left -lt $t.Right){throw 'The texture thumbnail is not a 30-DIP tile between the content thumbnail and the name'}
    Capture "texture-row-$theme" -WithModel
    Invoke "layer-$paper-content"
    Wait-Until {$apply=Find 'edit-color-apply';$apply -and !$apply.Current.IsOffscreen} 'Edit Color did not open from the Paper'
    $edit=Texture-Span 'edit-color'
    Invoke 'edit-color-cancel';Wait-Until {!(Find 'edit-color-apply')} 'Edit Color did not close'

    Texture-Click "layer-texture-$paper"
    Wait-Until {(Model).state.layer_tools.editing_layer.id -eq $paper -and (Texture-Control)} 'The texture thumbnail did not select the Paper'
    Texture-Ready;$revision=(Model).state.document_file.revision
    $tiles=Texture-Tiles;$current=[int](Texture-Control).value.value
    if((Texture-Pending) -ne $current){throw 'Choose Texture did not start on the current texture'}
    $span=Texture-Span 'texture' $edit
    $preview=(Control "texture-preview-$current" -Arranged).Current.BoundingRectangle
    [ordered]@{edit_color=$edit;texture=$span;preview=$preview;scale=$scale}|ConvertTo-Json|Set-Content (Join-Path $run "texture-dialog-geometry-$theme.json")
    if([Math]::Abs($span.button/$scale-34) -gt .5 -or [Math]::Abs($preview.Width/$scale-96) -gt .5){throw 'Footer buttons or texture previews are not at their native size'}
    Capture "texture-dialog-$theme" -WithModel
    Wait-Until {(Control "texture-tile-$current").Current.HasKeyboardFocus} 'The pending tile did not take keyboard focus'
    [CapyLayersCapture]::SetForegroundWindow($drawingWindow)|Out-Null
    [CapyRowPointer]::Key([uint32]$review.Id,$(if($current -eq 0){0x27}else{0x25}))
    $focused=$(if($current -eq 0){1}else{$current-1})
    Wait-Until {(Control "texture-tile-$focused").Current.HasKeyboardFocus} 'Arrow keys did not move between tiles'
    [CapyRowPointer]::Key([uint32]$review.Id,0x20)
    Wait-Until {(Texture-Pending) -eq $focused} 'Space did not choose the focused tile'
    if((Model).state.document_file.revision -ne $revision){throw 'Choosing a tile changed the drawing'}
    [CapyRowPointer]::Key([uint32]$review.Id,0x1B)
    Texture-Closed 'Escape did not close Choose Texture'
    if((Model).state.document_file.revision -ne $revision -or (Relationship-Layer $paper).paper.id -ne $original){throw 'Escape changed the texture'}

    if(!@((Model).layout.groups|Where-Object {$_.active -eq 'properties'}).Count){Invoke 'Properties' -Name}
    $button=Control 'property-texture' -Arranged
    Wait-Until {(Find 'property-texture-preview').Current.ItemStatus -eq 'Ready'} 'The Texture button preview did not load' 30
    if($button.Current.HelpText -ne $tiles[$current].label){throw 'The Texture button does not show the current texture'}
    Capture "texture-properties-$theme" -WithModel
    $pick=$(if($current -eq 0){1}else{0})
    Texture-Open 'property-texture'
    if((Texture-Pending) -ne $current){throw 'Choose Texture did not start on the current texture from Properties'}
    Invoke "texture-tile-$pick";Wait-Until {(Texture-Pending) -eq $pick} 'Clicking a tile did not move the pending texture'
    Invoke 'texture-cancel';Texture-Closed 'Cancel did not close Choose Texture'
    if((Model).state.document_file.revision -ne $revision -or (Relationship-Layer $paper).paper.id -ne $original){throw 'Cancel changed the texture'}
    Texture-Open 'property-texture'
    if((Texture-Pending) -ne $current){throw 'Reopening Choose Texture did not reset the pending texture'}
    Invoke "texture-tile-$pick";Wait-Until {(Texture-Pending) -eq $pick} 'Clicking a tile did not move the pending texture'
    Texture-Ready;Capture "texture-dialog-chosen-$theme" -WithModel
    Invoke 'texture-apply';Texture-Closed 'Use Texture did not close Choose Texture'
    Wait-Until {(Relationship-Layer $paper).paper.id -eq $tiles[$pick].id -and [int](Texture-Control).value.value -eq $pick} 'Use Texture did not apply the chosen texture'
    Wait-Until {(Control 'property-texture').Current.HelpText -eq $tiles[$pick].label} 'The Texture button did not follow the chosen texture'
    Capture "texture-applied-$theme" -WithModel
    Relationship-Undo
    Wait-Until {(Relationship-Layer $paper).paper.id -eq $original} 'One Undo did not restore the texture'

    Relationship-Layers
    Texture-Open "layer-texture-$paper"
    Invoke "texture-tile-$pick";Wait-Until {(Texture-Pending) -eq $pick} 'Clicking a tile did not move the pending texture'
    Invoke 'texture-apply';Texture-Closed 'Use Texture did not close Choose Texture from the row'
    Wait-Until {(Relationship-Layer $paper).paper.id -eq $tiles[$pick].id -and (Find "layer-texture-$paper").Current.HelpText -eq $tiles[$pick].label} 'Use Texture from the row did not apply the texture'
    Relationship-Undo
    Wait-Until {(Relationship-Layer $paper).paper.id -eq $original} 'One Undo did not restore the texture from the row'
    Texture-Open "layer-texture-$paper"
    Invoke "texture-tile-$pick";Wait-Until {(Texture-Pending) -eq $pick} 'Clicking a tile did not move the pending texture'
    Toggle-Flag 'layer-lock';Wait-Until {(Relationship-Layer $paper).locked} 'Locking under Choose Texture did not publish'
    Invoke 'texture-apply';Texture-Closed 'Use Texture did not close after the Paper was locked'
    Wait-Until {(Model).state.notice.text -eq 'Could not complete this action'} 'A refused Use Texture did not report the shared failure'
    if((Relationship-Layer $paper).paper.id -ne $original){throw 'A refused Use Texture changed the texture'}
    Capture "texture-refused-$theme" -WithModel
    Toggle-Flag 'layer-lock';Wait-Until {!(Relationship-Layer $paper).locked} 'Unlocking did not publish'

    $library=-1;$tiles=Texture-Tiles;for($i=0;$i -lt $tiles.Count;$i++){if($tiles[$i].id -eq $TextureLibraryId){$library=$i}}
    if($library -ge 0){
        Texture-Open "layer-texture-$paper"
        Invoke "texture-tile-$library";Wait-Until {(Texture-Pending) -eq $library} 'Clicking the library tile did not move the pending texture'
        Texture-Menu $library 'rename_paper'
        Wait-Until {!(Find 'texture-apply') -and (Find 'paper-rename-name')} 'Rename did not replace Choose Texture with the rename prompt'
        Capture "texture-rename-$theme"
        $name="Renamed $theme scan"
        (Control 'paper-rename-name').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($name)
        Invoke 'PrimaryButton'
        Wait-Until {$apply=Find 'texture-apply';$apply -and !$apply.Current.IsOffscreen -and (Find "texture-tile-$library").Current.Name -eq $name} 'Choose Texture did not return with the renamed paper'
        if((Texture-Pending) -ne $library){throw 'Renaming lost the pending texture'}
        Capture "texture-renamed-$theme" -WithModel
        Invoke 'texture-apply';Texture-Closed 'Use Texture did not close Choose Texture for the library paper'
        Wait-Until {(Relationship-Layer $paper).paper.id -eq $TextureLibraryId -and (Relationship-Layer $paper).paper.label -eq $name} 'Use Texture did not apply the library paper'
        Texture-Open "layer-texture-$paper"
        if((Texture-Pending) -ne $library){throw 'Choose Texture did not start on the library paper'}
        Wait-Until {Texture-Visible $library $edit $scale} 'Choose Texture did not scroll the pending library paper into view'
        Texture-Ready;$null=Texture-Span 'texture' $edit;Capture "texture-dialog-library-$theme" -WithModel
        Invoke 'texture-cancel';Texture-Closed 'Cancel did not close Choose Texture on the library paper'
        Relationship-Undo
        Wait-Until {(Relationship-Layer $paper).paper.id -eq $original} 'One Undo did not restore the texture after the library paper'
        Texture-Open "layer-texture-$paper"
        Invoke "texture-tile-$library";Wait-Until {(Texture-Pending) -eq $library} 'Clicking the library tile did not move the pending texture'
        if($theme -eq 'light'){
            Texture-Menu $library 'delete_paper'
            Wait-Until {!(Find "texture-tile-$library") -and (Texture-Pending) -eq $current} 'Deleting the pending paper did not fall back to the current texture'
        }
        Invoke 'texture-cancel';Texture-Closed 'Cancel did not close Choose Texture after the library edits'
        Relationship-Layers
        if((Relationship-Layer $paper).paper.id -ne $original){throw 'Library edits changed the texture'}
    }elseif($theme -ne 'light'){throw 'The seeded library paper is missing'}

    Invoke "layer-$paper-name";Wait-Until {(Model).state.layer_tools.editing_layer.id -eq $paper} 'Paper selection did not publish'
    Invoke 'layer-add-mask';Wait-Until {(Relationship-Layer $paper).has_mask} 'The Paper mask did not publish'
    $m=(Control "layer-$paper-mask" -Arranged).Current.BoundingRectangle;$t=(Control "layer-texture-$paper" -Arranged).Current.BoundingRectangle
    if($t.Left -lt $m.Right){throw 'The texture thumbnail is not right of the mask'}
    Capture "texture-row-mask-$theme" -WithModel
    Relationship-Undo;Wait-Until {!(Relationship-Layer $paper).has_mask} 'Undo did not remove the Paper mask'
    Toggle-Flag 'layer-lock';Wait-Until {(Relationship-Layer $paper).locked -and !(Relationship-Layer $paper).paper.editable} 'Locking did not publish a read-only texture'
    Invoke "layer-texture-$paper";Start-Sleep -Milliseconds 800
    if(Find 'texture-apply'){throw 'A locked Paper opened Choose Texture'}
    Toggle-Flag 'layer-lock';Wait-Until {(Relationship-Layer $paper).paper.editable} 'Unlocking did not restore the texture'
    if((Relationship-Layer $paper).paper.id -ne $original){throw 'The texture journey left the texture changed'}
}
function Paper-Geometry {
    $list=Control 'layer-list'
    $viewport=$list.Current.BoundingRectangle
    if($list.Current.IsOffscreen -or $viewport.IsEmpty -or $viewport.Width -le 0 -or $viewport.Height -le 0){$list=Find 'layer-list' -Visible}
    if(!$list){throw 'Paper geometry requires the current visible layer list'}
    $button=Control 'layer-add-paper' -Within $list -Arranged
    $last=@((Model).state.layers)[-1]
    $row=Control "layer-row-$($last.id)" -Within $list -Arranged
    $b=$button.Current.BoundingRectangle;$r=$row.Current.BoundingRectangle
    @{button=$b;last_row=$r;theme=(Model).state.theme}|ConvertTo-Json|Set-Content (Join-Path $run "paper-geometry-$((Model).state.theme).json")
    $scale=[CapyRowPointer]::GetDpiForWindow($drawingWindow)/96.
    if([Math]::Abs($b.Top-$r.Bottom) -gt 1 -or [Math]::Abs($b.Height/$scale-40) -gt 1 -or [Math]::Abs($b.Height-$r.Height) -gt 1){throw 'Add paper is not a 40-DIP row immediately after the last layer'}
    if($button.Current.Name -ne (Model).state.layer_tools.add_paper.label){throw 'Add paper caption differs from shared copy'}
}
function Paper-Activate([switch]$Keyboard,[string]$Device='mouse'){
    $count=(Model).state.layers.Count
    if($Keyboard){Focus 'layer-add-paper';[CapyRowPointer]::Key([uint32]$review.Id,0x20)}
    else{
        $b=(Control 'layer-add-paper' -Arranged).Current.BoundingRectangle
        @{device=$Device;bounds=$b;count=$count}|ConvertTo-Json|Set-Content (Join-Path $run "paper-activate-$Device-$((Model).state.theme).json")
        if($Device -eq 'mouse'){
            [CapyRowPointer]::Hover([int]($b.X+$b.Width/2),[int]($b.Y+$b.Height/2))
            Capture "paper-hover-$((Model).state.theme)" -Composed -WithModel
        }
        [CapyRowPointer]::Down($Device,[int]($b.X+$b.Width/2),[int]($b.Y+$b.Height/2));Start-Sleep -Milliseconds 35
        [CapyRowPointer]::Up()
    }
    Wait-Until {(Model).state.layers.Count -eq $count+1 -and $null -eq (Model).state.layer_tools.add_paper} 'Add paper did not insert the shared paper layer'
    Relationship-Layers
    Wait-Until {!(Find 'layer-add-paper' -Visible)} 'Add paper stayed visible with paper present'
    $bottom=@((Model).state.layers)[-1]
    if((Model).state.layer_tools.editing_layer.id -ne $bottom.id){throw 'Inserted paper did not become the selected bottom layer'}
}
function Test-LayerPaper([switch]$Light) {
    [CapyRowPointer]::Initialize([uint32]$review.Id)
    try{
    if($Light -and (Model).state.theme -ne 'light'){Relationship-Theme}
    foreach($theme in $(if($Light){@(2)}else{1..2})){
        Test-PaperTexture
        Relationship-Layers
        if((Model).state.layer_tools.add_paper -or (Find 'layer-add-paper' -Visible)){throw 'Add paper appears while initial paper exists'}
        $paper=@((Model).state.layers)[-1].id
        Relationship-Reveal "layer-$paper-name";Invoke "layer-$paper-name"
        Wait-Until {(Model).state.layer_tools.editing_layer.id -eq $paper} 'Paper row selection did not publish'
        $count=(Model).state.layers.Count;Invoke 'layer-delete'
        Wait-Until {(Model).state.layers.Count -eq $count-1 -and $null -ne (Model).state.layer_tools.add_paper} 'Deleting paper did not expose Add paper'
        Paper-Geometry
        if(!$Light){
        foreach($device in @('mouse','pen','touch')){
            $before=(Model).state.document_file.revision
            $b=(Control 'layer-add-paper' -Arranged).Current.BoundingRectangle
            [CapyRowPointer]::Down($device,[int]($b.X+$b.Width/2),[int]($b.Y+$b.Height/2));Start-Sleep -Milliseconds 35;[CapyRowPointer]::Move([int]($b.X-20),[int]($b.Y-20));Start-Sleep -Milliseconds 35;[CapyRowPointer]::Up()
            Capture "paper-cancel-$device-$((Model).state.theme)" -WithModel
            if((Model).state.document_file.revision -ne $before){throw "Canceled $device Add paper press changed the drawing"}
            Paper-Activate -Device $device
            Relationship-Undo
            Wait-Until {$null -ne (Model).state.layer_tools.add_paper} 'Undo did not restore Add paper'
            Relationship-Layers
            Paper-Geometry
        }
        }
        $inserted=@(foreach($i in 1..64){Relationship-New})
        $rows=$root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
        $realized=@($rows|Where-Object {$_.Current.AutomationId -match '^layer-row-[0-9]+$'}).Count
        if($realized -ge 40){throw 'Add paper list wrapper disabled layer virtualization'}
        $realized|Set-Content (Join-Path $run "paper-realized-$((Model).state.theme).txt")
        $scroll=(Control 'layer-list').GetCurrentPattern([System.Windows.Automation.ScrollPattern]::Pattern)
        Wait-Until {$scroll.Current.VerticallyScrollable} 'Additional layers did not make the list scrollable'
        $scroll.SetScrollPercent(-1,0)
        Wait-Until {!(Find 'layer-add-paper' -Visible)} 'Add paper did not scroll away with the layers'
        $scroll.SetScrollPercent(-1,100)
        Wait-Until {$null -ne (Find 'layer-add-paper' -Visible)} 'Scrolling to the bottom did not reveal Add paper'
        Paper-Geometry
        Paper-Activate -Keyboard
        Relationship-Undo
        Invoke 'Redo' -Name
        Wait-Until {$null -eq (Model).state.layer_tools.add_paper} 'Redo did not restore paper'
        Relationship-Layers
        Capture "paper-$((Model).state.theme)" -Composed -WithModel
        Relationship-Undo
        $first=$inserted[-1];$last=$inserted[0]
        Relationship-Layers
        $list=Control 'layer-list' -Arranged
        $scroll=$list.GetCurrentPattern([System.Windows.Automation.ScrollPattern]::Pattern)
        $scroll.SetScrollPercent(-1,0)
        $null=Control "layer-$first-name" -Within $list -Arranged;Invoke "layer-$first-name"
        Wait-Until {(Model).state.layer_tools.editing_layer.id -eq $first} 'Cleanup range anchor did not select'
        $list=Control 'layer-list' -Arranged
        $scroll=$list.GetCurrentPattern([System.Windows.Automation.ScrollPattern]::Pattern)
        $scroll.SetScrollPercent(-1,100)
        $endpoint=Control "layer-$last-name" -Within $list -Arranged
        $viewport=$list.Current.BoundingRectangle
        if($endpoint.Current.BoundingRectangle.Top -lt $viewport.Top -or $endpoint.Current.BoundingRectangle.Bottom -gt $viewport.Bottom){throw 'Cleanup range endpoint is outside the list viewport'}
        $bounds=$endpoint.Current.BoundingRectangle
        [CapyRowPointer]::Hover([int]($bounds.X+$bounds.Width/2),[int]($bounds.Y+$bounds.Height/2))
        [CapyRowPointer]::Hold(0x10,$true)
        try{
            Invoke "layer-$last-name"
            Wait-Until {@((Model).state.layers|Where-Object selected).Count -eq $inserted.Count} 'Cleanup did not select the inserted range'
        }finally{[CapyRowPointer]::Hold(0x10,$false)}
        Invoke 'layer-delete'
        Wait-Until {(Model).state.layers.Count -eq $count-1 -and !@((Model).state.layers|Where-Object {$_.id -in $inserted}).Count} 'Cleanup did not remove exactly the inserted rows'
        Paper-Activate -Keyboard
        if($theme -eq 1){Relationship-Theme}
    }
    while((Model).state.layers.Count){
        $count=(Model).state.layers.Count
        $id=@((Model).state.layers)[0].id
        Relationship-Reveal "layer-$id-name";Invoke "layer-$id-name"
        Wait-Until {(Model).state.layer_tools.editing_layer.id -eq $id} 'Layer row selection did not publish'
        Invoke 'layer-delete'
        Wait-Until {(Model).state.layers.Count -eq $count-1} 'Deleting the remaining row did not publish'
    }
    Wait-Until {$null -ne (Find 'layer-add-paper' -Visible)} 'Empty Layers omitted Add paper'
    Capture 'paper-empty' -Composed -WithModel
    Paper-Activate
    }finally{[CapyRowPointer]::Dispose()}
}
