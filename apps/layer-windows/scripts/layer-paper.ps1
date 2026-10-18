function Paper-Geometry {
    $button=Control 'layer-add-paper' -Arranged
    $last=@((Model).state.layers)[-1]
    $row=Control "layer-row-$($last.id)" -Arranged
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
        $scroll=(Control 'layer-list').GetCurrentPattern([System.Windows.Automation.ScrollPattern]::Pattern)
        $scroll.SetScrollPercent(-1,0)
        $null=Control "layer-$first-name" -Arranged;Invoke "layer-$first-name"
        Wait-Until {(Model).state.layer_tools.editing_layer.id -eq $first} 'Cleanup range anchor did not select'
        $scroll=(Control 'layer-list').GetCurrentPattern([System.Windows.Automation.ScrollPattern]::Pattern)
        $scroll.SetScrollPercent(-1,100)
        $endpoint=Control "layer-$last-name" -Arranged
        $viewport=(Control 'layer-list').Current.BoundingRectangle
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
