#include "pch.h"
#include "ControlContext.h"
#include "NativeMenus.h"
#include <microsoft.ui.xaml.window.h>
#include <fstream>
using namespace CapyUi;
namespace {
int resultCode=0;
struct Fixture:std::enable_shared_from_this<Fixture> {
    Window window,other;StackPanel root;Button anchor,untagged;
    std::shared_ptr<WorkspaceData> data=std::make_shared<WorkspaceData>();
    bool windowClosed=false;OwnedPopup owner;std::optional<OwnedPopup::Ticket> origin;std::vector<MenuFlyout> popups;HWND hwnd=nullptr;unsigned checks=0,keys=0,presses=0,blurs=0,opened=0,closedCount=0,originPresses=0;
    std::filesystem::path output;hstring phase;
    winrt::apartment_context apartment;
    static void require(bool value,wchar_t const* text){if(!value)throw hresult_error(E_FAIL,text);}
    static HWND handle(Window const& value){HWND result=nullptr;check_hresult(value.as<IWindowNative>()->get_WindowHandle(&result));return result;}
    void privacy(){
        wchar_t expected[256],actual[256];DWORD bytes=0;
        auto length=GetEnvironmentVariableW(L"CAPY_PRIVATE_DESKTOP",expected,256);
        require(length&&length<256,L"Private desktop name required");
        check_bool(GetUserObjectInformationW(GetThreadDesktop(GetCurrentThreadId()),UOI_NAME,actual,sizeof actual,&bytes));
        require(std::wstring(expected)==actual,L"Fixture is not on its named private desktop");
        DWORD pid=0;GetWindowThreadProcessId(GetForegroundWindow(),&pid);
        require(pid==GetCurrentProcessId()&&GetForegroundWindow()==hwnd,L"Fixture does not own foreground input");
    }
    void nativePress(FrameworkElement const& target,bool right=false){
        privacy();auto position=target.TransformToVisual(nullptr).TransformPoint({float(target.ActualWidth()/2),float(target.ActualHeight()/2)});
        bool hit=false;for(auto const& element:VisualTreeHelper::FindElementsInHostCoordinates(position,root))if(element==target)hit=true;
        require(hit,L"Native point does not hit the intended XAML source");
        auto scale=root.XamlRoot().RasterizationScale();POINT point{LONG(position.X*scale),LONG(position.Y*scale)};check_bool(ClientToScreen(hwnd,&point));
        require(GetAncestor(WindowFromPoint(point),GA_ROOT)==hwnd,L"Native press target is outside owned window");
        check_bool(SetCursorPos(point.x,point.y));INPUT input[2]{};input[0].type=input[1].type=INPUT_MOUSE;
        input[0].mi.dwFlags=right?MOUSEEVENTF_RIGHTDOWN:MOUSEEVENTF_LEFTDOWN;input[1].mi.dwFlags=right?MOUSEEVENTF_RIGHTUP:MOUSEEVENTF_LEFTUP;
        require(SendInput(2,input,sizeof(INPUT))==2,L"Native pointer injection failed");
    }
    winrt::Windows::Foundation::IAsyncAction settle(){co_await winrt::resume_after(std::chrono::milliseconds(120));co_await apartment;}
    winrt::Windows::Foundation::IAsyncAction wait(std::function<bool()> ready,wchar_t const* message){
        auto deadline=std::chrono::steady_clock::now()+std::chrono::seconds(5);
        while(!ready()){require(std::chrono::steady_clock::now()<deadline,message);co_await winrt::resume_after(std::chrono::milliseconds(10));co_await apartment;}
    }
    MenuFlyout menu(){MenuFlyout popup;MenuFlyoutItem item;item.Text(L"Fixture action");popup.Items().Append(item);popup.Opened([weak=weak_from_this()](auto&&,auto&&){if(auto self=weak.lock())++self->opened;});popup.Closed([weak=weak_from_this()](auto&&,auto&&){if(auto self=weak.lock())++self->closedCount;});popups.push_back(popup);return popup;}
    bool visiblePopup(){for(auto const& popup:VisualTreeHelper::GetOpenPopupsForXamlRoot(root.XamlRoot()))if(popup.IsOpen())return true;return false;}
    void pending(){require(owner.Pending()&&!owner.Open()&&data->transients.size()==1,L"Pending popup lacks immediate unique cancellation registration");}
    void empty(){require(!owner.Active()&&!owner.Pending()&&!owner.Open()&&data->transients.empty()&&data->popupCount==0,L"Popup cancellation left retained state or registrations");}
    winrt::Windows::Foundation::IAsyncAction ordinaryOpening(){
        struct Observation{unsigned opening=0,opened=0,closed=0;bool cancelOpening=true;hstring error;};
        auto observed=std::make_shared<Observation>();MenuFlyout ordinary;MenuFlyoutItem item;item.Text(L"Ordinary popup action");ordinary.Items().Append(item);
        TrackPopup(ordinary,data);auto weak=weak_from_this();
        ordinary.Opening([weak,observed](auto&&,auto&&){if(auto self=weak.lock())try{
            ++observed->opening;
            require(self->data->transients.size()==1&&self->data->popupCount==0,L"Ordinary popup is not registered before native Opened");
            if(observed->cancelOpening){
                require(self->data->dismissTransients(),L"Ordinary popup ignored cancellation during native Opening");
                require(!self->data->dismissTransients(),L"Repeated opening cancellation retained a registration");
            }
        }catch(hresult_error const& e){observed->error=e.message();}catch(...){observed->error=L"Ordinary popup Opening callback failed";}});
        ordinary.Opened([observed](auto&&,auto&&){++observed->opened;});
        ordinary.Closed([observed](auto&&,auto&&){++observed->closed;});
        require(data->transients.empty()&&data->popupCount==0,L"Ordinary popup opening inherited another popup lifetime");
        ordinary.ShowAt(anchor);co_await settle();require(observed->error.empty(),observed->error.c_str());
        require(observed->opening==1&&!ordinary.IsOpen()&&!visiblePopup(),L"Canceled ordinary Opening left a visible or late native popup");
        require(!data->dismissTransients(),L"Late ordinary Opened resurrected cancellation registration");empty();
        observed->cancelOpening=false;auto beforeOpened=observed->opened;auto beforeClosed=observed->closed;
        ordinary.ShowAt(anchor);
        co_await wait([this,observed,beforeOpened]{return observed->opened==beforeOpened+1&&data->popupCount==1&&visiblePopup();},L"Same ordinary flyout did not reopen with fresh lifetime");
        require(observed->error.empty(),observed->error.c_str());
        require(observed->opening==2&&data->transients.size()==1,L"Reopened ordinary flyout lacks exactly one fresh registration");
        require(data->dismissTransients(),L"Reopened ordinary flyout ignored cancellation");
        require(!data->dismissTransients(),L"Repeated visible ordinary cancellation retained a registration");
        co_await wait([this,observed,beforeClosed]{return observed->closed==beforeClosed+1&&!visiblePopup();},L"Reopened ordinary flyout did not acknowledge native Closed");
        co_await settle();require(!ordinary.IsOpen(),L"Ordinary flyout reopened after cancellation");empty();
    }
    winrt::Windows::Foundation::IAsyncAction ordinaryClosing(){
        struct Observation{bool cancel=true,seen=false;int count=-1;};
        auto observed=std::make_shared<Observation>();auto blocker=std::make_shared<WorkspaceData>();
        blocker->popupChanged=[data=data](bool open){data->externalPopup=open;};
        MenuFlyout ordinary;MenuFlyoutItem item;item.Text(L"Ordinary popup action");ordinary.Items().Append(item);TrackPopup(ordinary,blocker);
        ordinary.Closing([weak=weak_from_this(),observed,blocker](auto const& sender,auto const& args){
            args.Cancel(observed->cancel);
            sender.DispatcherQueue().TryEnqueue([weak,observed,blocker]{if(weak.lock()){observed->count=blocker->popupCount;observed->seen=true;}});
        });
        ordinary.ShowAt(anchor);co_await wait([this,blocker]{return blocker->popupCount==1&&data->externalPopup;},L"Ordinary popup did not block foreign admission");
        ordinary.Hide();co_await wait([observed]{return observed->seen;},L"Canceled Closing did not finish event dispatch");
        require(observed->count==1&&data->externalPopup&&ordinary.IsOpen(),L"Canceled Closing retired popup admission");
        observed->cancel=false;observed->seen=false;ordinary.Hide();
        co_await wait([observed]{return observed->seen;},L"Accepted Closing did not finish event dispatch");
        require(observed->count==0&&!data->externalPopup,L"Accepted Closing blocked the next native menu until Closed");
        auto next=owner.Begin(data,anchor,menu());require(next.Current(),L"Closing popup rejected the next source request");next.Cancel();empty();
        co_await wait([this,&ordinary]{return !ordinary.IsOpen()&&!visiblePopup();},L"Ordinary popup did not finish closing");
        co_await wait([blocker]{return blocker->transients.empty();},L"Closed ordinary popup retained cancellation registration");
    }
    winrt::fire_and_forget run(){
        auto self=shared_from_this();hstring error;
        try {
            co_await wait([this]{return anchor.IsLoaded()&&untagged.ActualWidth()>0&&GetForegroundWindow()==hwnd;},L"Fixture did not become arranged and foreground");privacy();
            phase=L"registry";auto popup=menu();auto ticket=owner.Begin(data,anchor,popup);pending();
            require(data->dismissTransients(),L"Pending popup ignored window cancellation registry");
            require(!ticket.Current()&&!ticket.Show(anchor,{}),L"Registry-canceled reply reopened popup");empty();++checks;
            phase=L"replacement";auto reused=menu();auto old=owner.Begin(data,anchor,reused);auto current=owner.Begin(data,anchor,reused);pending();
            require(!old.Cancel()&&!old.Current()&&!old.Show(anchor,{})&&current.Current(),L"Superseded query canceled or replaced current popup lifetime");
            require(current.Cancel(),L"Current query cancellation was refused");empty();++checks;
            phase=L"unload";auto unloaded=owner.Begin(data,anchor,menu());root.Children().RemoveAt(0);co_await settle();
            require(!unloaded.Current()&&!unloaded.Show(untagged,{}),L"Unloaded source query opened on replacement anchor");empty();
            root.Children().InsertAt(0,anchor);co_await settle();++checks;
            phase=L"escape";auto escaped=owner.Begin(data,anchor,menu());pending();anchor.Focus(FocusState::Programmatic);privacy();
            INPUT keyboard[2]{};keyboard[0].type=keyboard[1].type=INPUT_KEYBOARD;keyboard[0].ki.wVk=keyboard[1].ki.wVk=VK_ESCAPE;keyboard[1].ki.dwFlags=KEYEVENTF_KEYUP;
            require(SendInput(2,keyboard,sizeof(INPUT))==2,L"Native Escape injection failed");co_await wait([this]{return keys==1&&!owner.Active();},L"Native Escape cancellation timed out");
            require(keys==1&&!escaped.Current()&&!escaped.Show(anchor,{}),L"Native Escape did not cancel delayed popup");empty();++checks;
            phase=L"originating-press";nativePress(anchor,true);
            co_await wait([this]{return originPresses==1;},L"Originating native press did not reach source");co_await settle();
            require(origin&&origin->Current(),L"Popup canceled itself on its originating routed press");pending();
            nativePress(untagged);co_await wait([this]{return presses==1&&!owner.Active();},L"Next native press did not cancel originating popup request");
            require(!origin->Current()&&!origin->Show(anchor,{}),L"Canceled originating request reopened popup");empty();++checks;
            phase=L"untagged-press";auto beforePress=presses;auto pressed=owner.Begin(data,anchor,menu());pending();co_await settle();co_await wait([this]{return anchor.IsLoaded()&&untagged.ActualWidth()>0&&GetForegroundWindow()==hwnd;},L"Fixture did not become arranged and foreground");privacy();
            nativePress(untagged);co_await wait([this,beforePress]{return presses==beforePress+1&&!owner.Active();},L"Native untagged press cancellation timed out");
            require(presses==beforePress+1&&!pressed.Current()&&!pressed.Show(anchor,{}),L"Handled native untagged press did not cancel delayed popup");empty();++checks;
            phase=L"blur";auto beforeBlur=blurs;auto blurred=owner.Begin(data,anchor,menu());pending();other.Activate();SetForegroundWindow(handle(other));co_await wait([this,beforeBlur]{return blurs>beforeBlur&&!owner.Active();},L"Native deactivation cancellation timed out");
            require(blurs>beforeBlur&&!blurred.Current()&&!blurred.Show(anchor,{}),L"Native window deactivation did not cancel delayed popup");empty();auto wrongOwner=owner.Begin(data,anchor,menu());
            require(!wrongOwner.Current()&&!wrongOwner.Show(anchor,{}),L"Popup accepted source owned by background window");empty();++checks;
            window.Activate();SetForegroundWindow(hwnd);co_await wait([this]{return anchor.IsLoaded()&&untagged.ActualWidth()>0&&GetForegroundWindow()==hwnd;},L"Fixture did not become arranged and foreground");privacy();
            phase=L"opening-gap";auto beforeGap=opened;auto opening=owner.Begin(data,anchor,menu());pending();require(opening.Show(anchor,{}),L"Current opening-gap popup did not show");
            require(owner.Pending()&&opened==beforeGap,L"Opening-gap prerequisite reached Opened synchronously");
            require(data->dismissTransients(),L"Opening popup ignored window cancellation registry");co_await settle();
            require(opened==beforeGap&&!visiblePopup(),L"Opening-gap cancellation left a late or visible native popup");
            require(!opening.Current()&&!opening.Show(anchor,{}),L"Opening-gap canceled reply reopened popup");empty();
            phase=L"ordinary-opening";co_await ordinaryOpening();phase=L"ordinary-closing";co_await ordinaryClosing();++checks;
            phase=L"show-close";auto beforeOpen=opened;auto beforeClosed=closedCount;auto shown=owner.Begin(data,anchor,menu());pending();require(shown.Show(anchor,{}),L"Current popup did not show");co_await wait([this,beforeOpen]{return opened==beforeOpen+1&&owner.Open();},L"Popup Opened timed out");
            require(opened==beforeOpen+1&&owner.Open()&&data->transients.size()==1,L"Popup registration did not survive ShowAt through Opened");
            require(data->dismissTransients(),L"Visible popup ignored cancellation registry");co_await wait([this,beforeClosed]{return closedCount==beforeClosed+1&&!visiblePopup();},L"Native popup Closed acknowledgement or visible-popup retirement timed out");
            require(!shown.Current()&&!shown.Show(anchor,{}),L"Reply reopened closed popup");empty();++checks;
            phase=L"window-close";auto closed=owner.Begin(data,anchor,menu());window.Close();co_await settle();
            require(!closed.Current()&&!closed.Show(anchor,{}),L"Reply reopened canceled owner");empty();++checks;
        } catch(hresult_error const& e){resultCode=1;error=e.message();}catch(...){resultCode=1;error=L"Unhandled popup fixture failure";}
        owner.Cancel();auto count=checks;auto step=to_string(phase);auto message=to_string(error);auto path=output;
        co_await resume_background();
        {std::ofstream file(path);file<<"{\"passed\":"<<count<<",\"exit_code\":"<<resultCode<<",\"phase\":\""<<step<<"\"}\n";if(!file)resultCode=1;}
        if(!message.empty()){std::ofstream file(path.parent_path()/L"popup-fixture-error.log");file<<message;}
        co_await apartment;other.Close();if(!windowClosed)window.Close();Application::Current().Exit();
    }
    void init(std::filesystem::path path){
        output=path.parent_path()/L"popup-fixture-result.json";
        wchar_t theme[16]{};GetEnvironmentVariableW(L"CAPY_POPUP_THEME",theme,16);root.RequestedTheme(std::wstring(theme)==L"light"?ElementTheme::Light:ElementTheme::Dark);
        anchor.Content(box_value(L"Delayed popup source"));untagged.Content(box_value(L"Untagged input target"));
        root.Children().Append(anchor);root.Children().Append(untagged);root.Width(400);root.Height(240);
        root.HorizontalAlignment(HorizontalAlignment::Left);root.VerticalAlignment(VerticalAlignment::Top);
        root.KeyDown([weak=weak_from_this()](auto&&,Input::KeyRoutedEventArgs const& e){if(e.Key()==Windows::System::VirtualKey::Escape)if(auto self=weak.lock()){++self->keys;e.Handled(self->data->dismissTransients());}});
        anchor.AddHandler(UIElement::PointerPressedEvent(),box_value(Input::PointerEventHandler([weak=weak_from_this()](auto&&,Input::PointerRoutedEventArgs const& e){
            if(auto self=weak.lock();self&&self->phase==L"originating-press"){
                ++self->originPresses;self->origin=self->owner.Begin(self->data,self->anchor,self->menu());e.Handled(true);
            }
        })),true);
        untagged.AddHandler(UIElement::PointerPressedEvent(),box_value(Input::PointerEventHandler([weak=weak_from_this()](auto&&,Input::PointerRoutedEventArgs const& e){if(auto self=weak.lock())++self->presses;e.Handled(true);})),true);
        window.Closed([weak=weak_from_this()](auto&&,auto&&){if(auto self=weak.lock()){self->windowClosed=true;self->owner.Cancel();}});
        window.Activated([weak=weak_from_this()](auto&&,WindowActivatedEventArgs const& e){if(e.WindowActivationState()==WindowActivationState::Deactivated)if(auto self=weak.lock()){++self->blurs;self->owner.Cancel();}});
        TextBlock text;text.Text(L"Owned blur target");other.Content(text);other.Title(L"Popup fixture blur target");other.AppWindow().Move({600,80});
        root.Loaded([weak=weak_from_this()](auto&&,auto&&){if(auto self=weak.lock())self->run();});
        window.Content(root);window.Title(L"Capy Canvas popup review");hwnd=handle(window);window.AppWindow().Move({40,40});window.Activate();
    }
};
struct App:ApplicationT<App,Markup::IXamlMetadataProvider>{
    Microsoft::UI::Xaml::XamlTypeInfo::XamlControlsXamlMetaDataProvider metadata;
    std::shared_ptr<Fixture> fixture;
    App(){UnhandledException([this](auto&&,UnhandledExceptionEventArgs const& e){resultCode=1;e.Handled(true);Exit();});}
    Markup::IXamlType GetXamlType(Windows::UI::Xaml::Interop::TypeName const& type){return metadata.GetXamlType(type);}
    Markup::IXamlType GetXamlType(hstring const& name){return metadata.GetXamlType(name);}
    com_array<Markup::XmlnsDefinition> GetXmlnsDefinitions(){return metadata.GetXmlnsDefinitions();}
    void OnLaunched(LaunchActivatedEventArgs const&){Resources().MergedDictionaries().Append(XamlControlsResources());wchar_t path[32768];auto size=GetEnvironmentVariableW(L"CAPY_POPUP_FIXTURE",path,32768);if(!size||size>=32768||!std::filesystem::path(path).is_absolute())throw hresult_invalid_argument();fixture=std::make_shared<Fixture>();fixture->init(path);}
};
}
int WINAPI wWinMain(HINSTANCE,HINSTANCE,PWSTR,int){init_apartment(apartment_type::single_threaded);Application::Start([](auto&&){make<App>();});return resultCode;}
