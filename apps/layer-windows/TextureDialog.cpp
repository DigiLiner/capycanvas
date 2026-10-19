#include "pch.h"
#include "TextureDialog.h"
#include "PickerDialog.h"
#include "BitmapUpload.h"
#include "WorkspaceGeometry.h"
#include "NativeMenus.h"
#include <deque>
#include <set>

namespace CapyUi {
struct PaperPreviewCache : std::enable_shared_from_this<PaperPreviewCache> {
    struct Entry {std::shared_ptr<BitmapUpload> upload;bool ready=false;};
    struct Shown {ImageSource source{nullptr};bool pending=false;};
    PreviewTransport transport;
    Microsoft::UI::Dispatching::DispatcherQueue dispatcher{Microsoft::UI::Dispatching::DispatcherQueue::GetForCurrentThread()};
    Microsoft::UI::Dispatching::DispatcherQueueTimer retry{nullptr};
    std::map<hstring,Entry> entries;
    std::deque<hstring> queue;
    std::set<hstring> missing;
    std::vector<std::weak_ptr<PaperThumbnail>> viewers;
    size_t viewerCleanup=64;
    hstring library;
    bool busy=false;
    ~PaperPreviewCache(){if(retry)retry.Stop();for(auto& [id,entry]:entries)entry.upload->Close();}
    void watch(std::weak_ptr<PaperThumbnail> viewer){
        if(viewers.size()>=viewerCleanup){std::erase_if(viewers,[](auto const& item){return item.expired();});viewerCleanup=std::max(size_t(64),viewers.size()*2);}
        viewers.push_back(std::move(viewer));
    }
    Shown find(hstring const& id,hstring const& generation){
        if(generation!=library){library=generation;missing.clear();}
        if(auto found=entries.find(id);found!=entries.end())return {found->second.ready?found->second.upload->Image():nullptr,!found->second.ready};
        if(missing.contains(id))return {};
        entries.emplace(id,Entry{BitmapUpload::Create()});queue.push_back(id);send();
        return {nullptr,true};
    }
    void send(){
        if(busy||queue.empty()||!transport)return;
        std::vector<hstring> batch;A ids;
        while(!queue.empty()&&batch.size()<8){batch.push_back(queue.front());ids.Append(S(queue.front()));queue.pop_front();}
        busy=true;auto weak=weak_from_this();auto target=dispatcher;
        if(transport(CanvasQueryKind::Papers,to_string(O({{L"ids",ids}}).Stringify()),[weak,target,batch](PreviewPacket packet){
            target.TryEnqueue([weak,packet=std::move(packet),batch]{if(auto self=weak.lock())self->receive(packet,batch);});
        }))return;
        busy=false;queue.insert(queue.begin(),batch.begin(),batch.end());
        if(!retry){
            retry=dispatcher.CreateTimer();retry.IsRepeating(false);retry.Interval(std::chrono::milliseconds(120));
            retry.Tick([weak](auto&&,auto&&){if(auto self=weak.lock())self->send();});
        }
        retry.Start();
    }
    void receive(PreviewPacket const& packet,std::vector<hstring> const& batch){
        busy=false;std::set<hstring> delivered;
        try{
            auto status=packet?J::Parse(to_hstring(capy_preview_metadata(packet.get()))):J{};
            auto images=array(status,L"images");int size=int(num(status,L"size"));
            if(size>0&&size<=512&&images.Size()<=batch.size()){
                size_t bytes=size_t(size)*size_t(size)*4*images.Size();auto weak=weak_from_this();
                for(auto value:images){
                    auto image=value.GetObject();auto id=str(image,L"id");auto found=entries.find(id);
                    if(found==entries.end()||found->second.ready||std::find(batch.begin(),batch.end(),id)==batch.end()||!delivered.insert(id).second)continue;
                    std::weak_ptr<BitmapUpload> expected=found->second.upload;
                    found->second.upload->Submit({BitmapUpload::ReadPacket(packet),size_t(num(image,L"offset")),bytes,size,size,false,true,
                        [weak,id,expected]{auto self=weak.lock();if(!self)return false;auto found=self->entries.find(id);return found!=self->entries.end()&&found->second.upload==expected.lock();},
                        [weak,id,expected](bool accepted){if(auto self=weak.lock())self->loaded(id,expected,accepted);}});
                }
            }
        }catch(hresult_error const&){}
        bool forgotten=false;
        for(auto const& id:batch)if(!delivered.contains(id)){forget(id);forgotten=true;}
        if(forgotten)notify();
        send();
    }
    void loaded(hstring const& id,std::weak_ptr<BitmapUpload> const& expected,bool accepted){
        auto found=entries.find(id);if(found==entries.end()||found->second.upload!=expected.lock())return;
        if(accepted)found->second.ready=true;else forget(id);
        notify();
    }
    void forget(hstring const& id){
        auto found=entries.find(id);if(found==entries.end())return;
        found->second.upload->Close();entries.erase(found);missing.insert(id);
    }
    void notify(){
        auto current=viewers;
        for(auto const& item:current)if(auto viewer=item.lock())viewer->refresh();
        std::erase_if(viewers,[](auto const& item){return item.expired();});
    }
};
namespace {
std::shared_ptr<PaperPreviewCache> const& paperPreviews(std::shared_ptr<WorkspaceData> const& data){
    if(!data->papers){data->papers=std::make_shared<PaperPreviewCache>();data->papers->transport=data->query;}
    return data->papers;
}
PathGeometry squircleShape(double size,double inset=0){
    auto radius=float(size/2);auto geometry=squircleRectangle(float(size),float(size),{radius,radius,radius,radius});
    if(inset){TranslateTransform offset;offset.X(inset);offset.Y(inset);geometry.Transform(offset);}
    return geometry;
}
Shapes::Path squircle(double size){
    Shapes::Path shape;shape.Width(size);shape.Height(size);shape.Data(squircleShape(size));shape.IsHitTestVisible(false);return shape;
}
}
std::shared_ptr<PaperThumbnail> PaperThumbnail::Create(std::shared_ptr<WorkspaceData> const& data,double size,double glyph,bool well){
    auto result=std::make_shared<PaperThumbnail>();result->data=data;result->glyphSize=glyph;
    auto& root=result->root;root.Width(size);root.Height(size);root.IsHitTestVisible(false);root.IsTabStop(false);
    root.HorizontalAlignment(HorizontalAlignment::Center);root.VerticalAlignment(VerticalAlignment::Center);
    result->face.Width(size);result->face.Height(size);root.Content(result->face);
    result->well=squircle(size);result->picture=squircle(size);
    result->well.Fill(well?data->tint(L"text",15):clear());
    result->pixels.Stretch(Stretch::Fill);result->picture.Fill(result->pixels);
    result->glyph.HorizontalAlignment(HorizontalAlignment::Center);result->glyph.VerticalAlignment(VerticalAlignment::Center);
    for(UIElement part:{UIElement(result->well),UIElement(result->picture),UIElement(result->glyph)})result->face.Children().Append(part);
    paperPreviews(data)->watch(result);
    return result;
}
void PaperThumbnail::show(V const& id,hstring const& label){shown=id;AutomationProperties::SetName(root,label);refresh();}
void PaperThumbnail::refresh(){
    PaperPreviewCache::Shown image;
    if(shown&&shown.ValueType()==JsonValueType::String)
        image=paperPreviews(data)->find(shown.GetString(),str(object(data->model,L"windows_papers"),L"generation"));
    if(pixels.ImageSource()!=image.source)pixels.ImageSource(image.source);
    picture.Visibility(image.source?Visibility::Visible:Visibility::Collapsed);
    glyph.Visibility(image.source?Visibility::Collapsed:Visibility::Visible);
    if(!image.source&&theme!=data->theme()){theme=data->theme();setIcon(glyph,L"layer-paper-symbolic",theme,glyphSize);}
    AutomationProperties::SetItemStatus(root,image.source?data->caption(L"header",L"ready"):image.pending?data->caption(L"layers",L"pending"):hstring());
}

namespace {
constexpr double TileFrame=102,TilePreview=96,TileColumnGap=12,TileRowGap=14;
struct TexturePicker {J paper;uint32_t selected=0;hstring key;bool enabled=false;};
std::optional<TexturePicker> texturePicker(J const& state,double layer,double epoch){
    if(num(object(state,L"document_file"),L"epoch")!=epoch)return std::nullopt;
    auto view=object(state,L"layer_properties");auto shown=view.GetNamedValue(L"layer",JsonValue::CreateNullValue());
    if(shown.ValueType()!=JsonValueType::Number||shown.GetNumber()!=layer)return std::nullopt;
    for(auto value:array(view,L"controls")){
        auto control=value.GetObject();auto paper=object(control,L"paper");
        if(str(object(control,L"kind"),L"kind")==L"choice"&&paper.Size())
            return TexturePicker{paper,uint32_t(num(object(control,L"value"),L"value")),str(control,L"key"),flag(view,L"enabled")};
    }
    return std::nullopt;
}
hstring tileId(J const& tile){return tile.GetNamedValue(L"id",JsonValue::CreateNullValue()).Stringify();}

struct TextureDialog;
std::vector<std::shared_ptr<TextureDialog>>& openDialogs(){thread_local std::vector<std::shared_ptr<TextureDialog>> dialogs;return dialogs;}

struct TextureDialog : std::enable_shared_from_this<TextureDialog> {
    struct Tile {Button node{nullptr};std::shared_ptr<PaperThumbnail> picture;Shapes::Path hover{nullptr},ring{nullptr};hstring id;};
    std::shared_ptr<WorkspaceData> data;
    double layer=0,epoch=0;
    ContentDialog dialog{nullptr};
    Grid root,footer,grid;
    StackPanel page;
    ScrollViewer scroll;
    TextBlock title;
    Button import{nullptr},cancel{nullptr},apply{nullptr};
    std::vector<Tile> tiles;
    J paper;
    hstring shown,pending;
    std::optional<J> renaming,importing;
    int columns=0;
    uint64_t listener=0;
    bool chosen=false,accepting=false,finished=false,focusPending=false;
    XamlRoot::Changed_revoker resized;

