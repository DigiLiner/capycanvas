#pragma once
#include "UiControls.h"

namespace CapyUi {
class NewDrawingDialog {
public:
    using Dispatch=std::function<void(std::string)>;
    NewDrawingDialog(std::shared_ptr<WorkspaceData> data,Dispatch document,Window window,std::function<void()> changed);
    ~NewDrawingDialog();
    void Open(J const& envelope);
    void Apply();
    bool IsOpen()const;
    void Hide();
private:
    struct Impl;
    std::shared_ptr<Impl> impl;
};
}
