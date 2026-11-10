#pragma once
#include <cstdint>
#include "CanvasModelInputs.h"
#include <initializer_list>
#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>
#include <winrt/Windows.Foundation.Collections.h>
#include <winrt/Windows.Data.Json.h>

namespace CapyUi {
using winrt::hstring;
using winrt::Windows::Data::Json::JsonValue;
using winrt::Windows::Data::Json::JsonValueType;
using J=winrt::Windows::Data::Json::JsonObject;
using A=winrt::Windows::Data::Json::JsonArray;
using V=winrt::Windows::Data::Json::IJsonValue;

inline std::string CameraTracePayload(winrt::Windows::Data::Json::JsonObject const& packet,bool full,std::string const& identity){
    using namespace winrt::Windows::Data::Json;
    auto source=full?packet.GetNamedObject(L"state",JsonObject{}):packet;
    auto camera=source.GetNamedObject(L"camera",JsonObject{});
    if(!camera.Size())return {};
    auto result=identity+",\"camera\":"+winrt::to_string(camera.Stringify());
    auto tabValue=packet.GetNamedValue(L"windows_tabs",JsonValue::CreateNullValue());
    if(tabValue.ValueType()!=JsonValueType::Object)return result+"}";
    auto tabs=tabValue.GetObject();
    auto selected=tabs.GetNamedNumber(L"selected",0);
    for(auto value:tabs.GetNamedArray(L"session_stamps",JsonArray{})){
        auto entry=value.GetObject();
        if(entry.GetNamedNumber(L"id",-1)!=selected)continue;
        auto stamp=entry.GetNamedObject(L"stamp");
        JsonObject navigation;
        navigation.Insert(L"drawing_id",entry.GetNamedValue(L"id"));
        navigation.Insert(L"artwork",stamp.GetNamedValue(L"artwork"));
        navigation.Insert(L"epoch",stamp.GetNamedValue(L"epoch"));
        navigation.Insert(L"revision",stamp.GetNamedValue(L"camera_navigation"));
        result+=",\"navigation\":"+winrt::to_string(navigation.Stringify());
        break;
    }
    return result+"}";
}
inline bool JsonEqual(winrt::Windows::Data::Json::IJsonValue const& left,winrt::Windows::Data::Json::IJsonValue const& right){
    using Type=winrt::Windows::Data::Json::JsonValueType;
    if(!left||!right)return !left&&!right;
    if(left.ValueType()!=right.ValueType())return false;
    switch(left.ValueType()){
    case Type::Object:{
        auto a=left.GetObject(),b=right.GetObject();if(a.Size()!=b.Size())return false;
        for(auto const& entry:a)if(!b.HasKey(entry.Key())||!JsonEqual(entry.Value(),b.Lookup(entry.Key())))return false;
        return true;
    }
    case Type::Array:{
        auto a=left.GetArray(),b=right.GetArray();if(a.Size()!=b.Size())return false;
        for(uint32_t i=0;i<a.Size();++i)if(!JsonEqual(a.GetAt(i),b.GetAt(i)))return false;
        return true;
    }
    case Type::String:return left.GetString()==right.GetString();
    case Type::Number:return left.GetNumber()==right.GetNumber();
    case Type::Boolean:return left.GetBoolean()==right.GetBoolean();
    case Type::Null:return true;
    default:return false;
    }
}
inline bool ApplyColorPreview(winrt::Windows::Data::Json::JsonObject const& packet,winrt::Windows::Data::Json::JsonObject& preview){
    using namespace winrt::Windows::Data::Json;
    if(packet.HasKey(L"color_preview")){auto value=packet.GetNamedValue(L"color_preview");preview=value.ValueType()==JsonValueType::Object?value.GetObject():JsonObject{};return true;}
    return false;
}
inline std::string JsonUnsignedFields(std::string object,std::initializer_list<std::pair<std::string_view,uint64_t>> fields){
    if(object.empty()||object.front()!='{'||object.back()!='}')throw std::invalid_argument("Expected a JSON object");
    object.pop_back();
    for(auto const& [key,value]:fields){
        if(object.size()>1)object+=',';
        object+='"';object+=key;object+="\":";object+=std::to_string(value);
    }
    object+='}';return object;
}
inline V named(J const& value,wchar_t const* key){CanvasModelInputs::Capture::Read(value,key);return value.GetNamedValue(key,JsonValue::CreateNullValue());}
inline J object(J const& value,wchar_t const* key){
    auto item=named(value,key);
    auto result=item.ValueType()==JsonValueType::Object?item.GetObject():J{};if(CanvasModelInputs::Capture::Active())CanvasModelInputs::Capture::Alias(result,value,{key});return result;
}
inline J branch(J const& value,wchar_t const* key){
    auto item=value.GetNamedValue(key,JsonValue::CreateNullValue());auto result=item.ValueType()==JsonValueType::Object?item.GetObject():J{};
    if(CanvasModelInputs::Capture::Active()){if(!result.Size())CanvasModelInputs::Capture::Read(value,key);CanvasModelInputs::Capture::Alias(result,value,{key});}
    return result;
}
inline A array(J const& value,wchar_t const* key){
    auto item=named(value,key);
    return item.ValueType()==JsonValueType::Array?item.GetArray():A{};
}
inline uint32_t count(J const& value,wchar_t const* field){
    if(CanvasModelInputs::Capture::Active())CanvasModelInputs::Capture::ReadPath(value,{field,L"length"});
    return value.GetNamedArray(field,A{}).Size();
}
inline J objectAt(A const& values,int32_t index){
    if(index<0||uint32_t(index)>=values.Size())return {};auto item=values.GetAt(index);return item.ValueType()==JsonValueType::Object?item.GetObject():J{};
}
inline hstring str(J const& value,wchar_t const* key,hstring fallback=L""){
    auto item=named(value,key);
    return item.ValueType()==JsonValueType::String?item.GetString():fallback;
}
inline double num(J const& value,wchar_t const* key,double fallback=0){
    auto item=named(value,key);
    return item.ValueType()==JsonValueType::Number?item.GetNumber():fallback;
}
inline bool flag(J const& value,wchar_t const* key,bool fallback=false){CanvasModelInputs::Capture::Read(value,key);return value.GetNamedBoolean(key,fallback);}
inline J find(A const& list,wchar_t const* key,hstring const& id){
    for(auto value:list){auto row=value.GetObject();if(str(row,key)==id)return row;}return J{};
}
inline J find(J const& model,wchar_t const* field,wchar_t const* key,hstring const& id){
    auto list=model.GetNamedArray(field,A{});
    bool recording=CanvasModelInputs::Capture::Active();
    for(uint32_t i=0;i<list.Size();++i){auto row=list.GetObjectAt(i);if(recording)CanvasModelInputs::Capture::ReadPath(model,{field,std::to_wstring(i),key});if(str(row,key)==id){if(recording)CanvasModelInputs::Capture::Alias(row,model,{field,std::to_wstring(i)});return row;}}
    CanvasModelInputs::Capture::Read(model,field);return {};
}
inline J findId(J const& model,wchar_t const* field,double id){
    auto list=model.GetNamedArray(field,A{});
    bool recording=CanvasModelInputs::Capture::Active();
    for(uint32_t i=0;i<list.Size();++i){auto row=list.GetObjectAt(i);if(recording)CanvasModelInputs::Capture::ReadPath(model,{field,std::to_wstring(i),L"id"});if(num(row,L"id")==id){if(recording)CanvasModelInputs::Capture::Alias(row,model,{field,std::to_wstring(i)});return row;}}
    CanvasModelInputs::Capture::Read(model,field);return {};
}
inline double num(J const& model,std::initializer_list<wchar_t const*> fields,double fallback=0){
    if(CanvasModelInputs::Capture::Active()){CanvasModelChanges::Path path;for(auto field:fields)path.emplace_back(field);CanvasModelInputs::Capture::ReadPath(model,path);}
    auto current=model;V value=JsonValue::CreateNullValue();
    for(auto it=fields.begin();it!=fields.end();++it){value=current.GetNamedValue(*it,JsonValue::CreateNullValue());if(it+1!=fields.end()){if(value.ValueType()!=JsonValueType::Object)return fallback;current=value.GetObject();}}
    return value.ValueType()==JsonValueType::Number?value.GetNumber():fallback;
}
inline J findId(A const& list,double id){
    for(auto value:list){auto row=value.GetObject();if(num(row,L"id")==id)return row;}return J{};
}
}
