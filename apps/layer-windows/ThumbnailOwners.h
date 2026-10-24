#pragma once
#include <algorithm>
#include <list>
#include <map>
#include <utility>

template<class Key,class Target>
class ThumbnailOwners {
    using Targets=std::map<Key,Target>;
    std::list<std::pair<void const*,Targets>> owners;
    Targets wanted;
    void combine(){wanted.clear();for(auto const& [owner,targets]:owners)for(auto const& [key,target]:targets)wanted.insert_or_assign(key,target);}
public:
    void Update(void const* owner,Targets targets){
        if(targets.empty()){Forget(owner);return;}
        auto found=std::find_if(owners.begin(),owners.end(),[owner](auto const& entry){return entry.first==owner;});
        if(found==owners.end())owners.emplace_back(owner,std::move(targets));
        else{if(found->second==targets)return;found->second=std::move(targets);owners.splice(owners.end(),owners,found);}
        combine();
    }
    void Forget(void const* owner){if(std::erase_if(owners,[owner](auto const& entry){return entry.first==owner;}))combine();}
    void Clear(){owners.clear();wanted.clear();}
    Targets const& Items()const{return wanted;}
};
