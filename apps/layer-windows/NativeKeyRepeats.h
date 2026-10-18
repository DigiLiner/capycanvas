#pragma once
#include <algorithm>
#include <cstdint>

template<typename Forward>
void ForwardKeyRepeats(bool pressed,bool wasDown,uint32_t count,Forward&& forward){
    for(uint32_t i=0,n=pressed?std::max(1u,count):1u;i<n;++i)forward(pressed&&(wasDown||i>0));
}
