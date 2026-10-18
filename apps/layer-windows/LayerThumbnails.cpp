#include "pch.h"
#include "UiControls.h"
#include "LayerThumbnails.h"
#include "BitmapUpload.h"
#include <chrono>
#include <set>

namespace CapyUi {
namespace {
hstring slot(LayerThumbnail const& item){return item.layer+(item.mask?L":mask":L":content");}
hstring key(LayerThumbnail const& item){return slot(item)+L":"+item.target+L":"+item.revision;}
}
struct LayerThumbnailCache : std::enable_shared_from_this<LayerThumbnailCache> {
    PreviewTransport transport;
    Microsoft::UI::Dispatching::DispatcherQueue dispatcher{Microsoft::UI::Dispatching::DispatcherQueue::GetForCurrentThread()};
    struct Tile {hstring key,target;std::shared_ptr<BitmapUpload> upload;uint64_t used=0;};
    struct Readback {LayerThumbnail item;size_t offset;};
    std::map<hstring,Tile> images;
    std::map<hstring,LayerThumbnail> pending;
    std::map<hstring,hstring> wanted;
    hstring epoch;
    uint64_t serial=0,clock=0;
    bool busy=false;
    std::chrono::steady_clock::time_point last{};
    std::chrono::milliseconds wait{120};
    ~LayerThumbnailCache(){for(auto& [id,tile]:images)tile.upload->Close();}
    void upload(PreviewPacket packet,hstring const& generation,std::vector<Readback> const& readbacks){
        if(readbacks.empty()||readbacks.size()>8)return;
        auto left=std::make_shared<size_t>(readbacks.size());auto weak=weak_from_this();busy=true;
        for(auto const& readback:readbacks){
            auto id=slot(readback.item),revision=key(readback.item);
            if(!images.contains(id)){
                if(images.size()>=128){
                    auto oldest=std::min_element(images.begin(),images.end(),[](auto const& a,auto const& b){return a.second.used<b.second.used;});
                    oldest->second.upload->Close();images.erase(oldest);
                }
                images.emplace(id,Tile{L"",readback.item.target,BitmapUpload::Create(),++clock});
            }
            auto& tile=images.at(id);tile.key=L"";auto bitmap=tile.upload;std::weak_ptr<BitmapUpload> weakBitmap=bitmap;
            bitmap->Clear();
            bitmap->Submit({BitmapUpload::ReadPacket(packet),readback.offset,8*4096,32,32,true,false,
                [weak,generation,id,revision,weakBitmap]{
                    auto self=weak.lock();if(!self||self->epoch!=generation)return false;
                    auto found=self->images.find(id);auto wanted=self->wanted.find(id);
                    return found!=self->images.end()&&found->second.upload==weakBitmap.lock()&&wanted!=self->wanted.end()&&wanted->second==revision;
                },
                [weak,id,revision,weakBitmap,left](bool accepted){
                    if(auto self=weak.lock()){
                        if(!--*left)self->busy=false;
                        if(accepted){auto found=self->images.find(id);if(found!=self->images.end()&&found->second.upload==weakBitmap.lock()){found->second.key=revision;found->second.used=++self->clock;}}
                    }
                }});
        }
    }
    void receive(PreviewPacket packet,hstring const& generation,std::map<hstring,LayerThumbnail> const& proposed){
        busy=false;if(!packet||epoch!=generation)return;
        try{
            auto status=J::Parse(to_hstring(capy_preview_metadata(packet.get())));
            if(str(status,L"epoch")!=epoch)return;
            wait=std::chrono::milliseconds(std::max(8,int(num(status,L"wait_ms"))));
            for(auto value:array(status,L"accepted")){
                auto found=proposed.find(value.GetString());
                if(found!=proposed.end())pending.insert_or_assign(found->first,found->second);
            }
            std::vector<Readback> readbacks;
            for(auto value:array(status,L"images")){
                auto image=value.GetObject();auto found=pending.find(str(image,L"request"));
                if(found==pending.end())continue;
                auto item=found->second;pending.erase(found);
                if(num(image,L"width")!=32||num(image,L"height")!=32||num(image,L"length")!=4096)continue;
                double offset=num(image,L"offset",-1);
                if(offset<0||offset>7*4096||std::floor(offset)!=offset)continue;
                auto current=wanted.find(slot(item));
                if(current!=wanted.end()&&current->second==key(item))readbacks.push_back({item,size_t(offset)});
            }
            if(!readbacks.empty())upload(std::move(packet),generation,readbacks);
        }catch(hresult_error const& e){OutputDebugStringW(e.message().c_str());}
    }
    std::chrono::milliseconds refresh(hstring const& generation,std::vector<LayerThumbnail> const& visible){
        if(epoch!=generation){epoch=generation;pending.clear();wanted.clear();for(auto& [id,tile]:images)tile.upload->Close();images.clear();}
        bool needed=!pending.empty();
        wanted.clear();for(auto const& item:visible){
            auto id=slot(item),revision=key(item);wanted.insert_or_assign(id,revision);auto found=images.find(id);
            if(found!=images.end()&&found->second.target!=item.target){found->second.key=L"";found->second.target=item.target;found->second.upload->Clear();}
            needed|=found==images.end()||found->second.key!=revision;
        }
        auto now=std::chrono::steady_clock::now();
        if(!busy&&!needed)return std::chrono::milliseconds(120);
        if(busy||now-last<wait)return wait;
        A requests;std::map<hstring,LayerThumbnail> proposed;
        std::set<hstring> outstanding;for(auto const& [id,item]:pending)outstanding.insert(slot(item));
        for(auto const& item:visible){
            if(pending.size()+proposed.size()>=8)break;
            auto id=slot(item);auto found=images.find(id);
            if(outstanding.contains(id)||(found!=images.end()&&found->second.key==key(item)))continue;
            auto request=to_hstring(++serial);A pair;pair.Append(S(request));pair.Append(S(item.target));requests.Append(pair);
            proposed.emplace(request,item);outstanding.insert(id);
        }
        if(pending.empty()&&proposed.empty())return std::chrono::milliseconds(120);
        auto query=O({{L"epoch",S(epoch)},{L"requests",requests}});
        auto weak=weak_from_this();auto queue=dispatcher;busy=true;
        bool sent=transport(CanvasQueryKind::Thumbnails,to_string(query.Stringify()),
            [weak,queue,generation,proposed=std::move(proposed)](PreviewPacket packet){
                queue.TryEnqueue([weak,packet=std::move(packet),generation,proposed]{
                    if(auto self=weak.lock())self->receive(packet,generation,proposed);
                });
            });
        if(sent)last=now;else busy=false;
        return wait;
    }
};
std::shared_ptr<LayerThumbnailCache> CreateLayerThumbnailCache(PreviewTransport transport){
    auto cache=std::make_shared<LayerThumbnailCache>();cache->transport=std::move(transport);return cache;
}
std::chrono::milliseconds RefreshLayerThumbnails(std::shared_ptr<LayerThumbnailCache> const& cache,hstring const& epoch,std::vector<LayerThumbnail> const& visible){
    return cache?cache->refresh(epoch,visible):std::chrono::milliseconds(120);
}
ImageSource LayerThumbnailSource(std::shared_ptr<LayerThumbnailCache> const& cache,hstring const& epoch,LayerThumbnail const& item){
    if(cache&&cache->epoch==epoch){
        auto found=cache->images.find(slot(item));
        if(found!=cache->images.end()&&found->second.key==key(item)){found->second.used=++cache->clock;return found->second.upload->Image();}
    }return nullptr;
}
}
