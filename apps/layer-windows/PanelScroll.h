#pragma once
#include "UiControls.h"

namespace CapyUi {
inline void panelScrolling(ScrollView const& scroll,UIElement const& content,std::shared_ptr<WorkspaceData> const& data){
    struct Contact {std::optional<uint32_t> pointer;uint64_t epoch=0;double offset=0;bool running=false,canceled=false;std::function<void()> complete;};
    auto state=std::make_shared<Contact>();auto context=std::weak_ptr(data);auto weak=make_weak(scroll);auto surface=make_weak(content);
    auto forget=[context,state]{if(auto data=context.lock();data&&state->pointer)data->pointerCancels.Forget(*state->pointer,state.get());state->pointer.reset();};
    auto cancel=[state,forget,surface]{state->canceled=true;state->running=false;forget();if(auto content=surface.get())content.ManipulationMode(ManipulationModes::None);if(auto complete=std::exchange(state->complete,{}))complete();};
    scroll.IgnoredInputKinds(ScrollingInputKinds::Touch|ScrollingInputKinds::Pen);scroll.ContentOrientation(ScrollingContentOrientation::Vertical);
    scroll.Background(nullptr);scroll.Loaded([weak](auto&&,auto&&){if(auto scroll=weak.get())if(auto presenter=scroll.ScrollPresenter())presenter.Background(nullptr);});
    content.ManipulationMode(ManipulationModes::TranslateY|ManipulationModes::TranslateInertia);
    content.AddHandler(UIElement::PointerPressedEvent(),box_value(PointerEventHandler([context,state,cancel,surface](auto&&,PointerRoutedEventArgs const& e){
        if(auto data=context.lock();data&&data->claimBubbleContact(e))return;
        auto point=e.GetCurrentPoint(nullptr);
        if(state->pointer||point.PointerDeviceType()==Microsoft::UI::Input::PointerDeviceType::Mouse||!PointerGesture::PrimaryContact(point))return;
        if(state->complete)cancel();
        if(auto content=surface.get())content.ManipulationMode(ManipulationModes::TranslateY|ManipulationModes::TranslateInertia);
        state->pointer=point.PointerId();state->canceled=false;
        if(auto data=context.lock()){state->epoch=uint64_t(num(object(data->state,L"document_file"),L"epoch"));data->pointerCancels.Observe(*state->pointer,state.get(),cancel);}
    })),true);
    content.ManipulationStarting([weak,state](auto&&,ManipulationStartingRoutedEventArgs const& e){if(auto scroll=weak.get())e.Container(scroll);if(state->canceled||!state->pointer)e.Mode(ManipulationModes::None);});
    content.ManipulationStarted([weak,state,cancel](auto&&,ManipulationStartedRoutedEventArgs const& e){state->complete=[e]{e.Complete();};if(auto scroll=weak.get();scroll&&!state->canceled){state->offset=scroll.VerticalOffset();state->running=true;}else cancel();});
    content.ManipulationDelta([weak,context,state,cancel](auto&&,ManipulationDeltaRoutedEventArgs const& e){
        state->complete=[e]{e.Complete();};auto data=context.lock();auto scroll=weak.get();
        if(!scroll||!data||!state->running||state->canceled||scroll.VerticalScrollMode()==ScrollingScrollMode::Disabled||state->epoch!=uint64_t(num(object(data->state,L"document_file"),L"epoch"))){cancel();return;}
        state->offset=std::clamp(state->offset-e.Delta().Translation.Y,0.,scroll.ScrollableHeight());
        scrollVertical(scroll,state->offset);e.Handled(true);
    });
    content.ManipulationCompleted([state](auto&&,auto&&){state->complete=nullptr;state->running=false;});
    content.AddHandler(UIElement::PointerReleasedEvent(),box_value(PointerEventHandler([state,forget,cancel](auto&&,PointerRoutedEventArgs const& e){
        if(state->pointer==e.Pointer().PointerId()){if(e.GetCurrentPoint(nullptr).Properties().IsCanceled())cancel();else forget();}
    })),true);
    content.AddHandler(UIElement::PointerCanceledEvent(),box_value(PointerEventHandler([state,cancel](auto&&,PointerRoutedEventArgs const& e){
        if(state->pointer==e.Pointer().PointerId())cancel();
    })),true);
    content.as<FrameworkElement>().Unloaded([cancel](auto&&,auto&&){cancel();});
}
}
