$ErrorActionPreference='Stop'
$repo=(Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
function Import-Function([string]$Path,[string]$Name,[string]$As){
 $tokens=$null;$errors=$null;$ast=[Management.Automation.Language.Parser]::ParseFile($Path,[ref]$tokens,[ref]$errors)
 if($errors.Count){throw 'Function source parse failed'}
 $functions=@($ast.FindAll({param($n)$n-is[Management.Automation.Language.FunctionDefinitionAst]-and$n.Name-eq$Name},$true))
 if($functions.Count-ne1){throw 'Expected exactly one actual function'}
 $body=$functions[0].Extent.Text
 Invoke-Expression ('function script:'+$As+$body.Substring(('function '+$Name).Length))
}
Import-Function (Join-Path $repo 'apps/layer-windows/scripts/CapyUia.ps1') 'Read-Snapshot' 'Read-Snapshot'
Import-Function (Join-Path $repo 'apps/layer-windows/scripts/exercise-tools.ps1') 'Model' 'Model'
Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
public sealed class CapyAtomicPublisher : IDisposable {
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool ReplaceFileW(string replaced,string replacement,string backup,uint flags,IntPtr exclude,IntPtr reserved);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool MoveFileExW(string existing,string target,uint flags);
 readonly string path; readonly byte[] data; readonly Thread thread; volatile bool stop;
 public long Published; public long Errors; public int LastError;
 public CapyAtomicPublisher(string path,string payload){this.path=path;data=new UTF8Encoding(false).GetBytes(payload);thread=new Thread(Run);thread.IsBackground=true;thread.Start();}
 void Run(){while(!stop){string pending=path+".pending";try{File.WriteAllBytes(pending,data);bool ok=ReplaceFileW(path,pending,null,0,IntPtr.Zero,IntPtr.Zero);int error=Marshal.GetLastWin32Error();if(!ok&&error==2){ok=MoveFileExW(pending,path,0);error=Marshal.GetLastWin32Error();}if(ok)Interlocked.Increment(ref Published);else{LastError=error;Interlocked.Increment(ref Errors);File.Delete(pending);}}catch(IOException){Interlocked.Increment(ref Errors);}}}
 public void Dispose(){stop=true;thread.Join();}
}
'@
$out=Join-Path $repo ('artifacts/windows/model-snapshot-'+[Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($out)|Out-Null
$ProcessId=$PID;$StateFile=Join-Path $out 'ui-state.json'
$payload=@{process_id=$ProcessId;model=@{state=@{camera=@{rotation=0;flipped=@($false,$false)}};padding=('x'*524288)}}|ConvertTo-Json -Depth 8 -Compress
[IO.File]::WriteAllText($StateFile,$payload)
$publisher=[CapyAtomicPublisher]::new($StateFile,$payload)
$reads=0;$nulls=0;$errors=0;$misclassified=0
try{
 $watch=[Diagnostics.Stopwatch]::StartNew()
 while($watch.Elapsed.TotalSeconds-lt10-and$reads-lt100){
  $reads++
  try{
   $value=Model
   if($null-eq$value){$nulls++}
   if($value.state.camera.rotation-ne0-or$value.state.camera.flipped[0]-or$value.state.camera.flipped[1]){$misclassified++}
  }catch{$errors++}
 }
}finally{$publisher.Dispose()}
$invalidOwnerRejected=$false
[IO.File]::WriteAllText($StateFile,('{"process_id":'+($PID+1)+',"model":{}}'))
try{$null=Model}catch{$invalidOwnerRejected=$_.Exception.Message-eq'Review snapshot process owner mismatch'}
$malformedRejected=$false
[IO.File]::WriteAllText($StateFile,'{invalid')
try{$null=Model}catch{$malformedRejected=$true}
$missingRejected=$false
Remove-Item -LiteralPath $StateFile
try{$null=Model}catch{$missingRejected=$true}
$pass=$reads-eq100-and$nulls-eq0-and$errors-eq0-and$misclassified-eq0-and$publisher.Published-gt0-and$invalidOwnerRejected-and$malformedRejected-and$missingRejected
@{pass=$pass;reads=$reads;nulls=$nulls;read_errors=$errors;false_rotation_preconditions=$misclassified;published=$publisher.Published;replace_errors=$publisher.Errors;last_replace_error=$publisher.LastError;invalid_owner_rejected=$invalidOwnerRejected;malformed_rejected=$malformedRejected;missing_rejected=$missingRejected;writer_joined=$true;pid=$PID}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $out 'result.json')
Get-Content -LiteralPath (Join-Path $out 'result.json')
if(!$pass){throw 'Atomic snapshot model reader regression failed'}
