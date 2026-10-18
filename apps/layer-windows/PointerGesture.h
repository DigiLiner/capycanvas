#pragma once
#include <winrt/Microsoft.UI.Input.h>
#include <memory>

namespace CapyUi {
class PointerGesture : public winrt::Microsoft::UI::Input::GestureRecognizer {
    std::shared_ptr<bool> motion=std::make_shared<bool>(false);
public:
    PointerGesture(){
        Dragging([motion=motion](auto&&,auto const& event){
            if(event.DraggingState()==winrt::Microsoft::UI::Input::DraggingState::Started)*motion=true;
        });
        ManipulationStarted([motion=motion](auto&&,auto&&){*motion=true;});
    }
    void Begin(winrt::Microsoft::UI::Input::PointerPoint const& point,
        winrt::Microsoft::UI::Input::GestureSettings settings=winrt::Microsoft::UI::Input::GestureSettings::Hold){
        using Settings=winrt::Microsoft::UI::Input::GestureSettings;
        *motion=false;
        GestureSettings(settings|Settings::Tap|Settings::Drag|Settings::ManipulationTranslateX|Settings::ManipulationTranslateY);
        ProcessDownEvent(point);
    }
    bool Moved()const{return *motion;}
};
}
