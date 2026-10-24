using System;
using System.IO;
using System.Runtime.Serialization;
using System.Runtime.Serialization.Json;
using System.Text;
using System.Threading;
using System.Diagnostics;
using System.Windows.Automation;
using System.Runtime.InteropServices;
public sealed class CapyBubbleFoldObserver : IDisposable {
 [DataContract] sealed class Snapshot {[DataMember(Name="model")] public Model Model {get;set;}}
 [DataContract] sealed class Model {[DataMember(Name="bubbles")] public Packet[] Bubbles {get;set;}}
 [DataContract] sealed class Packet {[DataMember(Name="view")] public View View {get;set;}}
 [DataContract] sealed class View {[DataMember(Name="id")] public long Id {get;set;}[DataMember(Name="card")] public Card Card {get;set;}}
 [DataContract] sealed class Card {}
 readonly string path;readonly long id;readonly Thread thread;readonly ManualResetEventSlim ready=new ManualResetEventSlim();
 volatile bool stopping;public volatile bool OpenSeen,FoldSeen;public string FoldSnapshot;public int Samples;
 volatile AutomationElement nativeCard;readonly ManualResetEvent nativeFold=new ManualResetEvent(false);readonly ManualResetEventSlim nativeReady=new ManualResetEventSlim();
 public bool NativeOpenSeen {get;private set;} public long NativeLastOpenTicks {get;private set;} public long NativeFoldDetectedTicks {get;private set;} public WaitHandle NativeFoldSignal {get{return nativeFold;}}
 public CapyBubbleFoldObserver(string stateFile,long bubble){path=stateFile;id=bubble;thread=new Thread(Read){IsBackground=true};thread.Start();if(!ready.Wait(2000)){stopping=true;thread.Join();ready.Dispose();throw new Exception("Fold observer did not read an initial snapshot.");}}
 public CapyBubbleFoldObserver(string stateFile,long bubble,AutomationElement card){path=stateFile;id=bubble;nativeCard=card;thread=new Thread(ReadNative){IsBackground=true};thread.Start();if(!nativeReady.Wait(2000)){Dispose();throw new Exception("Native fold observer did not observe the open card.");}}
 void Read(){
  var serializer=new DataContractJsonSerializer(typeof(Snapshot));
  while(!stopping){try{
   string text;using(var stream=new FileStream(path,FileMode.Open,FileAccess.Read,FileShare.ReadWrite|FileShare.Delete))using(var reader=new StreamReader(stream))text=reader.ReadToEnd();
   Snapshot snapshot;using(var stream=new MemoryStream(Encoding.UTF8.GetBytes(text)))snapshot=(Snapshot)serializer.ReadObject(stream);
   if(snapshot!=null&&snapshot.Model!=null&&snapshot.Model.Bubbles!=null)foreach(var packet in snapshot.Model.Bubbles){
    var view=packet==null?null:packet.View;if(view==null||view.Id!=id)continue;
    ++Samples;if(view.Card!=null)OpenSeen=true;else if(OpenSeen){FoldSnapshot=text;FoldSeen=true;}ready.Set();break;
   }
  }catch(IOException){}catch(SerializationException){}
  Thread.Sleep(1);}
 }
 void ReadNative(){
  while(!stopping){++Samples;
  if(nativeCard!=null)try{
   long sample=Stopwatch.GetTimestamp();bool visible=!nativeCard.Current.IsOffscreen&&TreeWalker.RawViewWalker.GetParent(nativeCard)!=null;
   if(visible&&NativeFoldDetectedTicks==0){NativeOpenSeen=true;NativeLastOpenTicks=sample;nativeReady.Set();}else if(!visible&&NativeOpenSeen&&NativeFoldDetectedTicks==0){NativeFoldDetectedTicks=Stopwatch.GetTimestamp();nativeFold.Set();}
  }catch(ElementNotAvailableException){if(NativeOpenSeen&&NativeFoldDetectedTicks==0){NativeFoldDetectedTicks=Stopwatch.GetTimestamp();nativeFold.Set();}}catch(COMException){}
  if(NativeFoldDetectedTicks!=0)return;Thread.Sleep(1);}
 }
 public void Dispose(){stopping=true;thread.Join();ready.Dispose();nativeReady.Dispose();nativeFold.Dispose();}
}
