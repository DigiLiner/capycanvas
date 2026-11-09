using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Windows.Forms;

public static class CapyHeldKeyWindow {
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window,out uint process);
    [DllImport("user32.dll")] static extern IntPtr GetThreadDesktop(uint thread);
    [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr window,uint flags);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern bool GetUserObjectInformation(IntPtr handle,int index,StringBuilder value,uint bytes,out uint needed);
    static readonly ManualResetEvent Ready=new ManualResetEvent(false);
    static Thread Worker;
    static Form Peer;
    static TextBox Input;
    static Exception Failure;
    public static IntPtr Window {get;private set;}
    sealed class Popup : Form {
        readonly IntPtr AppOwner;
        public Popup(IntPtr owner){AppOwner=owner;}
        protected override CreateParams CreateParams {
            get{
                var value=base.CreateParams;
                value.Style=(value.Style|unchecked((int)0x80000000))&~0x40000000;
                value.Parent=AppOwner;
                return value;
            }
        }
    }
    sealed class Owner : IWin32Window {
        public IntPtr Handle {get;private set;}
        public Owner(IntPtr handle){Handle=handle;}
    }
    static string Desktop(uint thread){
        var value=new StringBuilder(256);uint needed;
        return GetUserObjectInformation(GetThreadDesktop(thread),2,value,512,out needed)?value.ToString():null;
    }
    static void Verify(IntPtr window,uint process,string desktop){
        uint actual;uint thread=GetWindowThreadProcessId(window,out actual);
        if(!IsWindowVisible(window)||actual!=process||Desktop(thread)!=desktop)
            throw new InvalidOperationException("The held-key window is not owned on the private desktop.");
    }
    public static void Start(IntPtr owner,uint process,string desktop){
        if(string.IsNullOrWhiteSpace(desktop)||string.Equals(desktop,"Default",StringComparison.OrdinalIgnoreCase))
            throw new InvalidOperationException("A private desktop is required.");
        Verify(owner,process,desktop);
        if(Worker!=null)throw new InvalidOperationException("The held-key window is already running.");
        Worker=new Thread(()=>{
            try{
                if(Desktop(GetCurrentThreadId())!=desktop)throw new InvalidOperationException("The held-key thread is outside the private desktop.");
                Peer=new Popup(owner) {Text="Held-key fixture",Width=240,Height=100,StartPosition=FormStartPosition.Manual,Left=30,Top=30,ShowInTaskbar=false};
                Input=new TextBox {Dock=DockStyle.Fill};Peer.Controls.Add(Input);
                Peer.Shown+=delegate {Window=Peer.Handle;Ready.Set();};
                Peer.Show(new Owner(owner));
                Application.Run(Peer);
            }catch(Exception error){Failure=error;Ready.Set();}
            finally{if(Peer!=null)Peer.Dispose();}
        });
        Worker.IsBackground=true;Worker.SetApartmentState(ApartmentState.STA);Worker.Start();
        if(!Ready.WaitOne(10000))throw new TimeoutException("The held-key window did not open.");
        if(Failure!=null)throw new InvalidOperationException("The held-key window failed.",Failure);
        Check(owner,process,desktop);
    }
    public static void Focus(){Peer.Invoke(new Action(()=>{if(!Input.Focus())throw new InvalidOperationException("The held-key peer did not receive keyboard focus.");}));}
    public static void Check(IntPtr owner,uint process,string desktop){
        Verify(owner,process,desktop);Verify(Window,(uint)Process.GetCurrentProcess().Id,desktop);
        if(GetAncestor(Window,3)!=GetAncestor(owner,3))throw new InvalidOperationException("The held-key window has another root owner.");
    }
    public static void Stop(string desktop){
        if(Peer!=null&&!Peer.IsDisposed){
            Verify(Window,(uint)Process.GetCurrentProcess().Id,desktop);
            Peer.BeginInvoke(new Action(Peer.Close));
        }
        if(Worker!=null&&!Worker.Join(3000))throw new TimeoutException("The held-key window did not close.");
    }
}
