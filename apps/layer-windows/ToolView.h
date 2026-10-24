#pragma once
#include "UiControls.h"

inline winrt::Microsoft::UI::Xaml::Media::ImageSource BrushPreview(int id,winrt::hstring const& theme){
    return winrt::Microsoft::UI::Xaml::Media::Imaging::BitmapImage(CapyUi::asset(L"brush-previews/"+std::to_wstring(id)+L"-"+std::wstring(theme.c_str())+L".png"));
}

winrt::Microsoft::UI::Xaml::FrameworkElement ToolSetPanel(
    std::shared_ptr<CapyUi::WorkspaceData> const& data, CapyUi::Bindings& bindings, winrt::hstring const& panel=L"brushes",std::function<CapyUi::J()> projection={});
winrt::Microsoft::UI::Xaml::FrameworkElement ToolSettingsPanel(
    std::shared_ptr<CapyUi::WorkspaceData> const& data, CapyUi::Bindings& bindings);
winrt::Microsoft::UI::Xaml::FrameworkElement BrushSizePanel(
    std::shared_ptr<CapyUi::WorkspaceData> const& data, CapyUi::Bindings& bindings);
