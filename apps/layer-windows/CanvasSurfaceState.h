#pragma once
#include <cstdint>
#include <chrono>

class CanvasSurfaceState {
    uint64_t minimum=0;
    bool attached=false;
public:
    void Reset(uint64_t generation){minimum=generation+1;attached=false;}
    bool CanAttach(uint64_t generation,bool ready)const{return !attached&&ready&&generation>=minimum;}
    void DidAttach(){attached=true;}
    bool Attached()const{return attached;}
};

class CanvasLaunchTiming {
public:
    using Clock=std::chrono::steady_clock;
    static constexpr auto FadeDuration=std::chrono::milliseconds(180);
    explicit CanvasLaunchTiming(Clock::time_point start=Clock::now()):started(start){}
    std::chrono::milliseconds CaptionRemaining(Clock::time_point now,std::chrono::milliseconds delay)const {
        auto elapsed=std::chrono::duration_cast<std::chrono::milliseconds>(now-started);
        return elapsed<delay?delay-elapsed:std::chrono::milliseconds(0);
    }
private:
    Clock::time_point started;
};
