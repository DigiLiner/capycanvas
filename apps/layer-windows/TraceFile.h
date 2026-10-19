#pragma once
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <filesystem>
#include <fstream>
#include <cstring>
#include <string>
#include <vector>

inline void WriteTraceFile(std::filesystem::path const& name,std::string const& value){
    auto pending=name.wstring()+L".pending";
    {std::ofstream stream(pending);stream<<value;if(!stream)return;}
    auto target=std::filesystem::absolute(name).wstring();
    auto size=sizeof(FILE_RENAME_INFO)+target.size()*sizeof(wchar_t);
    std::vector<FILE_RENAME_INFO> storage((size+sizeof(FILE_RENAME_INFO)-1)/sizeof(FILE_RENAME_INFO));
    auto info=storage.data();
    info->Flags=FILE_RENAME_FLAG_REPLACE_IF_EXISTS|FILE_RENAME_FLAG_POSIX_SEMANTICS;
    info->FileNameLength=static_cast<DWORD>(target.size()*sizeof(wchar_t));
    std::memcpy(info->FileName,target.c_str(),(target.size()+1)*sizeof(wchar_t));
    auto source=CreateFileW(pending.c_str(),DELETE,FILE_SHARE_READ|FILE_SHARE_WRITE|FILE_SHARE_DELETE,nullptr,OPEN_EXISTING,FILE_ATTRIBUTE_NORMAL,nullptr);
    auto published=source!=INVALID_HANDLE_VALUE&&SetFileInformationByHandle(source,FileRenameInfoEx,info,static_cast<DWORD>(size));
    auto error=GetLastError();
    if(source!=INVALID_HANDLE_VALUE)CloseHandle(source);
    if(!published){
        DeleteFileW(pending.c_str());
        OutputDebugStringW((std::wstring(L"Capy trace replacement failed: ")+std::to_wstring(error)+L"\n").c_str());
    }
}
