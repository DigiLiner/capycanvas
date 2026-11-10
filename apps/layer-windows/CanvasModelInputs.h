#pragma once
#include <algorithm>
#include <initializer_list>
#include <functional>
#include <optional>
#include <set>
#include <string>
#include <utility>
#include <vector>
#include <winrt/Windows.Data.Json.h>

class CanvasModelPaths {
public:
    using Path=std::vector<std::wstring>;
private:
    std::set<Path> paths;
    static bool contains(Path const& prefix,Path const& path){return prefix.size()<=path.size()&&std::equal(prefix.begin(),prefix.end(),path.begin());}
public:
    CanvasModelPaths(std::initializer_list<Path> initial={}){for(auto const& path:initial)Add(path);}
    void Add(Path const& path){
        auto at=paths.lower_bound(path);
        if(at!=paths.end()&&*at==path)return;
        if(at!=paths.begin()&&contains(*std::prev(at),path))return;
        auto end=at;while(end!=paths.end()&&contains(path,*end))++end;
        paths.emplace_hint(paths.erase(at,end),path);
    }
    void Merge(CanvasModelPaths const& other){for(auto const& path:other.paths)Add(path);}
    bool Touches(Path const& path)const{
        auto at=paths.lower_bound(path);
        return (at!=paths.end()&&contains(path,*at))||(at!=paths.begin()&&contains(*std::prev(at),path));
    }
    bool Intersects(CanvasModelPaths const& other)const{
        if(paths.size()>other.paths.size())return other.Intersects(*this);
        return std::any_of(paths.begin(),paths.end(),[&](auto const& path){return other.Touches(path);});
    }
};

struct CanvasModelChanges {
    using Path=CanvasModelPaths::Path;
    bool all=false;
    CanvasModelPaths paths;
    void Merge(CanvasModelChanges const& other){all|=other.all;paths.Merge(other.paths);}
    bool Touches(Path const& input)const{return all||paths.Touches(input);}
};

class CanvasModelInputs {
    using J=winrt::Windows::Data::Json::JsonObject;
    using Path=CanvasModelChanges::Path;
    CanvasModelPaths keys;
    bool captured=false;
public:
    class Capture {
        CanvasModelInputs& inputs;
        std::vector<std::pair<J,Path>> roots;
        Capture* previous;
        CanvasModelChanges const* changes;
        bool force;
        static inline thread_local Capture* current=nullptr;
        Capture(CanvasModelInputs& readSet,std::vector<std::pair<J,Path>> sources,CanvasModelChanges const* changed,bool refresh):
            inputs(readSet),roots(std::move(sources)),previous(std::exchange(current,this)),changes(changed),force(refresh){inputs.captured=true;}
    public:
        Capture(CanvasModelInputs& readSet,std::initializer_list<std::pair<J,Path>> sources,CanvasModelChanges const* changed=nullptr,bool refresh=false):
            Capture(readSet,std::vector<std::pair<J,Path>>(sources),changed,refresh){}
        Capture(Capture const&)=delete;
        ~Capture(){current=previous;}
        static bool Active(){return current!=nullptr;}
        static CanvasModelChanges const* Changes(){return current?current->changes:nullptr;}
        static void Merge(CanvasModelInputs const& readSet){if(current)current->inputs.keys.Merge(readSet.keys);}
        static void ReadPath(J const& object,Path const& suffix){
            if(!current)return;
            for(auto const& [root,prefix]:current->roots)if(object==root){
                auto path=prefix;path.insert(path.end(),suffix.begin(),suffix.end());
                current->inputs.keys.Add(path);
                break;
            }
        }
        static void Read(J const& object,wchar_t const* field){if(current)ReadPath(object,{field});}
        static void Alias(J const& object,J const& owner,Path const& suffix){
            if(!current||std::any_of(current->roots.begin(),current->roots.end(),[&](auto const& entry){return entry.first==object;}))return;
            for(auto const& [root,prefix]:current->roots)if(owner==root){auto path=prefix;path.insert(path.end(),suffix.begin(),suffix.end());current->roots.emplace_back(object,std::move(path));return;}
        }
        static bool Apply(CanvasModelInputs& readSet,std::function<void()> const& update,bool refresh=false){
            auto parent=current;
            if(!parent){update();return true;}
            bool changed=refresh||parent->force||!parent->changes||readSet.Changed(*parent->changes);
            if(changed){
                Capture capture(readSet,parent->roots,parent->changes,refresh||parent->force);update();
            }
            parent->inputs.keys.Merge(readSet.keys);return changed;
        }
    };
    class Presentation {
        CanvasModelInputs& inputs;
        std::optional<Capture> scope;
    public:
        Presentation(CanvasModelInputs& reads,std::initializer_list<std::pair<J,Path>> roots,CanvasModelChanges const* changes,bool force):inputs(reads){
            if(force||!changes||reads.Changed(*changes))scope.emplace(reads,roots,changes,force||!changes);
        }
        ~Presentation(){scope.reset();Capture::Merge(inputs);}
        explicit operator bool()const{return scope.has_value();}
    };
    void Add(Path path){keys.Add(path);}
    bool Changed(CanvasModelChanges const& changes)const{
        if(!captured||changes.all)return true;
        return keys.Intersects(changes.paths);
    }
};

class CanvasModelBinding {
    std::function<void()> update;
    mutable CanvasModelInputs inputs;
public:
    CanvasModelBinding()=default;
    template<class Update> CanvasModelBinding(Update callback):update(std::move(callback)){}
    explicit operator bool()const{return bool(update);}
    void operator()()const{update();}
    void Apply(bool force=false)const{CanvasModelInputs::Capture::Apply(inputs,update,force);}
};
