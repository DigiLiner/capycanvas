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
struct PaperPreview {ImageSource source{nullptr};bool pending=false;};
PaperPreview paperPreview(std::shared_ptr<WorkspaceData> const& data,hstring const& id);
void watchPaperPreviews(std::shared_ptr<WorkspaceData> const& data,std::weak_ptr<void> owner,std::function<void()> refresh);
hstring textureTileId(J const& tile);
struct TextureGrid : std::enable_shared_from_this<TextureGrid> {
    struct Tile {Button node{nullptr};std::shared_ptr<PaperThumbnail> picture;Microsoft::UI::Xaml::Shapes::Path hover{nullptr},ring{nullptr};hstring id;};
    std::shared_ptr<WorkspaceData> data;
    hstring prefix;
    Grid grid;
    std::vector<Tile> tiles;
    int columns=0;
    std::function<void(uint32_t)> picked,activated;
    std::function<void(J)> act;
    static std::shared_ptr<TextureGrid> Create(std::shared_ptr<WorkspaceData> const& data,hstring const& prefix);
    void rebuild(A const& list);
    void arrange();
    void mark(hstring const& id);
    bool focus(hstring const& id);
};
void ChooseTexture(std::shared_ptr<WorkspaceData> const& data,UIElement const& owner,double layer);
}
