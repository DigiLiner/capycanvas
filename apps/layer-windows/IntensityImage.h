#pragma once
#include "ColorWheel.h"

namespace CapyWheel {
struct IntensityImage {
    std::shared_ptr<Device> device;
    std::shared_ptr<FieldWorker> worker;
    std::optional<FieldResult> ready;
    Imaging::SurfaceImageSource surface{nullptr};
    com_ptr<ID2D1Bitmap> bitmap;
    hstring layoutKey,bitmapKey,drawnLayout;
    uint64_t epoch=0;
    int pixels=0;
    Point2 bitmapCenter{};
    float bitmapRadius=0,bitmapSize=0;
    ~IntensityImage(){if(worker){worker->deliver=nullptr;worker->cancel();}}
    void start(std::function<void()> changed){
        worker=std::make_shared<FieldWorker>();
        worker->deliver=[this,changed](FieldResult result){ready=std::move(result);changed();};
    }
    void invalidate(){surface=nullptr;}
    void present(Image const& image,J const& intensity,CapyUi::Point origin,double size,double scale){
        try{draw(image,intensity,origin,size,scale);}
        catch(hresult_error const& error){
            auto code=error.code();
            if(code!=DXGI_ERROR_DEVICE_REMOVED&&code!=DXGI_ERROR_DEVICE_RESET&&code!=D2DERR_RECREATE_TARGET&&code!=E_SURFACE_CONTENTS_LOST)throw;
            draw(image,intensity,origin,size,scale,true);
        }
    }
    void draw(Image const& image,J const& intensity,CapyUi::Point origin,double size,double scale,bool reset=false){
        if(size<=0||scale<=0)return;
        if(reset){surface=nullptr;device.reset();}
        if(!device||FAILED(device->d3d->GetDeviceRemovedReason())){
            device=Device::get();surface=nullptr;bitmap=nullptr;bitmapKey=L"";++epoch;ready.reset();worker->cancel();
        }
        auto geometry=object(intensity,L"geometry");auto center=point(array(geometry,L"center"));center.x+=origin.X;center.y+=origin.Y;
        auto radius=float(num(geometry,L"radius"));auto ramp=array(intensity,L"ramp");
        auto layout=geometry.Stringify()+array(intensity,L"range").Stringify()+L":"+to_hstring(origin.X)+L":"+to_hstring(origin.Y)+L":"+to_hstring(size)+L":"+to_hstring(scale);
        if(layout!=layoutKey){layoutKey=layout;++epoch;ready.reset();worker->cancel();}
        auto wanted=layout+ramp.Stringify();bool changed=false;
        if(auto result=std::exchange(ready,std::nullopt);result&&result->epoch==epoch&&result->device==device&&(bitmapKey!=wanted||result->key==wanted)){
            check_hresult(result->status);bitmap=result->bitmap;bitmapKey=result->key;bitmapCenter=center;bitmapRadius=radius;bitmapSize=float(size);changed=true;
        }
        int next=std::max(1,int(std::ceil(size*scale)));
        if(bitmapKey!=wanted){
            auto arc=std::make_shared<ArcRaster>();arc->center=center;arc->radius=radius;arc->width=float(num(geometry,L"width"));arc->endAngle=float(num(geometry,L"end_angle"));arc->size=float(size);
            arc->ramp.reserve(ramp.Size());for(auto value:ramp)arc->ramp.push_back(paint(value.GetArray()));
            worker->submit(FieldRequest{wanted,uint32_t(next),0,0,0,false,false,{},epoch,device,arc});
        }
        if(!surface||pixels!=next){
            pixels=next;surface=Imaging::SurfaceImageSource(pixels,pixels,false);
            check_hresult(surface.as<ISurfaceImageSourceNativeWithD2D>()->SetDevice(device->d2d.get()));image.Source(surface);changed=true;
        }
        if(!changed&&drawnLayout==layout)return;
        auto native=surface.as<ISurfaceImageSourceNativeWithD2D>();com_ptr<ID2D1DeviceContext> context;POINT offset{};
        check_hresult(native->BeginDraw(RECT{0,0,pixels,pixels},__uuidof(ID2D1DeviceContext),context.put_void(),&offset));
        bool active=true;
        try{
            context->SetDpi(96,96);context->SetPrimitiveBlend(D2D1_PRIMITIVE_BLEND_SOURCE_OVER);
            auto dpi=float(pixels/size);context->SetTransform(D2D1::Matrix3x2F::Scale(dpi,dpi)*D2D1::Matrix3x2F::Translation(float(offset.x),float(offset.y)));
            context->Clear(D2D1::ColorF(0,0,0,0));
            if(bitmap&&bitmapRadius>0){auto ratio=radius/bitmapRadius;auto x=center.x-bitmapCenter.x*ratio,y=center.y-bitmapCenter.y*ratio,side=bitmapSize*ratio;
                context->DrawBitmap(bitmap.get(),D2D1::RectF(x,y,x+side,y+side),1,D2D1_BITMAP_INTERPOLATION_MODE_LINEAR);}
            active=false;check_hresult(native->EndDraw());drawnLayout=layout;
        }catch(...){if(active)native->EndDraw();throw;}
    }
};
}