    void init(XamlRoot const& xaml){
        auto weak=weak_from_this();
        dialog=pickerDialog(data,xaml,root,scroll,footer,L"texture-dialog");
        pickerTitle(data,title,L"texture-title");
        grid.ColumnSpacing(TileColumnGap);grid.RowSpacing(TileRowGap);grid.XYFocusKeyboardNavigation(XYFocusKeyboardNavigationMode::Enabled);
        AutomationProperties::SetAutomationId(grid,L"texture-grid");
        grid.SizeChanged([weak](auto&&,auto&&){if(auto self=weak.lock())self->arrange();});
        page.Padding({PickerPadding,PickerPadding,PickerPadding,PickerPadding});page.Spacing(14);
        page.Children().Append(title);page.Children().Append(grid);
        scroll.Content(page);scroll.HorizontalScrollBarVisibility(ScrollBarVisibility::Disabled);scroll.VerticalScrollBarVisibility(ScrollBarVisibility::Auto);mouseScrollbars(scroll);
        pickerFooter(data,footer,{GridUnitType::Auto,GridUnitType::Star,GridUnitType::Auto,GridUnitType::Auto});
        import=pickerButton(data,L"texture-import",0,false,[weak]{if(auto self=weak.lock()){self->importing=object(self->paper,L"import");self->close(false);}});
        cancel=pickerButton(data,L"texture-cancel",2,false,[weak]{if(auto self=weak.lock())self->close(false);});
        apply=pickerButton(data,L"texture-apply",3,true,[weak]{if(auto self=weak.lock())self->close(true);});
        import.Visibility(Visibility::Collapsed);
        for(auto const& node:{import,cancel,apply})footer.Children().Append(node);
        root.Height(PickerDialogHeight);
        root.KeyDown([weak](auto&&,KeyRoutedEventArgs const& e){if(auto self=weak.lock();self&&e.Key()==Windows::System::VirtualKey::Enter){e.Handled(true);self->close(true);}});
        resized=xaml.Changed(auto_revoke,[weak](auto&&,auto&&){if(auto self=weak.lock())fitPicker(self->root);});
        dialog.Opened([weak](auto&&,auto&&){if(auto self=weak.lock()){fitPicker(self->root);self->focusPending=true;self->focus();}});
        dialog.Closed([weak](auto&&,auto&&){if(auto self=weak.lock()){
            self->data->popup(false);
            if(auto action=std::exchange(self->renaming,std::nullopt))self->rename(*action);else self->finish();
        }});
        listener=data->colorView([weak]{if(auto self=weak.lock())self->update();});
        data->copyView([weak]{if(auto self=weak.lock();self&&!self->finished){self->relabel();return true;}return false;});
        relabel();fitPicker(root);update();
    }
    bool current()const{return num(object(data->state,L"document_file"),L"epoch")==epoch;}
    void relabel(){
        auto text=data->caption(L"paper",L"choose");
        title.Text(text);AutomationProperties::SetName(dialog,text);AutomationProperties::SetName(root,text);AutomationProperties::SetName(grid,text);
        pickerLabel(cancel,data->common(L"cancel"));pickerLabel(apply,data->caption(L"paper",L"use_texture"));
        if(auto action=object(paper,L"import");action.Size())pickerLabel(import,str(action,L"label"));
    }
    void update(){
        if(finished)return;
        auto picker=texturePicker(data->state,layer,epoch);if(!picker)return;
        if(auto key=picker->paper.Stringify();key!=shown){
            shown=key;paper=picker->paper;rebuild();
            bool kept=chosen&&std::any_of(tiles.begin(),tiles.end(),[this](Tile const& tile){return tile.id==pending;});
            if(!kept&&picker->selected<tiles.size())pending=tiles[picker->selected].id;
            chosen=true;
            auto action=object(paper,L"import");import.Visibility(action.Size()?Visibility::Visible:Visibility::Collapsed);relabel();
        }
        mark();focus();
    }
    void rebuild(){
        auto weak=weak_from_this();
        grid.Children().Clear();tiles.clear();
        auto list=array(paper,L"tiles");
        for(uint32_t index=0;index<list.Size();++index){
            auto item=list.GetObjectAt(index);auto text=str(item,L"label");
            Tile tile;tile.id=tileId(item);
            tile.node=button(data,text,[weak,id=tile.id]{if(auto self=weak.lock()){self->pending=id;self->mark();}});
            tile.node.HorizontalAlignment(HorizontalAlignment::Stretch);tile.node.VerticalAlignment(VerticalAlignment::Top);
            tile.node.HorizontalContentAlignment(HorizontalAlignment::Stretch);tile.node.Padding({0,0,0,0});
            for(auto state:{L"ButtonBackgroundPointerOver",L"ButtonBackgroundPressed"})tile.node.Resources().Insert(box_value(hstring(state)),clear());
            Grid frame;frame.Width(TileFrame);frame.Height(TileFrame);frame.HorizontalAlignment(HorizontalAlignment::Center);
            tile.hover=squircle(TileFrame);tile.hover.Fill(clear());
            tile.picture=PaperThumbnail::Create(data,TilePreview,48,true);tile.picture->show(item.GetNamedValue(L"id",JsonValue::CreateNullValue()),text);
            tile.ring=squircle(TileFrame);
            GeometryGroup edge;edge.FillRule(FillRule::EvenOdd);edge.Children().Append(squircleShape(TileFrame));edge.Children().Append(squircleShape(TileFrame-4,2));
            tile.ring.Data(edge);tile.ring.Fill(accent(data));tile.ring.Opacity(0);
            for(UIElement part:{UIElement(tile.hover),UIElement(tile.picture->root),UIElement(tile.ring)})frame.Children().Append(part);
            AutomationProperties::SetAutomationId(tile.picture->root,L"texture-preview-"+to_hstring(index));
            auto caption=label(data,text,false,false);caption.FontSize(data->textSize()*.82);caption.LineHeight(0);caption.LineStackingStrategy(LineStackingStrategy::MaxHeight);
            caption.FontWeight(winrt::Windows::UI::Text::FontWeights::Normal());caption.TextAlignment(TextAlignment::Center);caption.HorizontalAlignment(HorizontalAlignment::Stretch);
            caption.TextWrapping(TextWrapping::Wrap);caption.TextTrimming(TextTrimming::CharacterEllipsis);caption.MaxLines(2);
            StackPanel content;content.Spacing(6);content.Children().Append(frame);content.Children().Append(caption);tile.node.Content(content);
            AutomationProperties::SetAutomationId(tile.node,L"texture-tile-"+to_hstring(index));tooltip(tile.node,text);
            auto hover=tile.hover;
            tile.node.PointerEntered([hover,data=data](auto&&,auto&&){hover.Fill(data->tint(L"text",26));});
            auto leave=[hover](auto&&,auto&&){hover.Fill(clear());};
            tile.node.PointerExited(leave);tile.node.PointerCanceled(leave);tile.node.PointerCaptureLost(leave);
            if(auto actions=array(item,L"actions");actions.Size()){
                MenuFlyout menu;TrackPopup(menu,data);AutomationProperties::SetAutomationId(menu,L"paper-tile-menu");
                for(auto value:actions){
                    auto spec=value.GetObject();auto action=object(spec,L"action");MenuFlyoutItem command;command.Text(str(spec,L"label"));
                    AutomationProperties::SetAutomationId(command,L"paper-"+str(action,L"op")+L"-"+to_hstring(index));
                    command.Click([weak,action](auto&&,auto&&){if(auto self=weak.lock())self->act(action);});menu.Items().Append(command);
                }
                tile.node.ContextFlyout(menu);
            }
            grid.Children().Append(tile.node);tiles.push_back(std::move(tile));
        }
        columns=0;arrange();
    }
    void arrange(){
        double width=grid.ActualWidth();if(width<=0||tiles.empty())return;
        int next=std::clamp(int((width+TileColumnGap)/(TileFrame+TileColumnGap)),1,6);
        if(next!=columns){
            columns=next;grid.ColumnDefinitions().Clear();
            for(int i=0;i<columns;++i){ColumnDefinition column;column.Width({1,GridUnitType::Star});grid.ColumnDefinitions().Append(column);}
        }
        auto rows=int((tiles.size()+columns-1)/columns);
        while(int(grid.RowDefinitions().Size())<rows){RowDefinition row;row.Height({1,GridUnitType::Auto});grid.RowDefinitions().Append(row);}
        while(int(grid.RowDefinitions().Size())>rows)grid.RowDefinitions().RemoveAtEnd();
        for(size_t i=0;i<tiles.size();++i){Grid::SetColumn(tiles[i].node,int(i)%columns);Grid::SetRow(tiles[i].node,int(i)/columns);}
    }
    void mark(){
        for(auto const& tile:tiles){
            bool on=tile.id==pending;tile.ring.Opacity(on?1:0);
            AutomationProperties::SetItemStatus(tile.node,on?data->caption(L"search",L"selected"):hstring());
        }
    }
    void focus(){
        if(!focusPending)return;
        for(auto const& tile:tiles)if(tile.id==pending){
            if(!tile.node.Focus(FocusState::Programmatic))return;
            focusPending=false;BringIntoViewOptions options;options.AnimationDesired(false);options.VerticalAlignmentRatio(.5);tile.node.StartBringIntoView(options);return;
        }
    }
    void act(J action){
        if(finished||!current())return;
        if(str(action,L"op")==L"rename_paper"){renaming=action;dialog.Hide();return;}
        data->dispatchDocument(O({{L"type",S(L"effect")},{L"action",action}}),to_hstring(uint64_t(epoch)));
    }
    fire_and_forget rename(J action){
        auto lifetime=shared_from_this();
        ContentDialog prompt;prompt.XamlRoot(dialog.XamlRoot());prompt.Title(box_value(data->caption(L"paper",L"rename_title")));
        prompt.PrimaryButtonText(data->common(L"save"));prompt.CloseButtonText(data->common(L"cancel"));prompt.DefaultButton(ContentDialogButton::Primary);
        prompt.RequestedTheme(data->theme()==L"light"?ElementTheme::Light:ElementTheme::Dark);
        AutomationProperties::SetAutomationId(prompt,L"paper-rename-dialog");
        TextBox input;input.Header(box_value(data->common(L"name")));input.Text(str(action,L"name"));input.MaxLength(64);AutomationProperties::SetAutomationId(input,L"paper-rename-name");
        auto validate=[source=data,prompt=make_weak(prompt),input=make_weak(input)]{auto view=prompt.get();auto entry=input.get();if(view&&entry)view.IsPrimaryButtonEnabled(source->caption(O({{L"type",S(L"paper_name_error")},{L"name",S(entry.Text())}})).empty());};
        input.TextChanged([validate](auto&&,auto&&){validate();});validate();prompt.Content(input);
        input.Loaded([weak=make_weak(input)](auto&&,auto&&){if(auto input=weak.get()){input.Focus(FocusState::Programmatic);input.SelectAll();}});
        data->popup(true);bool saved=false;
        try{saved=co_await prompt.ShowAsync()==ContentDialogResult::Primary;}catch(hresult_error const&){}
        data->popup(false);
        if(saved&&texturePicker(data->state,layer,epoch)){
            action.Insert(L"name",S(input.Text()));data->dispatchDocument(O({{L"type",S(L"effect")},{L"action",action}}),to_hstring(uint64_t(epoch)));
        }
        present();
    }
    void present(){
        if(finished)return;
        data->popup(true);focusPending=true;
        try{dialog.ShowAsync();}catch(hresult_error const&){data->popup(false);finish();}
    }
    void close(bool accept){accepting=accept;dialog.Hide();}
    void finish(){
        if(std::exchange(finished,true))return;
        if(listener)data->colorViews.erase(listener);
        resized.revoke();
        auto picker=texturePicker(data->state,layer,epoch);
        auto epochText=to_hstring(uint64_t(epoch));
        if(importing&&importing->Size()&&picker)data->dispatchDocument(O({{L"type",S(L"effect")},{L"action",object(*importing,L"action")}}),epochText);
        else if(accepting&&(!picker||!picker->enabled))data->dispatch(O({{L"windows_failure",S(L"action_failed")}}));
        else if(accepting){
            auto list=array(picker->paper,L"tiles");
            for(uint32_t index=0;index<list.Size();++index){
                if(tileId(list.GetObjectAt(index))!=pending)continue;
                if(index!=picker->selected)data->dispatchDocument(O({{L"type",S(L"effect")},{L"action",O({{L"op",S(L"set")},{L"layer",N(layer)},{L"key",S(picker->key)},
                    {L"value",O({{L"kind",S(L"choice")},{L"value",N(index)}})}})}}),epochText);
                break;
            }
        }
        auto& open=openDialogs();auto self=shared_from_this();
        open.erase(std::remove(open.begin(),open.end(),self),open.end());
    }
};
}

void ChooseTexture(std::shared_ptr<WorkspaceData> const& data,UIElement const& owner,double layer){
    auto xaml=owner.XamlRoot();if(!xaml||!openDialogs().empty())return;
    auto dialog=std::make_shared<TextureDialog>();dialog->data=data;dialog->layer=layer;
    dialog->epoch=num(object(data->state,L"document_file"),L"epoch");
    dialog->init(xaml);openDialogs().push_back(dialog);dialog->present();
}
}
