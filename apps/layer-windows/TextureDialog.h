#pragma once
#include "UiControls.h"
#include <winrt/Microsoft.UI.Xaml.Shapes.h>

namespace CapyUi {
struct PaperThumbnail : std::enable_shared_from_this<PaperThumbnail> {
    std::shared_ptr<WorkspaceData> data;
    ContentControl root;
    Grid face;
    Microsoft::UI::Xaml::Shapes::Path well,picture;
    ImageBrush pixels;
    Image glyph;
    double glyphSize=0;
    V shown{nullptr};
    hstring theme;
    static std::shared_ptr<PaperThumbnail> Create(std::shared_ptr<WorkspaceData> const& data,double size,double glyph,bool well=false);
    void show(V const& id,hstring const& label);
    void refresh();
};
void ChooseTexture(std::shared_ptr<WorkspaceData> const& data,UIElement const& owner,double layer);
}
