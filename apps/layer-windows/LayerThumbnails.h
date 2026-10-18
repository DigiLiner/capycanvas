#pragma once
#include "FilterPreviews.h"
#include <chrono>

namespace CapyUi {
struct LayerThumbnailCache;
std::shared_ptr<LayerThumbnailCache> CreateLayerThumbnailCache(PreviewTransport);
struct LayerThumbnail {
    winrt::hstring layer,target,revision;
    bool mask=false;
};
std::chrono::milliseconds RefreshLayerThumbnails(std::shared_ptr<LayerThumbnailCache> const&,
    winrt::hstring const& epoch,std::vector<LayerThumbnail> const& visible);
winrt::Microsoft::UI::Xaml::Media::ImageSource LayerThumbnailSource(
    std::shared_ptr<LayerThumbnailCache> const&,winrt::hstring const& epoch,LayerThumbnail const&);
}
