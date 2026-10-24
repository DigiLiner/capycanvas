using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;

public static class CapyBubbleFixtureNative {
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern IntPtr LoadLibrary(string path);
 [DllImport("layer_windows.dll",CallingConvention=CallingConvention.Cdecl)] static extern IntPtr capy_localization_for_tag([MarshalAs(UnmanagedType.LPUTF8Str)]string tag);
 [DllImport("layer_windows.dll",CallingConvention=CallingConvention.Cdecl)] static extern IntPtr capy_bubble_ui(IntPtr context,[MarshalAs(UnmanagedType.LPUTF8Str)]string json);
 [DllImport("layer_windows.dll",CallingConvention=CallingConvention.Cdecl)] static extern void capy_localization_free(IntPtr context);
 [DllImport("layer_windows.dll",CallingConvention=CallingConvention.Cdecl)] static extern void capy_string_free(IntPtr value);
 [DllImport("layer_windows.dll",CallingConvention=CallingConvention.Cdecl)] static extern uint capy_color_hit(float x,float y,float size,uint shape);
 [StructLayout(LayoutKind.Sequential)] struct ScreenPoint {public int x,y;}
 [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(ScreenPoint point);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window,out uint process);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window,StringBuilder name,int count);
 public static string WindowAt(int x,int y){var window=WindowFromPoint(new ScreenPoint{x=x,y=y});uint process;GetWindowThreadProcessId(window,out process);var name=new StringBuilder(256);GetClassName(window,name,name.Capacity);return "hwnd="+window.ToInt64().ToString("X")+";class="+name+";process="+process;}
 static IntPtr library;
 static void EnsureLibrary(string path) {
  if(library==IntPtr.Zero){library=LoadLibrary(path);if(library==IntPtr.Zero)throw new Win32Exception(Marshal.GetLastWin32Error());}
 }
 public static uint ColorHit(string path,float x,float y,float size,uint shape) {
  EnsureLibrary(path);return capy_color_hit(x,y,size,shape);
 }
 public static string Query(string path,string json) {
  EnsureLibrary(path);
  IntPtr context=capy_localization_for_tag("en"),result=IntPtr.Zero;
  try{
   if(context==IntPtr.Zero)throw new InvalidOperationException("Fixture localization unavailable.");
   result=capy_bubble_ui(context,json);
   if(result==IntPtr.Zero)throw new InvalidOperationException("Shared bubble geometry query failed.");
   int count=0;while(Marshal.ReadByte(result,count)!=0)count++;
   byte[] bytes=new byte[count];Marshal.Copy(result,bytes,0,count);return Encoding.UTF8.GetString(bytes);
  }finally{if(result!=IntPtr.Zero)capy_string_free(result);if(context!=IntPtr.Zero)capy_localization_free(context);}
 }
}
