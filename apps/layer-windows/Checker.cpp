#include "pch.h"
#include "Checker.h"
#include <robuffer.h>
#include <winrt/Windows.Storage.Streams.h>

namespace CapyUi {
static winrt::fire_and_forget rasterChecker(winrt::agile_ref<Imaging::WriteableBitmap> bitmap,
    winrt::agile_ref<winrt::Windows::Storage::Streams::IBuffer> buffer,uint8_t* bytes,int size,double scale,
    winrt::Windows::UI::Color light,winrt::Windows::UI::Color dark,double cell,
    winrt::Microsoft::UI::Dispatching::DispatcherQueue queue){
    co_await winrt::resume_background();
    for(int y=0;y<size;y++)for(int x=0;x<size;x++){
        double dx=std::fmod((x+.5)/scale,2*cell)-cell,dy=std::fmod((y+.5)/scale,2*cell)-cell;
        auto value=dx==0||dx*dy<0?dark:light;auto p=bytes+(y*size+x)*4;
        p[0]=value.B;p[1]=value.G;p[2]=value.R;p[3]=255;
    }
    queue.TryEnqueue([bitmap,buffer]{bitmap.get().Invalidate();});
}
winrt::Microsoft::UI::Xaml::Media::Imaging::WriteableBitmap checkerBitmap(double logical,double scale,winrt::Windows::UI::Color light,winrt::Windows::UI::Color dark,double cell){
    int size=int(std::ceil(logical*scale));
    winrt::Microsoft::UI::Xaml::Media::Imaging::WriteableBitmap result(size,size);uint8_t* bytes=nullptr;
    auto buffer=result.PixelBuffer();winrt::check_hresult(buffer.as<::Windows::Storage::Streams::IBufferByteAccess>()->Buffer(&bytes));
    rasterChecker(winrt::agile_ref(result),winrt::agile_ref(buffer),bytes,size,scale,light,dark,cell,Microsoft::UI::Dispatching::DispatcherQueue::GetForCurrentThread());
    return result;
}
}
