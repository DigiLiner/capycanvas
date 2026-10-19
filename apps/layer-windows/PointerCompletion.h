#pragma once
#include <cstdint>
#include <optional>
#include <utility>

namespace CapyUi {
enum class PointerCompletion { Released, Canceled, CaptureLost };
enum class PointerCaptureLoss { Cancel, CompleteLift };

inline bool CanceledCompletion(PointerCompletion completion,bool canceled,bool inContact,
    PointerCaptureLoss loss=PointerCaptureLoss::Cancel){
    return canceled||completion==PointerCompletion::Canceled||
        (completion==PointerCompletion::CaptureLost&&(loss==PointerCaptureLoss::Cancel||inContact));
}
class PointerClick {
    std::optional<uint32_t> pointer,completed;
    bool suppressed=false,released=false;
public:
    bool Owns(uint32_t id)const{return pointer==id;}
    bool Begin(uint32_t id){
        if(pointer)return false;
        Reset();pointer=id;return true;
    }
    void Suppress(uint32_t id){if(Owns(id))suppressed=true;}
    bool SuppressClick(){return std::exchange(suppressed,false);}
    bool Complete(uint32_t id,PointerCompletion phase,bool canceled,bool captured){
        bool ended=Owns(id);
        if(ended){pointer.reset();completed=id;}
        if(completed==id){
            if(phase==PointerCompletion::Released)released=true;
            if(canceled||(released&&(phase==PointerCompletion::CaptureLost||!captured)))Reset();
        }
        return ended;
    }
    void Reset(){if(!pointer){completed.reset();suppressed=false;released=false;}}
    std::optional<uint32_t> Cancel(){auto id=std::exchange(pointer,std::nullopt);Reset();return id;}
};
}
