#include "pch.h"
#include "EffectControls.h"
#include "NativeMenus.h"
#include "ScopesView.h"
#include "BitmapUpload.h"
#include <winrt/Microsoft.UI.Xaml.Shapes.h>
using namespace CapyEffects;
namespace {
Windows::UI::Color rgba(A const& a){
    if(a.Size()!=4)return {};
    auto byte=[&](int i){return uint8_t(std::round(std::clamp(a.GetNumberAt(i),0.,1.)*255));};
    return {byte(3),byte(0),byte(1),byte(2)};
}
struct ColorButton : std::enable_shared_from_this<ColorButton> {
    std::shared_ptr<WorkspaceData> data;
    hstring id;
    std::function<J()> get;
    std::function<void(J)> set;
    std::function<hstring()> context,currentTitle;
    std::function<bool()> opaque;
    bool compact=false;
    TextBlock name{nullptr};Button pick{nullptr};Grid root;
    SolidColorBrush sample{Windows::UI::Color{}};
    hstring shown;
    void init(hstring const& title){
        auto weak=weak_from_this();
        pick=button(data,title,[weak]{if(auto self=weak.lock())self->edit();});
        pick.Background(data->brush(L"input"));
        Shapes::Rectangle color;color.Fill(sample);color.Margin({2,2,2,2});pick.Content(color);
        pick.HorizontalContentAlignment(HorizontalAlignment::Stretch);pick.VerticalContentAlignment(VerticalAlignment::Stretch);
        AutomationProperties::SetAutomationId(pick,id+L"-color");
        if(compact){pick.Width(36);pick.Height(data->controlHeight());return;}
        ColumnDefinition text;text.Width({1,GridUnitType::Star});root.ColumnDefinitions().Append(text);
        ColumnDefinition swatch;swatch.Width({56,GridUnitType::Pixel});root.ColumnDefinitions().Append(swatch);
        name=label(data,title);name.VerticalAlignment(VerticalAlignment::Center);root.Children().Append(name);
        pick.Height(data->controlHeight());pick.HorizontalAlignment(HorizontalAlignment::Stretch);
        Grid::SetColumn(pick,1);root.Children().Append(pick);
    }
    void edit(){
        auto original=get();auto expected=context?context():hstring{};
        EditColor(data,pick,O({{L"color",original}}),opaque(),[weak=weak_from_this(),original,expected](J color,std::optional<double>){
            auto self=weak.lock();if(!self||!self->pick.XamlRoot())return;
            if((self->context&&self->context()!=expected)||self->get().Stringify()!=original.Stringify())return;
            self->set(color);
        });
    }
    void refresh(){
        auto title=currentTitle();if(name)name.Text(title);AutomationProperties::SetName(pick,title);tooltip(pick,title);
        auto panel=object(data->model,L"color_panel");A colors;colors.Append(get());
        auto request=O({{L"type",S(L"preview")},{L"colors",colors},{L"document_space",S(str(panel,L"rgb_space",L"Srgb"))},
            {L"display_space",S(L"Srgb")},{L"rendition",panel.GetNamedValue(L"rendition",JsonValue::CreateNullValue())}});
        auto key=request.Stringify();if(key==shown)return;shown=key;
        try{sample.Color(displayColor(colorUi(data->localization.get(),request).GetArray().GetObjectAt(0)));}catch(hresult_error const&){}
    }
};
hstring actionId(J const& action){
    auto role=str(action,L"role");
    return L"property-action-"+str(action,L"op")+(role.empty()?hstring():L"-"+role);
}
struct PaperPicker : std::enable_shared_from_this<PaperPicker> {
    std::shared_ptr<Property> property;
    Button pick{nullptr};Image thumbnail;TextBlock name;
    Flyout flyout;Grid grid;ScrollViewer scroll;
    Microsoft::UI::Dispatching::DispatcherQueue dispatcher{Microsoft::UI::Dispatching::DispatcherQueue::GetForCurrentThread()};
    struct Tile {Button control;Image image;std::shared_ptr<BitmapUpload> upload;bool loaded=false,pending=false;};
    std::vector<Tile> tiles;hstring schema,resources;bool busy=false,open=false,rebuilding=false;
    explicit PaperPicker(std::shared_ptr<Property> p):property(std::move(p)){}
    ~PaperPicker(){if(open){flyout.Hide();data()->popup(false);}for(auto& tile:tiles)tile.upload->Close();}
    std::shared_ptr<WorkspaceData> data()const{return property->data;}
    hstring key()const{return str(object(data()->model,L"windows_papers"),L"key");}
    void init(){
        auto weak=weak_from_this();
        pick=button(data(),property->label(),[]{});pick.HorizontalAlignment(HorizontalAlignment::Stretch);pick.HorizontalContentAlignment(HorizontalAlignment::Stretch);
        pick.MinHeight(data()->controlHeight());pick.Padding({6,3,6,3});pick.Background(data()->brush(L"input"));
        AutomationProperties::SetAutomationId(pick,property->id());
        Grid row;row.ColumnSpacing(data()->panelMetric(L"control_spacing"));ColumnDefinition picture;picture.Width({1,GridUnitType::Auto});row.ColumnDefinitions().Append(picture);ColumnDefinition title;title.Width({1,GridUnitType::Star});row.ColumnDefinitions().Append(title);
        thumbnail.Width(28);thumbnail.Height(28);thumbnail.Stretch(Stretch::UniformToFill);Grid swatch;swatch.Children().Append(icon(L"layer-fill-symbolic",data()->theme(),28));swatch.Children().Append(thumbnail);row.Children().Append(swatch);
        name=label(data(),L"",false,false);name.VerticalAlignment(VerticalAlignment::Center);name.TextTrimming(TextTrimming::CharacterEllipsis);Grid::SetColumn(name,1);row.Children().Append(name);pick.Content(row);
        grid.ColumnSpacing(6);grid.RowSpacing(6);for(int i=0;i<4;++i){ColumnDefinition column;column.Width({84,GridUnitType::Pixel});grid.ColumnDefinitions().Append(column);}
        AutomationProperties::SetAutomationId(grid,L"paper-picker");
        scroll.Content(grid);scroll.MaxHeight(520);scroll.HorizontalScrollBarVisibility(ScrollBarVisibility::Disabled);scroll.VerticalScrollBarVisibility(ScrollBarVisibility::Auto);
        flyout.Content(scroll);pick.Flyout(flyout);
        flyout.Opened([weak](auto&&,auto&&){if(auto self=weak.lock()){self->open=true;self->data()->popup(true);self->request();}});
        flyout.Closed([weak](auto&&,auto&&){if(auto self=weak.lock()){self->open=false;self->data()->popup(false);}});
    }
    void act(J action){
        if(!property->current()||data()->updating||!flag(property->view(),L"enabled"))return;
        flyout.Hide();
        if(str(action,L"op")==L"rename_paper"){rename(action);return;}
        data()->dispatchDocument(O({{L"type",S(L"effect")},{L"action",action}}),to_hstring(uint64_t(property->epoch)));
    }
    fire_and_forget rename(J action){
        auto lifetime=shared_from_this();auto identity=property->identity();
        ContentDialog dialog;dialog.XamlRoot(pick.XamlRoot());dialog.Title(box_value(data()->caption(L"paper",L"rename_title")));
        dialog.PrimaryButtonText(data()->common(L"save"));dialog.CloseButtonText(data()->common(L"cancel"));dialog.DefaultButton(ContentDialogButton::Primary);
        dialog.RequestedTheme(data()->theme()==L"light"?ElementTheme::Light:ElementTheme::Dark);
        TextBox input;input.Header(box_value(data()->common(L"name")));input.Text(str(action,L"name"));input.MaxLength(64);AutomationProperties::SetAutomationId(input,L"paper-rename-name");
        auto validate=[source=data(),dialog=make_weak(dialog),input=make_weak(input)]{auto view=dialog.get();auto entry=input.get();if(view&&entry)view.IsPrimaryButtonEnabled(source->caption(O({{L"type",S(L"paper_name_error")},{L"name",S(entry.Text())}})).empty());};
        input.TextChanged([validate](auto&&,auto&&){validate();});validate();dialog.Content(input);
        input.Loaded([weak=make_weak(input)](auto&&,auto&&){if(auto input=weak.get()){input.Focus(FocusState::Programmatic);input.SelectAll();}});
        data()->popup(true);
        try{auto result=co_await dialog.ShowAsync();if(result==ContentDialogResult::Primary&&property->current()&&property->identity()==identity){
            action.Insert(L"name",S(input.Text()));data()->dispatchDocument(O({{L"type",S(L"effect")},{L"action",action}}),to_hstring(uint64_t(property->epoch)));
        }}catch(hresult_error const&){}
        data()->popup(false);
    }
    void rebuild(J const& paper){
        rebuilding=true;
        for(auto& tile:tiles)tile.upload->Close();tiles.clear();grid.Children().Clear();grid.RowDefinitions().Clear();
        auto list=array(paper,L"tiles");auto import=object(paper,L"import");uint32_t count=list.Size()+(import.Size()?1:0);auto weak=weak_from_this();
        for(uint32_t i=0;i<count;++i){
            if(i%4==0){RowDefinition row;row.Height({1,GridUnitType::Auto});grid.RowDefinitions().Append(row);}
            bool importing=i==list.Size();auto item=importing?import:list.GetObjectAt(i);auto title=str(item,L"label");
            auto tile=button(data(),title,[weak,i,importing,item]{if(auto self=weak.lock()){
                if(importing)self->act(object(item,L"action"));else{self->flyout.Hide();self->property->set(N(i));}
            }});tile.Width(84);tile.Padding({6,6,6,6});tile.HorizontalContentAlignment(HorizontalAlignment::Stretch);
            AutomationProperties::SetAutomationId(tile,importing?L"paper-import":L"paper-tile-"+to_hstring(i));tooltip(tile,title);
            StackPanel content;content.Spacing(4);Image image;image.Width(72);image.Height(72);image.Stretch(Stretch::UniformToFill);
            if(importing)content.Children().Append(icon(str(item,L"icon"),data()->theme(),28));else{
                Grid picture;picture.Width(72);picture.Height(72);picture.Children().Append(icon(L"layer-fill-symbolic",data()->theme(),28));picture.Children().Append(image);content.Children().Append(picture);tiles.push_back({tile,image,BitmapUpload::Create()});
                auto actions=array(item,L"actions");if(actions.Size()){
                    MenuFlyout menu;AutomationProperties::SetAutomationId(menu,L"paper-tile-menu");
                    for(auto value:actions){auto spec=value.GetObject();MenuFlyoutItem command;command.Text(str(spec,L"label"));
                        auto action=object(spec,L"action");AutomationProperties::SetAutomationId(command,L"paper-"+str(action,L"op")+L"-"+to_hstring(i));
                        command.Click([weak,action](auto&&,auto&&){if(auto self=weak.lock())self->act(action);});menu.Items().Append(command);
                    }tile.ContextFlyout(menu);
                }
            }
            auto caption=label(data(),title,false,false);caption.FontWeight(winrt::Windows::UI::Text::FontWeights::Normal());caption.TextWrapping(TextWrapping::WrapWholeWords);caption.TextAlignment(TextAlignment::Center);caption.MaxLines(2);caption.LineHeight(16);content.Children().Append(caption);tile.Content(content);
            Grid::SetColumn(tile,i%4);Grid::SetRow(tile,i/4);grid.Children().Append(tile);
        }
        rebuilding=false;
    }
    void sources(){
        if(rebuilding)return;
        auto selected=uint32_t(num(object(property->model(),L"value"),L"value"));
        for(size_t i=0;i<tiles.size();++i){auto& tile=tiles[i];auto source=tile.upload->Image();if(tile.image.Source()!=source)tile.image.Source(source);
            bool chosen=i==selected;tile.control.Background(chosen?CapyUi::selected(data()):clear());AutomationProperties::SetItemStatus(tile.control,chosen?data()->caption(L"search",L"selected"):L"");
        }
        auto source=selected<tiles.size()?tiles[selected].upload->Image():nullptr;if(thumbnail.Source()!=source)thumbnail.Source(source);
    }
    void receive(PreviewPacket packet,hstring const& requested,std::vector<uint32_t> const& indices){
        busy=false;
        for(auto index:indices)if(index<tiles.size())tiles[index].pending=false;
        if(!packet||!property->current())return;
        if(requested!=key()){request();return;}
        auto status=J::Parse(to_hstring(capy_preview_metadata(packet.get())));if(str(status,L"key")!=requested)return;
        auto images=array(status,L"images");int size=int(num(status,L"size"));if(size<1||size>512||images.Size()>8)return;
        auto weak=weak_from_this();size_t bytes=size_t(size)*size*4*images.Size();
        for(auto index:indices)if(index<tiles.size())tiles[index].loaded=true;
        for(auto value:images){auto image=value.GetObject();auto index=uint32_t(num(image,L"index"));if(index>=tiles.size()||std::find(indices.begin(),indices.end(),index)==indices.end())continue;
            auto upload=tiles[index].upload;std::weak_ptr<BitmapUpload> expected=upload;tiles[index].pending=true;tiles[index].loaded=false;
            auto offset=size_t(num(image,L"offset"));
            upload->Submit({BitmapUpload::ReadPacket(packet),offset,bytes,size,size,false,true,
                [weak,index,expected,requested]{auto self=weak.lock();return self&&self->property->current()&&self->key()==requested&&index<self->tiles.size()&&self->tiles[index].upload==expected.lock();},
                [weak,index,expected](bool accepted){if(auto self=weak.lock();self&&index<self->tiles.size()&&self->tiles[index].upload==expected.lock()){
                    self->tiles[index].pending=false;self->tiles[index].loaded=accepted;self->sources();self->request();
                }}});
        }
        sources();request();
    }
    void request(){
        if(busy||rebuilding||!property->current()||!data()->query)return;
        auto selected=uint32_t(num(object(property->model(),L"value"),L"value"));std::vector<uint32_t> indices;
        if(selected<tiles.size()&&!tiles[selected].loaded&&!tiles[selected].pending)indices.push_back(selected);
        if(open)for(uint32_t i=0;i<tiles.size()&&indices.size()<8;++i)if(i!=selected&&!tiles[i].loaded&&!tiles[i].pending)indices.push_back(i);
        if(indices.empty())return;auto requested=key();if(requested.empty())return;
        A values;for(auto index:indices){values.Append(N(index));tiles[index].pending=true;}
        busy=true;auto weak=weak_from_this();auto queue=dispatcher;
        if(!data()->query(CanvasQueryKind::Papers,to_string(O({{L"key",S(requested)},{L"indices",values}}).Stringify()),[weak,queue,requested,indices](PreviewPacket packet){
            queue.TryEnqueue([weak,packet=std::move(packet),requested,indices]{if(auto self=weak.lock())self->receive(packet,requested,indices);});
        })){busy=false;for(auto index:indices)tiles[index].pending=false;}
    }
    void refresh(){
        auto model=property->model();AutomationProperties::SetName(grid,str(model,L"label"));auto paper=object(model,L"paper");auto next=paper.Stringify();auto library=str(object(data()->model,L"windows_papers"),L"generation");
        if(next!=schema||library!=resources){schema=next;resources=library;rebuild(paper);}
        auto options=array(object(model,L"kind"),L"options");auto selected=uint32_t(num(object(model,L"value"),L"value"));if(selected<options.Size())name.Text(options.GetStringAt(selected));
        AutomationProperties::SetName(pick,str(model,L"label"));pick.IsEnabled(property->current()&&flag(property->view(),L"enabled"));sources();request();
    }
};
struct PropertiesView : std::enable_shared_from_this<PropertiesView> {
    std::shared_ptr<WorkspaceData> data;
    StackPanel root,body,actions;
    ContentControl bodyGate;
    TextBlock title,layerType;
    Grid header;
    Button addFilter;
    TextBlock addFilterLabel;
    Image addFilterIcon,addFilterArrow;
    hstring addFilterTheme;
    ComboBox page,resource;
    Grid toolbar;
    FrameworkElement tonal{nullptr};
    Bindings tonalBindings;
    hstring actionSchema;
    std::vector<std::pair<uint32_t,Button>> actionButtons,groupButtons;
    std::vector<std::pair<uint32_t,MenuFlyoutItem>> menuItems;
    std::vector<uint32_t> presets;
    struct Field{FrameworkElement row{nullptr};Bindings bindings;hstring schema;};
    std::map<std::wstring,Field> fields;
    Bindings headings;
    hstring schema,pages,fieldOwner;
    static hstring fieldSchema(J const& c){
        return O({{L"kind",S(propertyKindSignature(c))},{L"color_action",object(c,L"color_action")},{L"domain",object(object(c,L"curve"),L"domain")}}).Stringify();
    }
    void arrange(std::vector<UIElement> const& items){
        auto children=body.Children();
        for(auto i=int32_t(children.Size())-1;i>=0;--i)if(std::find(items.begin(),items.end(),children.GetAt(uint32_t(i)))==items.end())children.RemoveAt(uint32_t(i));
        for(uint32_t i=0;i<items.size();++i){
            uint32_t at=0;
            if(i<children.Size()&&children.GetAt(i)==items[i])continue;
            if(children.IndexOf(items[i],at))children.Move(at,i);else children.InsertAt(i,items[i]);
        }
    }
    Grid propertyRow(std::shared_ptr<Property> const& property,FrameworkElement const& field,Bindings& bindings){
        Grid row;row.ColumnSpacing(data->panelMetric(L"control_spacing"));
        ColumnDefinition caption;caption.Width({1,GridUnitType::Auto});row.ColumnDefinitions().Append(caption);
        ColumnDefinition value;value.Width({1,GridUnitType::Star});row.ColumnDefinitions().Append(value);
        auto text=label(data,str(property->model(),L"label"));text.VerticalAlignment(VerticalAlignment::Center);text.TextTrimming(TextTrimming::CharacterEllipsis);row.Children().Append(text);
        auto measured=std::make_shared<std::pair<hstring,double>>();
        auto arrange=[data=data,caption,measured](double width){
            A labels;for(auto value:array(object(data->state,L"layer_properties"),L"controls")){auto item=value.GetObject();if(str(object(item,L"kind"),L"kind")==L"choice")labels.Append(S(str(item,L"label")));}
            auto key=labels.Stringify()+to_hstring(data->textSize());
            if(key!=measured->first){double largest=0;for(auto value:labels){auto text=label(data,value.GetString(),false,false);text.Measure({INFINITY,INFINITY});largest=std::max(largest,double(text.DesiredSize().Width));}*measured={key,largest};}
            caption.Width({std::min(measured->second,width*.45),GridUnitType::Pixel});
        };
        row.SizeChanged([arrange](auto&&,SizeChangedEventArgs const& event){arrange(event.NewSize().Width);});
        bindings.emplace_back([property,text,weak=make_weak(row),arrange]{auto title=str(property->model(),L"label");if(text.Text()!=title)text.Text(title);if(auto row=weak.get();row&&row.ActualWidth()>0)arrange(row.ActualWidth());});
        Grid::SetColumn(field,1);row.Children().Append(field);return row;
    }
    FrameworkElement build(J const& c,Bindings& bindings){
        auto property=std::make_shared<Property>(data,c);
        auto kind=object(c,L"kind");auto type=str(kind,L"kind");auto name=str(c,L"label");
        if(type==L"number"){
            auto captured=std::make_shared<bool>(false);NumberPresentation presentation;presentation.title=property->label().current;
            presentation.identity=[weak=std::weak_ptr<Property>(property)]{if(auto current=weak.lock())return current->identity();return hstring();};
            presentation.phase=[property,captured](hstring const& phase,double v){*captured=phase==L"down";property->action(property->setting(N(v)),phase);};
            presentation.reset=[property]{property->reset();};
            auto field=number(data,name,object(kind,L"numeric"),
                [property]{return num(object(property->model(),L"value"),L"value");},
                [property,captured](double v){property->action(property->setting(N(v)),*captured?L"move":L"");},
                bindings,nullptr,false,property->id(),false,presentation);
            return field;
        }
        if(type==L"choice"&&object(c,L"paper").Size()){
            auto picker=std::make_shared<PaperPicker>(property);picker->init();
            bindings.emplace_back([picker]{picker->refresh();});
            return propertyRow(property,picker->pick,bindings);
        }
        if(type==L"choice"){
            ComboBox choices;choices.MinWidth(0);choices.MinHeight(data->controlHeight());choices.Height(data->controlHeight());
            choices.HorizontalAlignment(HorizontalAlignment::Stretch);choices.Padding({6,0,0,0});
            choices.FontSize(data->textSize());
            choices.Background(data->brush(L"input"));choices.BorderThickness({0,0,0,0});choices.CornerRadius(data->controlCorner());
            for(auto option:array(kind,L"options"))comboOption(choices,option.GetString());
            AutomationProperties::SetName(choices,name);AutomationProperties::SetAutomationId(choices,property->id());
            choices.SelectionChanged([property,weak=make_weak(choices)](auto&&,auto&&){if(auto choices=weak.get();choices&&choices.SelectedIndex()>=0)property->set(N(choices.SelectedIndex()));});
            bindings.emplace_back([property,choices]{auto model=property->model();auto title=str(model,L"label");AutomationProperties::SetName(choices,title);auto options=array(object(model,L"kind"),L"options");for(uint32_t i=0;i<options.Size();++i)comboOptionText(choices,i,options.GetStringAt(i));auto selected=int(num(object(model,L"value"),L"value"));if(choices.SelectedIndex()!=selected)choices.SelectedIndex(selected);});
            return propertyRow(property,choices,bindings);
        }
        if(type==L"toggle"){
            CheckBox check;auto caption=label(data,name);check.Content(caption);check.MinHeight(data->controlHeight());check.Height(data->controlHeight());check.Padding({0});check.MinWidth(0);
            AutomationProperties::SetName(check,name);AutomationProperties::SetAutomationId(check,property->id());
            check.Click([property,weak=make_weak(check)](auto&&,auto&&){if(auto c=weak.get())property->set(B(c.IsChecked().Value()));});
            bindings.emplace_back([property,check,caption]{auto model=property->model();auto title=str(model,L"label");caption.Text(title);AutomationProperties::SetName(check,title);check.IsChecked(flag(object(model,L"value"),L"value"));});return check;
        }
        if(type==L"color"){
            auto color=ColorField(property,name,[property]{return object(object(property->model(),L"value"),L"value");},
                [property](J a){property->set(a);},bindings);
            auto action=object(c,L"color_action");
            if(!action.Size())return color;
            Grid row;row.ColumnSpacing(data->panelMetric(L"control_spacing"));
            ColumnDefinition fill;fill.Width({1,GridUnitType::Star});row.ColumnDefinitions().Append(fill);
            ColumnDefinition actionColumn;actionColumn.Width({data->panelMetric(L"control_height"),GridUnitType::Pixel});row.ColumnDefinitions().Append(actionColumn);
            row.Children().Append(color);
            auto bucket=button(data,data->caption(L"color",L"use_selected"),[property,action]{if(property->current()&&!property->data->updating&&flag(property->view(),L"enabled"))property->data->dispatchDocument(action,to_hstring(uint64_t(property->epoch)));});
            bucket.Content(icon(L"fill",data->theme()));bucket.Width(data->panelMetric(L"control_height"));bucket.Height(data->panelMetric(L"control_height"));bucket.VerticalAlignment(VerticalAlignment::Top);
            std::wstring id(str(c,L"key").c_str());std::replace(id.begin(),id.end(),L'_',L'-');
            bindings.emplace_back([data=data,bucket]{auto caption=data->caption(L"color",L"use_selected");AutomationProperties::SetName(bucket,caption);CapyUi::tooltip(bucket,caption);});AutomationProperties::SetAutomationId(bucket,hstring(id+L"-bucket"));Grid::SetColumn(bucket,1);row.Children().Append(bucket);
            return row;
        }
        if(type==L"curve")return CurveField(property,bindings);
        if(type==L"gradient")return GradientField(property,bindings);
        return Grid();
    }
    void runAction(uint32_t index)const{
        auto view=object(data->state,L"layer_properties");auto list=array(view,L"actions");
        if(data->updating||index>=list.Size()||!flag(view,L"enabled"))return;
        data->dispatchDocument(O({{L"type",S(L"effect")},{L"action",object(list.GetObjectAt(index),L"action")}}),
            to_hstring(uint64_t(num(object(data->state,L"document_file"),L"epoch"))));
    }
    Button actionButton(J const& spec,hstring const& id,hstring const& text,std::function<void()> click){
        auto pick=button(data,text,std::move(click));pick.Height(data->controlHeight());pick.MinWidth(data->panelMetric(L"control_height"));pick.Background(data->brush(L"input"));
        if(auto glyph=str(spec,L"icon");!glyph.empty()){pick.Width(data->panelMetric(L"control_height"));pick.Content(icon(glyph,data->theme()));}
        else pick.Padding({6,0,6,0});
        AutomationProperties::SetAutomationId(pick,id);return pick;
    }
    void buildActions(A const& list){
        actions.Children().Clear();actionButtons.clear();groupButtons.clear();menuItems.clear();presets.clear();
        std::vector<hstring> groups;auto weak=weak_from_this();
        for(uint32_t i=0;i<list.Size();++i){
            auto spec=list.GetObjectAt(i);auto action=object(spec,L"action");
            if(str(action,L"op")==L"lookup_preset"){presets.push_back(i);continue;}
            auto group=object(spec,L"group");
            if(!group.Size()){
                auto pick=actionButton(spec,actionId(action),str(spec,L"label"),[weak,i]{if(auto self=weak.lock())self->runAction(i);});
                actionButtons.emplace_back(i,pick);actions.Children().Append(pick);continue;
            }
            auto groupId=str(group,L"id");if(std::find(groups.begin(),groups.end(),groupId)!=groups.end())continue;groups.push_back(groupId);
            auto menu=actionButton(spec,L"property-action-group-"+groupId,str(group,L"label"),[]{});
            MenuFlyout flyout;TrackPopup(flyout,data);
            for(uint32_t j=i;j<list.Size();++j){
                auto choice=list.GetObjectAt(j);if(str(object(choice,L"group"),L"id")!=groupId)continue;
                MenuFlyoutItem item;item.FontSize(data->textSize());item.MinHeight(32);
                AutomationProperties::SetAutomationId(item,actionId(object(choice,L"action")));
                item.Click([weak,j](auto&&,auto&&){if(auto self=weak.lock())self->runAction(j);});
                flyout.Items().Append(item);menuItems.emplace_back(j,item);
            }
            menu.Flyout(flyout);groupButtons.emplace_back(i,menu);actions.Children().Append(menu);
        }
    }
    void refreshActions(J const& view){
        auto list=array(view,L"actions");
        A keys;for(auto value:list){
            auto spec=value.GetObject();
            keys.Append(O({{L"action",object(spec,L"action")},{L"icon",spec.GetNamedValue(L"icon",JsonValue::CreateNullValue())},{L"group",S(str(object(spec,L"group"),L"id"))}}));
        }
        if(auto next=keys.Stringify();next!=actionSchema){actionSchema=next;buildActions(list);}
        auto name=[](Button const& pick,hstring const& text){AutomationProperties::SetName(pick,text);tooltip(pick,text);};
        for(auto const& [index,pick]:actionButtons){
            auto spec=list.GetObjectAt(index);auto text=str(spec,L"label");name(pick,text);
            if(str(spec,L"icon").empty())if(auto shown=pick.Content().try_as<Windows::Foundation::IReference<hstring>>();!shown||shown.Value()!=text)pick.Content(box_value(text));
        }
        for(auto const& [index,pick]:groupButtons){
            auto spec=list.GetObjectAt(index);auto text=str(object(spec,L"group"),L"label");name(pick,text);
            if(str(spec,L"icon").empty())pick.Content(box_value(text));
        }
        for(auto const& [index,item]:menuItems){auto text=str(list.GetObjectAt(index),L"label");item.Text(text);AutomationProperties::SetName(item,text);}
        for(auto const& [index,pick]:actionButtons)pick.IsEnabled(flag(view,L"enabled"));
        for(auto const& [index,pick]:groupButtons)pick.IsEnabled(flag(view,L"enabled"));
        auto resourceLabel=view.GetNamedValue(L"resource_label",JsonValue::CreateNullValue());
        bool lookup=resourceLabel.ValueType()==JsonValueType::String;
        resource.Visibility(lookup?Visibility::Visible:Visibility::Collapsed);resource.IsEnabled(flag(view,L"enabled"));
        if(lookup){
            std::vector<hstring> labels;for(auto index:presets)labels.push_back(str(list.GetObjectAt(index),L"label"));
            auto selection=view.GetNamedValue(L"resource_selection",JsonValue::CreateNullValue());int selected=-1;
            if(selection.ValueType()==JsonValueType::Number)
                if(auto at=std::find(presets.begin(),presets.end(),uint32_t(selection.GetNumber()));at!=presets.end())selected=int(at-presets.begin());
            auto resourceName=str(view,L"resource_name");
            if(selected<0){labels.push_back(resourceName.empty()?resourceLabel.GetString():resourceName);selected=int(labels.size())-1;}
            if(resource.Items().Size()!=labels.size()){resource.Items().Clear();for(auto const& text:labels)comboOption(resource,text);}
            for(uint32_t i=0;i<labels.size();++i)comboOptionText(resource,i,labels[i]);
            if(resource.SelectedIndex()!=selected)resource.SelectedIndex(selected);
            AutomationProperties::SetName(resource,resourceLabel.GetString());AutomationProperties::SetHelpText(resource,resourceName);
            tooltip(resource,resourceName.empty()?resourceLabel.GetString():resourceName);
        }
        bool leading=array(view,L"pages").Size()>1||lookup;
        toolbar.ColumnDefinitions().GetAt(0).Width({1,leading?GridUnitType::Star:GridUnitType::Auto});
        toolbar.Visibility(array(view,L"pages").Size()>1||list.Size()?Visibility::Visible:Visibility::Collapsed);
        bool statistics=flag(view,L"histogram");tonal.Visibility(statistics?Visibility::Visible:Visibility::Collapsed);
        if(statistics)for(auto const& bind:tonalBindings)bind();
    }
    void styleChoice(ComboBox const& box,hstring const& id){
        box.MinWidth(0);box.MinHeight(data->controlHeight());box.Height(data->controlHeight());box.Padding({6,0,0,0});box.HorizontalAlignment(HorizontalAlignment::Stretch);box.FontSize(data->textSize());
        box.Background(data->brush(L"input"));box.BorderThickness({0,0,0,0});box.CornerRadius(data->controlCorner());
        AutomationProperties::SetAutomationId(box,id);box.Visibility(Visibility::Collapsed);
    }
    explicit PropertiesView(std::shared_ptr<WorkspaceData> source):data(std::move(source)){
        root.Spacing(data->panelMetric(L"control_spacing"));body.Spacing(data->panelMetric(L"control_spacing"));title=label(data,L"",true);
        title.TextTrimming(TextTrimming::CharacterEllipsis);
        AutomationProperties::SetAutomationId(title,L"properties-layer-name");
        layerType=label(data,L"");layerType.Opacity(.55);layerType.TextTrimming(TextTrimming::CharacterEllipsis);layerType.VerticalAlignment(VerticalAlignment::Center);
        AutomationProperties::SetAutomationId(layerType,L"properties-layer-type");
        header.ColumnSpacing(data->panelMetric(L"control_spacing"));header.Margin({0,0,0,data->panelMetric(L"group_spacing")-data->panelMetric(L"control_spacing")});
        for(auto width:{GridUnitType::Star,GridUnitType::Auto}){ColumnDefinition column;column.Width({1,width});header.ColumnDefinitions().Append(column);}
        AutomationProperties::SetAutomationId(root,L"layer-properties");
        bodyGate.Content(body);bodyGate.IsTabStop(false);bodyGate.HorizontalContentAlignment(HorizontalAlignment::Stretch);
        styleChoice(page,L"properties-page");styleChoice(resource,L"property-resource-choice");
        toolbar.ColumnSpacing(data->panelMetric(L"control_spacing"));
        for(auto width:{GridUnitType::Star,GridUnitType::Auto}){ColumnDefinition column;column.Width({1,width});toolbar.ColumnDefinitions().Append(column);}
        actions.Orientation(Orientation::Horizontal);actions.Spacing(4);actions.VerticalAlignment(VerticalAlignment::Center);
        Grid::SetColumn(actions,1);toolbar.Children().Append(page);toolbar.Children().Append(resource);toolbar.Children().Append(actions);
        tonal=CapyScopes::TonalScope(data,tonalBindings);tonal.Visibility(Visibility::Collapsed);
        page.SelectionChanged([weak=make_weak(page),data=data](auto&&,auto&&){
            auto box=weak.get();if(!box||data->updating||box.SelectedIndex()<0)return;
            auto view=object(data->state,L"layer_properties");auto pages=array(view,L"pages");
            if(uint32_t(box.SelectedIndex())>=pages.Size())return;auto id=str(pages.GetObjectAt(box.SelectedIndex()),L"id");
            if(id==str(view,L"page")||view.GetNamedValue(L"layer",JsonValue::CreateNullValue()).ValueType()!=JsonValueType::Number)return;
            data->dispatchDocument(O({{L"type",S(L"effect")},{L"action",O({{L"op",S(L"select_page")},{L"layer",N(num(view,L"layer"))},{L"page",S(id)}})}}),
                to_hstring(uint64_t(num(object(data->state,L"document_file"),L"epoch"))));
        });
        addFilter=button(data,L"",[]{});addFilter.MinHeight(24);addFilter.Height(24);addFilter.Padding({6,0,6,0});
        {StackPanel content;content.Orientation(Orientation::Horizontal);content.Spacing(data->panelMetric(L"control_spacing"));
            addFilterLabel=label(data,L"");addFilterLabel.VerticalAlignment(VerticalAlignment::Center);
            addFilterIcon.Width(16);addFilterIcon.Height(16);content.Children().Append(addFilterIcon);content.Children().Append(addFilterLabel);addFilterArrow.Width(12);addFilterArrow.Height(12);content.Children().Append(addFilterArrow);addFilter.Content(content);}
        addFilter.Click([data=data,weak=make_weak(addFilter)](auto&&,auto&&){
            auto anchor=weak.get();auto model=object(object(data->state,L"layer_properties"),L"add_filter");
            if(data->updating||!anchor||!anchor.XamlRoot()||!model.Size())return;
            auto epoch=to_hstring(uint64_t(num(object(data->state,L"document_file"),L"epoch")));
            MenuFlyout menu;TrackPopup(menu,data);
            NativeMenuItems(menu.Items(),array(model,L"sections"),data,[data,epoch](J action){data->dispatchDocument(action,epoch);});
            menu.ShowAt(anchor);
        });
        AutomationProperties::SetAutomationId(addFilter,L"properties-add-filter");
        Grid::SetColumn(addFilter,1);header.Children().Append(layerType);header.Children().Append(addFilter);
        for(FrameworkElement part:{FrameworkElement(title),FrameworkElement(header),FrameworkElement(toolbar),tonal,FrameworkElement(bodyGate)})root.Children().Append(part);
    }
    void init(){
        resource.SelectionChanged([weak=weak_from_this()](auto&&,auto&&){
            auto self=weak.lock();if(!self||self->data->updating)return;
            auto index=self->resource.SelectedIndex();
            if(index>=0&&size_t(index)<self->presets.size())self->runAction(self->presets[size_t(index)]);
        });
    }
    void refresh(){
        Updating updating(data);
        auto view=object(data->state,L"layer_properties");
        auto filterMenu=object(view,L"add_filter");addFilter.Visibility(filterMenu.Size()?Visibility::Visible:Visibility::Collapsed);
        addFilterLabel.Text(str(filterMenu,L"title"));AutomationProperties::SetName(addFilter,str(filterMenu,L"title"));
        if(addFilterTheme!=data->theme()){addFilterTheme=data->theme();addFilterIcon.Source(icon(L"add-filter",addFilterTheme).Source());addFilterArrow.Source(icon(L"chevron-down",addFilterTheme).Source());}
        title.Text(str(view,L"name"));AutomationProperties::SetName(root,str(view,L"title"));CapyUi::tooltip(title,str(view,L"title"));
        layerType.Text(str(view,L"layer_type"));layerType.FontSize(data->textSize()*.9);CapyUi::tooltip(layerType,str(view,L"layer_type"));
        header.Visibility(!str(view,L"layer_type").empty()||filterMenu.Size()?Visibility::Visible:Visibility::Collapsed);
        auto pageChoices=array(view,L"pages");
        A pageIds;for(auto choice:pageChoices)pageIds.Append(S(str(choice.GetObject(),L"id")));
        if(auto key=pageIds.Stringify();key!=pages){pages=key;page.Items().Clear();for(auto choice:pageChoices)comboOption(page,str(choice.GetObject(),L"label"));}
        for(uint32_t i=0;i<pageChoices.Size();++i)comboOptionText(page,i,str(pageChoices.GetObjectAt(i),L"label"));
        int32_t selected=-1;for(uint32_t i=0;i<pageChoices.Size();++i)if(str(pageChoices.GetObjectAt(i),L"id")==str(view,L"page"))selected=int32_t(i);
        if(page.SelectedIndex()!=selected)page.SelectedIndex(selected);
        page.Visibility(pageChoices.Size()>1?Visibility::Visible:Visibility::Collapsed);page.IsEnabled(flag(view,L"enabled"));
        AutomationProperties::SetName(page,str(view,L"title"));
        refreshActions(view);
        A keys;for(auto value:array(view,L"controls")){
            auto c=value.GetObject();keys.Append(O({{L"key",S(str(c,L"key"))},{L"field",S(fieldSchema(c))},
                {L"section_id",c.GetNamedValue(L"section_id",JsonValue::CreateNullValue())}}));
        }
        auto owner=to_hstring(uint64_t(num(object(data->state,L"document_file"),L"epoch")))+L":"+view.GetNamedValue(L"layer",JsonValue::CreateNullValue()).Stringify();
        auto next=O({{L"owner",S(owner)},{L"controls",keys}}).Stringify();
        if(next!=schema){
            schema=next;headings.clear();std::vector<UIElement> items;
            auto previous=std::exchange(fields,{});if(owner!=fieldOwner)previous.clear();fieldOwner=owner;
            hstring sectionId=L"null";
            for(auto value:array(view,L"controls")){
                auto c=value.GetObject();
                auto nextSectionId=c.GetNamedValue(L"section_id",JsonValue::CreateNullValue()).Stringify();
                if(sectionId!=nextSectionId){sectionId=nextSectionId;auto section=str(c,L"section");
                    if(!section.empty()){auto heading=label(data,section,true);heading.Margin({0,items.empty()?0.:2.,0,0});items.push_back(heading);auto property=std::make_shared<Property>(data,c);headings.emplace_back([heading,property]{heading.Text(str(property->model(),L"section"));});}
                }
                std::wstring key=str(c,L"key").c_str();auto shape=fieldSchema(c);
                auto found=previous.find(key);
                Field field=found!=previous.end()&&found->second.schema==shape?std::move(found->second):Field{nullptr,{},shape};
                if(!field.row){field.row=build(c,field.bindings);}
                items.push_back(field.row);fields.insert_or_assign(key,std::move(field));
            }
            arrange(items);
        }
        for(auto const& update:headings)update();
        for(auto const& [key,field]:fields)for(auto const& update:field.bindings)update();
        bodyGate.IsEnabled(flag(view,L"enabled"));body.Opacity(flag(view,L"enabled")?1.:.4);
    }
};
}
FrameworkElement CapyEffects::ColorField(std::shared_ptr<Property> const& property,hstring const& title,
    std::function<J()> get,std::function<void(J)> set,Bindings& bindings,std::function<hstring()> context,std::function<hstring()> currentTitle){
    auto editor=std::make_shared<ColorButton>();editor->data=property->data;editor->id=property->id();
    editor->get=std::move(get);editor->set=std::move(set);editor->context=std::move(context);
    editor->currentTitle=currentTitle?std::move(currentTitle):std::function<hstring()>([property]{return str(property->model(),L"label");});
    editor->opaque=[property]{return flag(object(property->model(),L"kind"),L"opaque");};
    editor->init(title);bindings.emplace_back([editor]{editor->refresh();});return editor->root;
}
Button CapyEffects::CompactColorField(std::shared_ptr<WorkspaceData> const& data,hstring const& id,std::function<hstring()> title,
    std::function<J()> get,std::function<void(J)> set,Bindings& bindings,std::function<hstring()> context){
    auto editor=std::make_shared<ColorButton>();editor->data=data;editor->id=id;editor->compact=true;
    editor->get=std::move(get);editor->set=std::move(set);editor->context=std::move(context);
    editor->currentTitle=std::move(title);editor->opaque=[]{return false;};
    editor->init(editor->currentTitle());bindings.emplace_back([editor]{editor->refresh();});return editor->pick;
}
FrameworkElement PropertiesPanel(std::shared_ptr<WorkspaceData> const& data,Bindings& bindings){
    auto view=std::make_shared<PropertiesView>(data);view->init();bindings.emplace_back([view]{view->refresh();});return view->root;
}
