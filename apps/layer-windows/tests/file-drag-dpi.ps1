param([string]$Output,[switch]$TypeSourceOnly)
$ErrorActionPreference='Stop'
$fixture=Join-Path $PSScriptRoot '../scripts/exercise-file-activation.ps1'
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($fixture,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'File activation fixture did not parse'}
$definitions=@($ast.FindAll({param($node) $node -is [Management.Automation.Language.CommandAst] -and $node.GetCommandName() -eq 'Add-Type'},$true)|ForEach-Object {
    foreach($element in $_.CommandElements){if(($element -is [Management.Automation.Language.StringConstantExpressionAst] -or $element -is [Management.Automation.Language.ExpandableStringExpressionAst]) -and $element.Value.Contains('public static class CapyFileDrag')){$element.Value}}
})
if($definitions.Count -ne 1){throw 'File activation drag type must have one definition'}
$drag=$definitions[0]
$send='[DllImport("user32.dll",SetLastError=true)] static extern uint SendInput(uint count,Input[] input,int size);'
if(!$drag.Contains($send)){throw 'File drag native input boundary declaration changed'}
$drag=$drag.Replace($send,'static uint SendInput(uint count,Input[] input,int size){if(count!=1)throw new Exception("Unexpected input packet count");return CapyFileDragDpiProbe.Observe(input[0].mouse.dx,input[0].mouse.dy,input[0].mouse.flags);}')
$probe=@'
public sealed class CapyFileDragDpiSample {
 public uint ThreadId;
 public long Context,ProcessContext;
 public int Awareness,ProcessAwareness,X,Y;
 public uint Flags;
 public int[] Metrics,PhysicalMetrics;
}
public static class CapyFileDragDpiProbe {
 [System.Runtime.InteropServices.DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
 [System.Runtime.InteropServices.DllImport("kernel32.dll")] static extern System.IntPtr GetCurrentProcess();
 [System.Runtime.InteropServices.DllImport("user32.dll")] static extern System.IntPtr GetDpiAwarenessContextForProcess(System.IntPtr process);
 [System.Runtime.InteropServices.DllImport("user32.dll")] public static extern System.IntPtr GetThreadDpiAwarenessContext();
 [System.Runtime.InteropServices.DllImport("user32.dll")] static extern int GetAwarenessFromDpiAwarenessContext(System.IntPtr context);
 [System.Runtime.InteropServices.DllImport("user32.dll")] public static extern bool AreDpiAwarenessContextsEqual(System.IntPtr first,System.IntPtr second);
 [System.Runtime.InteropServices.DllImport("user32.dll")] public static extern System.IntPtr SetThreadDpiAwarenessContext(System.IntPtr context);
 [System.Runtime.InteropServices.DllImport("user32.dll")] static extern int GetSystemMetrics(int code);
 [System.Runtime.InteropServices.DllImport("user32.dll")] public static extern uint GetDpiForWindow(System.IntPtr window);
 [System.Runtime.InteropServices.DllImport("user32.dll")] public static extern uint GetDpiForSystem();
 static readonly System.Collections.Generic.List<CapyFileDragDpiSample> samples=new System.Collections.Generic.List<CapyFileDragDpiSample>();
 static int[] Metrics(){return new[]{GetSystemMetrics(76),GetSystemMetrics(77),GetSystemMetrics(78),GetSystemMetrics(79)};}
 public static CapyFileDragDpiSample Measure(){
  var context=GetThreadDpiAwarenessContext();var process=GetDpiAwarenessContextForProcess(GetCurrentProcess());var metrics=Metrics();int[] physical;
  var previous=SetThreadDpiAwarenessContext(new System.IntPtr(-4));
  if(previous==System.IntPtr.Zero)throw new System.ComponentModel.Win32Exception(System.Runtime.InteropServices.Marshal.GetLastWin32Error());
  try{physical=Metrics();}finally{if(SetThreadDpiAwarenessContext(previous)==System.IntPtr.Zero)throw new System.Exception("DPI sample did not restore its thread context");}
  return new CapyFileDragDpiSample{ThreadId=GetCurrentThreadId(),Context=context.ToInt64(),Awareness=GetAwarenessFromDpiAwarenessContext(context),ProcessContext=process.ToInt64(),ProcessAwareness=GetAwarenessFromDpiAwarenessContext(process),Metrics=metrics,PhysicalMetrics=physical};
 }
 public static uint Observe(int x,int y,uint flags){
  var sample=Measure();sample.X=x;sample.Y=y;sample.Flags=flags;lock(samples)samples.Add(sample);return 1;
 }
 public static void Reset(){lock(samples)samples.Clear();}
 public static CapyFileDragDpiSample[] Snapshot(){lock(samples)return samples.ToArray();}
}
'@
$typeDefinition=$drag+"`n"+$probe
if($TypeSourceOnly){return $typeDefinition}
if(!$Output){throw 'File drag DPI test requires an output receipt path'}
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,System.Drawing
if('CapyFileDrag' -as [type]){throw 'Run the real file drag DPI check in a fresh PowerShell process'}
Add-Type -TypeDefinition $typeDefinition
$priorContext=[CapyFileDragDpiProbe]::GetThreadDpiAwarenessContext()
$previous=[CapyFileDragDpiProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
if($previous -eq [IntPtr]::Zero){throw 'File drag DPI test could not acquire physical caller coordinates'}
$failure=$null;$receipt=$null;$shell=$null
try{
    $physical=[CapyFileDragDpiProbe]::Measure().PhysicalMetrics
    if($physical[2] -le 1 -or $physical[3] -le 1){throw 'File drag DPI test has no virtual display geometry'}
    $x=$physical[0]+[int]($physical[2]/2);$y=$physical[1]+[int]($physical[3]/2)
    $formsPreloaded=@([AppDomain]::CurrentDomain.GetAssemblies()|Where-Object {$_.GetName().Name -eq 'System.Windows.Forms'}).Count -ne 0
    $shell=[powershell]::Create();$shell.Runspace=[runspacefactory]::CreateRunspace();$shell.Runspace.ApartmentState='STA';$shell.Runspace.Open()
    $null=$shell.AddScript({param($X,$Y)
        $ErrorActionPreference='Stop'
        $stages=[ordered]@{before_forms_load=[CapyFileDragDpiProbe]::Measure()}
        Add-Type -AssemblyName System.Windows.Forms,System.Drawing
        $stages['after_forms_load']=[CapyFileDragDpiProbe]::Measure()
        $prior=[CapyFileDragDpiProbe]::GetThreadDpiAwarenessContext()
        $saved=[CapyFileDrag]::SetThreadDpiAwarenessContext([IntPtr](-4))
        if($saved -eq [IntPtr]::Zero){throw 'File drag STA test could not acquire physical coordinates'}
        $source=$null
        try{
            $stages['before_form_creation']=[CapyFileDragDpiProbe]::Measure()
            $source=[Windows.Forms.Form]@{FormBorderStyle='None';ShowInTaskbar=$false;TopMost=$true;StartPosition='Manual';Bounds=[Drawing.Rectangle]::new($X-40,$Y+160,80,80)}
            $stages['after_form_creation']=[CapyFileDragDpiProbe]::Measure()
            $source.Show();[Windows.Forms.Application]::DoEvents()
            $stages['after_form_show']=[CapyFileDragDpiProbe]::Measure()
            $sourceDpi=[CapyFileDragDpiProbe]::GetDpiForWindow($source.Handle);$systemDpi=[CapyFileDragDpiProbe]::GetDpiForSystem()
            if(!$sourceDpi -or !$systemDpi){throw 'File drag DPI test did not measure its source window and system DPI'}
            [CapyFileDragDpiProbe]::Reset()
            $sendMethod=[CapyFileDrag].GetMethod('Send',[Reflection.BindingFlags]'NonPublic,Static')
            $sendMethod.Invoke($null,[object[]]@([uint32]0,[int]$X,[int]$Y))|Out-Null
            $reference=[CapyFileDragDpiProbe]::Snapshot()[0]
            $glide=[CapyFileDrag]::Press($source.Handle,$X,$Y);[Windows.Forms.Application]::DoEvents()
            if(!$glide.Join(10000)){throw 'The real file drag glide thread did not release'}
            if([CapyFileDrag]::Failure){throw [CapyFileDrag]::Failure}
            [pscustomobject]@{stages=$stages;source_window_dpi=$sourceDpi;system_dpi=$systemDpi;reference=$reference;samples=@([CapyFileDragDpiProbe]::Snapshot())}
        } finally {
            if($source){$source.Close();$source.Dispose()}
            if([CapyFileDrag]::SetThreadDpiAwarenessContext($saved) -eq [IntPtr]::Zero -or ![CapyFileDragDpiProbe]::AreDpiAwarenessContextsEqual($prior,[CapyFileDragDpiProbe]::GetThreadDpiAwarenessContext())){throw 'File drag DPI test did not restore its STA context'}
        }
    }).AddArgument($x).AddArgument($y)
    $result=@($shell.Invoke());if($shell.HadErrors){throw ($shell.Streams.Error|Out-String)}
    if($result.Count -ne 1){throw 'File drag STA path did not return one measurement receipt'}
    $reference=$result[0].reference
    $worker=@($result[0].samples|Where-Object {$_.ThreadId -ne $reference.ThreadId})
    if(!$worker.Count -or @($worker.ThreadId|Select-Object -Unique).Count -ne 1){throw 'The real file drag glide path did not create one distinct native worker'}
    $release=$worker[-1]
    $metricsMatch=@($worker|Where-Object {($_.Metrics -join ',') -cne ($_.PhysicalMetrics -join ',')}).Count -eq 0
    $tupleMatches=$release.X -eq $reference.X -and $release.Y -eq $reference.Y
    $receipt=[ordered]@{powershell=$PSVersionTable.PSVersion.ToString();fixture_sha256=(Get-FileHash -LiteralPath $fixture -Algorithm SHA256).Hash;forms_preloaded=$formsPreloaded;source_window_dpi=$result[0].source_window_dpi;system_dpi=$result[0].system_dpi;source_window_scale=([double]$result[0].source_window_dpi/96);requested=@($x,$y);stages=$result[0].stages;reference=$reference;worker=$worker;worker_metrics_are_physical=$metricsMatch;worker_release_matches_caller_tuple=$tupleMatches;desktop_input=$false;real_source_form=$true}
    if(!$metricsMatch -or !$tupleMatches){throw 'File drag glide worker changed the physical coordinate space used by its caller'}
} catch {
    $failure=$_
} finally {
    if($shell){$shell.Runspace.Dispose();$shell.Dispose()}
    if([CapyFileDragDpiProbe]::SetThreadDpiAwarenessContext($previous) -eq [IntPtr]::Zero){throw 'File drag DPI test did not restore its caller context'}
    if(![CapyFileDragDpiProbe]::AreDpiAwarenessContextsEqual($priorContext,[CapyFileDragDpiProbe]::GetThreadDpiAwarenessContext())){throw 'File drag DPI test changed its caller context'}
    if($receipt){$receipt['caller_context_restored']=$true;$receipt['sta_context_restored']=$true;$receipt|ConvertTo-Json -Depth 8|Set-Content -LiteralPath $Output -Encoding UTF8}
}
if($failure){throw $failure}
Write-Output 'File drag real STA Form and glide-thread physical coordinates passed without desktop input'
