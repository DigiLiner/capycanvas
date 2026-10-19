#pragma once
#include <optional>

namespace CapyUi {
class PointerScrub {
    double origin,current;
public:
    explicit PointerScrub(double y):origin(y),current(y){}
    std::optional<double> Move(double y,bool generated){
        if(generated||y==current)return std::nullopt;
        current=y;
        return origin-y;
    }
};
}
