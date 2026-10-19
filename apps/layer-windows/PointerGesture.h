#pragma once
#include "PointerCompletion.h"
#include <winrt/Microsoft.UI.Xaml.Input.h>
#include <winrt/Microsoft.UI.Input.h>
#include <winrt/Microsoft.UI.Xaml.Controls.h>
#include <winrt/Microsoft.UI.Xaml.Controls.Primitives.h>
#include <winrt/Microsoft.UI.Xaml.Media.h>
#include <memory>

namespace CapyUi {
class PointerGesture : public winrt::Microsoft::UI::Input::GestureRecognizer {
    std::shared_ptr<bool> motion=std::make_shared<bool>(false);
    static void ownHolding(winrt::Microsoft::UI::Xaml::DependencyObject const& node){
        using namespace winrt::Microsoft::UI::Xaml;
        if(node.try_as<Controls::TextBox>())return;
        if(auto element=node.try_as<UIElement>()){element.CanDrag(false);element.IsHoldingEnabled(false);}
        for(int i=0;i<Media::VisualTreeHelper::GetChildrenCount(node);++i)ownHolding(Media::VisualTreeHelper::GetChild(node,i));
    }
    template<class Finish>
    static void completions(winrt::Microsoft::UI::Xaml::UIElement const& element,Finish finish){
        using namespace winrt::Microsoft::UI::Xaml;
        auto bind=[&](RoutedEvent const& event,PointerCompletion completion){
            element.AddHandler(event,winrt::box_value(Input::PointerEventHandler([finish,completion](auto&& sender,Input::PointerRoutedEventArgs const& e){
                finish(sender,e,completion);
            })),true);
        };
        bind(UIElement::PointerReleasedEvent(),PointerCompletion::Released);
        bind(UIElement::PointerCanceledEvent(),PointerCompletion::Canceled);
        bind(UIElement::PointerCaptureLostEvent(),PointerCompletion::CaptureLost);
    }
public:
    static bool PrimaryContact(winrt::Microsoft::UI::Input::PointerPoint const& point){
        auto properties=point.Properties();
        return point.IsInContact()&&!properties.IsCanceled()&&!properties.IsRightButtonPressed()&&!properties.IsBarrelButtonPressed()&&
            (point.PointerDeviceType()!=winrt::Microsoft::UI::Input::PointerDeviceType::Mouse||properties.IsLeftButtonPressed());
    }
    static bool Captured(winrt::Microsoft::UI::Xaml::UIElement const& element,uint32_t id){
        if(auto captures=element.PointerCaptures())for(auto pointer:captures)if(pointer.PointerId()==id)return true;
        return false;
    }
    template<class Leave>
    static void OnLeaving(winrt::Microsoft::UI::Xaml::FrameworkElement const& element,Leave leave){
        using namespace winrt::Microsoft::UI::Xaml;
        element.AddHandler(UIElement::PointerExitedEvent(),winrt::box_value(Input::PointerEventHandler([leave](auto&& sender,Input::PointerRoutedEventArgs const& e){
            auto element=sender.template as<FrameworkElement>();
            if(Outside(element,e))leave(e);
        })),true);
    }
    static bool Outside(winrt::Microsoft::UI::Xaml::FrameworkElement const& element,winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const& event){
        auto at=event.GetCurrentPoint(element).Position();
        return at.X<0||at.Y<0||at.X>=element.ActualWidth()||at.Y>=element.ActualHeight();
    }
    template<class Finish>
    static void OnCompleted(winrt::Microsoft::UI::Xaml::UIElement const& element,Finish finish,
        PointerCaptureLoss loss=PointerCaptureLoss::Cancel){
        completions(element,[finish,loss](auto&&,auto const& e,PointerCompletion completion){
            auto point=e.GetCurrentPoint(nullptr);
            finish(e,CanceledCompletion(completion,point.Properties().IsCanceled(),point.IsInContact(),loss));
        });
    }
    template<class Finish>
    static std::shared_ptr<PointerClick> Clicks(winrt::Microsoft::UI::Xaml::Controls::Primitives::ButtonBase const& element,Finish finish){
        using namespace winrt::Microsoft::UI::Xaml;
        auto clicks=std::make_shared<PointerClick>();
        element.AddHandler(UIElement::PointerPressedEvent(),winrt::box_value(Input::PointerEventHandler([clicks](auto&& sender,Input::PointerRoutedEventArgs const& e){
            auto point=e.GetCurrentPoint(nullptr);
            if(PrimaryContact(point)&&Captured(sender.template as<UIElement>(),point.PointerId()))clicks->Begin(point.PointerId());
        })),true);
        element.Unloaded([clicks,finish](auto&&,auto&&){if(auto id=clicks->Cancel())finish(*id,true);});
        element.AddHandler(UIElement::PreviewKeyDownEvent(),winrt::box_value(Input::KeyEventHandler([clicks](auto&&,auto&&){clicks->Reset();})),true);
        completions(element,[clicks,finish](auto&& sender,Input::PointerRoutedEventArgs const& e,PointerCompletion completion){
            auto point=e.GetCurrentPoint(nullptr);auto id=point.PointerId();
            bool cancel=CanceledCompletion(completion,point.Properties().IsCanceled(),point.IsInContact(),PointerCaptureLoss::CompleteLift);
            if(clicks->Complete(id,completion,cancel,Captured(sender.template as<UIElement>(),id)))finish(id,cancel);
        });
        return clicks;
    }
    static void OwnHolding(winrt::Microsoft::UI::Xaml::FrameworkElement const& element){
        ownHolding(element);
        element.Loaded([](auto&& sender,auto&&){ownHolding(sender.template as<winrt::Microsoft::UI::Xaml::DependencyObject>());});
    }
    PointerGesture(){
        Dragging([motion=motion](auto&&,auto const& event){
            if(event.DraggingState()==winrt::Microsoft::UI::Input::DraggingState::Started)*motion=true;
        });
        ManipulationStarted([motion=motion](auto&&,auto&&){*motion=true;});
    }
    void Begin(winrt::Microsoft::UI::Xaml::UIElement const& source,winrt::Microsoft::UI::Input::PointerPoint const& point,
        winrt::Microsoft::UI::Input::GestureSettings settings=winrt::Microsoft::UI::Input::GestureSettings::Hold){
        using Settings=winrt::Microsoft::UI::Input::GestureSettings;
        if(source)ownHolding(source);
        *motion=false;
        GestureSettings(settings|Settings::Tap|Settings::Drag|Settings::ManipulationTranslateX|Settings::ManipulationTranslateY);
        ProcessDownEvent(point);
    }
    void Move(winrt::Microsoft::UI::Xaml::UIElement const& source,winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const& event){
        if(!event.IsGenerated())if(auto points=event.GetIntermediatePoints(source))ProcessMoveEvents(points);
    }
    bool Moved()const{return *motion;}
};
}
