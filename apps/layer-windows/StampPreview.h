#pragma once
#include "UiControls.h"

namespace CapyUi {
using StampReply=std::function<void(J,Imaging::WriteableBitmap)>;
bool RequestStamp(PreviewTransport const&,J const&,winrt::Windows::UI::Color,StampReply);
}
