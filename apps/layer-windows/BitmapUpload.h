#pragma once
#include "CanvasQueryQueue.h"
#include <winrt/Microsoft.UI.Xaml.Media.Imaging.h>
#include <winrt/Windows.Graphics.Imaging.h>
#include <algorithm>
#include <array>
#include <cstring>
#include <span>

namespace CapyUi {
struct BitmapUpload : std::enable_shared_from_this<BitmapUpload> {
    using Bitmap=winrt::Windows::Graphics::Imaging::SoftwareBitmap;
    using Source=winrt::Microsoft::UI::Xaml::Media::Imaging::SoftwareBitmapSource;
    using Reader=std::function<std::span<uint8_t const>()>;
    struct Request {
        Reader read;size_t offset=0,byteLimit=0;int width=0,height=0;
        bool straightRgba=false,exactLength=false;
        std::function<bool()> current;std::function<void(bool)> complete;
    };
    struct Job {Request request;Bitmap bitmap{nullptr};unsigned back=0;uint64_t epoch=0;};
    winrt::Microsoft::UI::Dispatching::DispatcherQueue queue{winrt::Microsoft::UI::Dispatching::DispatcherQueue::GetForCurrentThread()};
    Source source{nullptr};std::array<Bitmap,2> buffers{nullptr,nullptr};
    std::optional<Request> pending;std::shared_ptr<Job> active;
    uint64_t epoch=0;unsigned front=0;bool editing=false,closed=false,ready=false;
    winrt::event_token shutdown{};
    struct Edit {
        bool& flag;bool previous;
        explicit Edit(BitmapUpload& owner):flag(owner.editing),previous(std::exchange(flag,true)){}
        ~Edit(){flag=previous;}
    };
    static Reader ReadPacket(PreviewPacket packet){
        return [packet=std::move(packet)]{
            return packet.Bytes();
        };
    }
    static std::shared_ptr<BitmapUpload> Create(){
        auto result=std::make_shared<BitmapUpload>();std::weak_ptr<BitmapUpload> weak=result;
        result->shutdown=result->queue.ShutdownStarting([weak](auto&&,auto&&){if(auto self=weak.lock())self->Close();});
        return result;
    }
    ~BitmapUpload(){try{queue.ShutdownStarting(shutdown);if(queue.HasThreadAccess()&&source)source.Close();}catch(...){}}
    static bool Current(Request const& request)noexcept{
        try{return request.current&&request.current();}catch(...){return false;}
    }
    static void Notify(std::function<void(bool)> complete,bool accepted)noexcept{
        try{if(complete)complete(accepted);}catch(...){}
    }
    winrt::Microsoft::UI::Xaml::Media::ImageSource Image()const{return ready?source:nullptr;}
    void Submit(Request request){
        {Edit edit(*this);
            if(closed){Notify(std::move(request.complete),false);return;}
            auto old=std::exchange(pending,std::move(request));
            if(old)Notify(std::move(old->complete),false);
        }Pump();
    }
    void Clear(){
        {Edit edit(*this);auto old=std::exchange(pending,std::nullopt);++epoch;
            auto previous=std::exchange(source,nullptr);buffers={nullptr,nullptr};front=0;ready=false;
            try{if(previous)previous.Close();}catch(...){}
            if(old)Notify(std::move(old->complete),false);
        }Pump();
    }
    void Close(){if(closed)return;closed=true;auto job=active;Clear();if(job)Finish(job,false);}
    void Pump(){
        if(editing||active||closed)return;Edit edit(*this);
        while(pending&&!active&&!closed){
            auto request=std::move(*pending);pending.reset();
            auto generation=epoch;
            if(!Current(request)||closed||epoch!=generation){Notify(std::move(request.complete),false);continue;}
            std::shared_ptr<Job> job;
            try{if(!source)source=Source();job=std::make_shared<Job>();}
            catch(...){Notify(std::move(request.complete),false);continue;}
            job->request=std::move(request);
            job->back=1-front;job->bitmap=std::exchange(buffers[job->back],nullptr);job->epoch=epoch;active=job;
            try{Fill(weak_from_this(),queue,job);}catch(...){Finish(job,false);}
        }
    }
    static winrt::fire_and_forget Fill(std::weak_ptr<BitmapUpload> weak,
        winrt::Microsoft::UI::Dispatching::DispatcherQueue queue,std::shared_ptr<Job> job){
        try{
            co_await winrt::resume_background();
            auto const& request=job->request;auto bytes=request.read();auto input=bytes.data();auto length=bytes.size();
            uint64_t rowBytes=uint64_t(request.width)*4,needed=rowBytes*uint64_t(request.height);
            if(!input||request.width<=0||request.height<=0||length>request.byteLimit
                ||(request.exactLength&&length!=request.byteLimit)||request.offset>length
                ||needed>length-request.offset)throw winrt::hresult_invalid_argument();
            if(!job->bitmap||job->bitmap.PixelWidth()!=request.width||job->bitmap.PixelHeight()!=request.height)
                job->bitmap=Bitmap(winrt::Windows::Graphics::Imaging::BitmapPixelFormat::Bgra8,
                    request.width,request.height,winrt::Windows::Graphics::Imaging::BitmapAlphaMode::Premultiplied);
            auto locked=job->bitmap.LockBuffer(winrt::Windows::Graphics::Imaging::BitmapBufferAccessMode::Write);
            auto memory=locked.CreateReference();auto plane=locked.GetPlaneDescription(0);auto output=memory.data();
            int64_t first=plane.StartIndex,last=first+int64_t(plane.Stride)*(request.height-1);
            if(plane.Width<request.width||plane.Height<request.height||std::min(first,last)<0
                ||uint64_t(std::max(first,last))+rowBytes>memory.Capacity()
                ||uint64_t(plane.Stride<0?-int64_t(plane.Stride):plane.Stride)<rowBytes)
                throw winrt::hresult_invalid_argument();
            for(int y=0;y<request.height;++y){
                auto in=input+request.offset+size_t(y)*size_t(rowBytes);auto out=output+first+int64_t(y)*plane.Stride;
                if(!request.straightRgba)std::memcpy(out,in,size_t(rowBytes));
                else for(int x=0;x<request.width;++x){
                    auto p=in+x*4;auto d=out+x*4;unsigned a=p[3];
                    d[0]=uint8_t((unsigned(p[2])*a+127)/255);d[1]=uint8_t((unsigned(p[1])*a+127)/255);
                    d[2]=uint8_t((unsigned(p[0])*a+127)/255);d[3]=uint8_t(a);
                }
            }
        }catch(...){job->bitmap=nullptr;}
        job->request.read={};
        try{queue.TryEnqueue([weak,job]{
            try{Publish(weak,job);}catch(...){if(auto self=weak.lock())self->Finish(job,false);}
        });}catch(...){}
    }
    static winrt::fire_and_forget Publish(std::weak_ptr<BitmapUpload> weak,std::shared_ptr<Job> job){
        auto self=weak.lock();if(!self||self->active!=job)co_return;Source target{nullptr};bool accepted=false;
        try{
            if(job->bitmap&&self->epoch==job->epoch){
                self->buffers[job->back]=job->bitmap;
                if(Current(job->request)&&!self->closed&&self->epoch==job->epoch){
                    target=self->source;co_await target.SetBitmapAsync(job->bitmap);
                    if(!self->queue.HasThreadAccess())co_return;
                    if(self->epoch==job->epoch&&self->source==target){
                        self->front=job->back;self->ready=true;
                        accepted=Current(job->request)&&!self->closed&&self->epoch==job->epoch&&self->source==target;
                    }
                }
            }
        }catch(...){
            if(!self->queue.HasThreadAccess())co_return;
            if(self->epoch==job->epoch&&self->source==target)self->Clear();
        }
        self->Finish(job,accepted);
    }
    void Finish(std::shared_ptr<Job> const& job,bool accepted){
        if(active!=job)return;active.reset();auto complete=std::move(job->request.complete);
        {Edit edit(*this);Notify(std::move(complete),accepted);}Pump();
    }
};
}
