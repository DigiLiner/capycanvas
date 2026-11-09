$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'session-view.ps1')
if(!('CapySessionViewTest' -as [type])){Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Principal;
using Microsoft.Win32.SafeHandles;
public static class CapySessionViewTest {
 [StructLayout(LayoutKind.Sequential)] struct FileInfo {
  public uint Attributes,CreationLow,CreationHigh,AccessLow,AccessHigh,WriteLow,WriteHigh,Volume,SizeHigh,SizeLow,Links,IndexHigh,IndexLow;
 }
 [StructLayout(LayoutKind.Sequential)] struct SidAndAttributes { public IntPtr Sid;public uint Attributes; }
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetFileInformationByHandle(SafeFileHandle file,out FileInfo info);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern SafeFileHandle CreateFileW(string path,uint access,uint share,IntPtr security,uint creation,uint flags,IntPtr template);
 [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
 [DllImport("advapi32.dll",SetLastError=true)] static extern bool OpenProcessToken(IntPtr process,uint access,out IntPtr token);
 [DllImport("advapi32.dll",SetLastError=true)] static extern bool CreateRestrictedToken(IntPtr original,uint flags,uint disableCount,IntPtr disabled,uint deleteCount,IntPtr deleted,uint restrictCount,IntPtr restricted,out IntPtr token);
 [DllImport("advapi32.dll",SetLastError=true)] static extern bool ImpersonateLoggedOnUser(IntPtr token);
 [DllImport("advapi32.dll",SetLastError=true)] static extern bool RevertToSelf();
 static string Hex(uint value){return "0x"+value.ToString("X8");}
 static Win32Exception Failure(string context){int error=Marshal.GetLastWin32Error();return new Win32Exception(error,context+" error="+Hex(unchecked((uint)error)));}
 public static string Identity(string path) {
  using(var file=CreateFileW(path,0x80,7,IntPtr.Zero,3,0,IntPtr.Zero)) {
   if(file.IsInvalid)throw Failure(path);
   FileInfo info;if(!GetFileInformationByHandle(file,out info))throw Failure(path);
   return info.Volume.ToString("x8")+":"+info.IndexHigh.ToString("x8")+info.IndexLow.ToString("x8");
  }
 }
 public static IntPtr Restrict(string[] sids) {
  int size=Marshal.SizeOf(typeof(SidAndAttributes));var values=new List<IntPtr>();
  IntPtr table=Marshal.AllocHGlobal(size*sids.Length),original=IntPtr.Zero,token=IntPtr.Zero;
  try {
   for(int i=0;i<sids.Length;i++) {
    var value=new SecurityIdentifier(sids[i]);var bytes=new byte[value.BinaryLength];value.GetBinaryForm(bytes,0);
    IntPtr memory=Marshal.AllocHGlobal(bytes.Length);values.Add(memory);Marshal.Copy(bytes,0,memory,bytes.Length);
    Marshal.StructureToPtr(new SidAndAttributes{Sid=memory},IntPtr.Add(table,i*size),false);
   }
   if(!OpenProcessToken(GetCurrentProcess(),10,out original))throw Failure("OpenProcessToken");
   if(!CreateRestrictedToken(original,1,0,IntPtr.Zero,0,IntPtr.Zero,(uint)sids.Length,table,out token))throw Failure("CreateRestrictedToken");
   return token;
  } finally {if(original!=IntPtr.Zero)CloseHandle(original);foreach(var value in values)Marshal.FreeHGlobal(value);Marshal.FreeHGlobal(table);}
 }
 public static void CloseToken(IntPtr token){if(token!=IntPtr.Zero)CloseHandle(token);}
 static void Enter(IntPtr token){if(!ImpersonateLoggedOnUser(token))throw Failure("ImpersonateLoggedOnUser");}
 static void Leave(){if(!RevertToSelf())throw Failure("RevertToSelf");}
 public static string[] CheckRights(IntPtr token,string[] paths,string privateFile) {
  var errors=new List<string>();Enter(token);
  try {
   foreach(var path in paths) {
    using(var file=CreateFileW(path,0x21,7,IntPtr.Zero,3,0,IntPtr.Zero)) {
     if(file.IsInvalid)errors.Add("Session cannot read/execute payload: "+path+" error="+Hex(unchecked((uint)Marshal.GetLastWin32Error())));
    }
    foreach(uint access in new uint[]{2,4,0x40000000}) {
     using(var file=CreateFileW(path,access,7,IntPtr.Zero,access==0x40000000?2u:3u,0,IntPtr.Zero)) {
      int error=Marshal.GetLastWin32Error();
      if(!file.IsInvalid)errors.Add("Session can modify immutable payload: "+path+" access="+Hex(access));
      else if(error!=5)errors.Add("Payload denial was not AccessDenied5: "+path+" error="+Hex(unchecked((uint)error)));
     }
    }
   }
   try{File.WriteAllText(privateFile,"lease-private");if(File.ReadAllText(privateFile)!="lease-private")errors.Add("Private output changed");}
   catch(Exception error){errors.Add("Private output is not writable: "+error.Message+" HRESULT="+Hex(unchecked((uint)error.HResult)));}
  } finally {Leave();}
  return errors.ToArray();
 }
 public static void Unlink(IntPtr token,string path){Enter(token);try{File.Delete(path);}finally{Leave();}}
}
'@}
function Add-ViewTestLeaseAccess([string]$Path,[Security.Principal.SecurityIdentifier]$Sid){
 $acl=Get-Acl -LiteralPath $Path
 $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($Sid,[Security.AccessControl.FileSystemRights]::FullControl,[Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit',[Security.AccessControl.PropagationFlags]::None,[Security.AccessControl.AccessControlType]::Allow))
 Set-Acl -LiteralPath $Path -AclObject $acl
}
function Write-ViewTestFile([string]$Path,[string]$Value){
 [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Path))|Out-Null
 [IO.File]::WriteAllText($Path,$Value)
 (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash
}
function Test-ViewCase([string]$Name,[scriptblock]$Body){
 try{& $Body}catch{$failures.Add($Name+': '+$_.Exception.Message)}
}
$root=Join-Path ([IO.Path]::GetTempPath()) ('capy-session-view-'+[guid]::NewGuid().ToString('N'))
$payload=Join-Path $root 'payload';$build=Join-Path $payload 'artifacts/windows/Release'
$files=[ordered]@{};$controls=[ordered]@{};$views=[Collections.Generic.List[string]]::new();$failures=[Collections.Generic.List[string]]::new()
$identity=[Security.Principal.WindowsIdentity]::GetCurrent()
try{$maintainerSid=$identity.User;$maintainerName=$identity.Name}finally{$identity.Dispose()}
$domain=[guid]::NewGuid().ToByteArray()
$prefix='S-1-5-21-'+([BitConverter]::ToUInt32($domain,0))+'-'+([BitConverter]::ToUInt32($domain,4))+'-'+([BitConverter]::ToUInt32($domain,8))
$sids=@(1..15|ForEach-Object {[Security.Principal.SecurityIdentifier]::new($prefix+'-'+(1000+$_))})
$completed=$false
try{
 [IO.Directory]::CreateDirectory($root)|Out-Null
 foreach($name in 'CapyCanvas.exe','layer_windows.dll','CapyCanvas.pri'){$files[$name]=Write-ViewTestFile (Join-Path $build $name) ('immutable-'+$name)}
 foreach($i in 1..1446){$name='data/file-'+$i+'.bin';$files[$name]=Write-ViewTestFile (Join-Path $build $name) ('immutable-'+$i)}
 foreach($kind in 'Feedback','LayerDrop','Popup'){
  $map=[ordered]@{}
  foreach($name in 'CapyCanvas.exe','layer_windows.dll'){$map[$name]=Write-ViewTestFile (Join-Path ($build+'-'+$kind) $name) ($kind+'-'+$name)}
  $controls[$kind]=$map
 }
 $null=Write-ViewTestFile (Join-Path $payload 'tools/windows-vm/session-worker.ps1') 'private source copy'
 $null=Write-ViewTestFile (Join-Path $payload 'artifacts/windows/Release-neighbor/keep.txt') 'exact build-root exclusion'
 $null=Write-ViewTestFile (Join-Path $build 'unlisted-output.tmp') 'not immutable manifest data'
 $record=[ordered]@{profile='release';inputs=@{};files=$files;controls=$controls;hardware_gpu=$true;fixture_scale=1.5}
 $manifest=Join-Path $build 'fixture-build.json';$record|ConvertTo-Json -Depth 8|Set-Content -LiteralPath $manifest -Encoding UTF8
 Protect-FixtureSessionPayload -Path $root -User $maintainerName
 $null=Get-Content -LiteralPath $manifest -Raw
 $beforeManifest=(Get-FileHash -LiteralPath $manifest).Hash
 $beforeAcl=(Get-Acl -LiteralPath (Join-Path $build 'CapyCanvas.exe')).Sddl
 $watch=[Diagnostics.Stopwatch]::StartNew()
 foreach($i in 1..15){
  $session=Join-Path $root ('lease-'+$i);[IO.Directory]::CreateDirectory($session)|Out-Null
  $inherited=@((Get-Acl -LiteralPath $session).GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])|Where-Object {$_.IsInherited -and $_.AccessControlType -eq [Security.AccessControl.AccessControlType]::Allow -and $_.IdentityReference.Value -eq $maintainerSid.Value -and ($_.FileSystemRights -band [Security.AccessControl.FileSystemRights]::FullControl) -eq [Security.AccessControl.FileSystemRights]::FullControl})
  if(!$inherited.Count){throw 'New lease did not inherit the SSH user full-control grant'}
  Add-ViewTestLeaseAccess $session $sids[$i-1]
  $script:SessionViewAccount='*'+$sids[$i-1].Value
  $view=Join-Path $session 'repo';New-FixtureSessionView $payload $view $build
  $views.Add($view)
  $queuePath=Join-Path $session 'queue.json';$queueText='{"lease":'+$i+'}'
  [IO.File]::WriteAllText($queuePath,$queueText)
  if([IO.File]::ReadAllText($queuePath) -cne $queueText){throw 'Inherited SSH user access did not preserve the lease queue'}
 }
 $watch.Stop();Write-Output ('Session view setup: 1449 files x 15 leases, three tiny controls; seconds='+$watch.Elapsed.TotalSeconds.ToString('F3',[Globalization.CultureInfo]::InvariantCulture))
 Test-ViewCase 'shared-inodes' {
  foreach($view in $views){
   $viewBuild=Join-Path $view 'artifacts/windows/Release'
   foreach($name in $files.Keys){if([CapySessionViewTest]::Identity((Join-Path $build $name)) -cne [CapySessionViewTest]::Identity((Join-Path $viewBuild $name))){throw 'Lease view copied immutable payload'}}
   foreach($kind in $controls.Keys){foreach($name in $controls[$kind].Keys){if([CapySessionViewTest]::Identity((Join-Path ($build+'-'+$kind) $name)) -cne [CapySessionViewTest]::Identity((Join-Path ($viewBuild+'-'+$kind) $name))){throw 'Lease view copied immutable control payload'}}}
  }
 }
 Test-ViewCase 'manifest-and-paths' {
  foreach($view in $views){
   $viewBuild=Join-Path $view 'artifacts/windows/Release';$viewManifest=Join-Path $viewBuild 'fixture-build.json'
   if((Get-FileHash -LiteralPath $viewManifest).Hash -cne $beforeManifest -or [CapySessionViewTest]::Identity($manifest) -ceq [CapySessionViewTest]::Identity($viewManifest)){throw 'Lease build record was changed or hardlinked'}
   if(!(Test-Path -LiteralPath (Join-Path $view 'tools/windows-vm/session-worker.ps1')) -or !(Test-Path -LiteralPath (Join-Path $view 'artifacts/windows/Release-neighbor/keep.txt'))){throw 'Source copy excluded a neighboring path or lost the worker path'}
   if(Test-Path -LiteralPath (Join-Path $viewBuild 'unlisted-output.tmp')){throw 'Lease copied generated data from an immutable build root'}
  }
 }
 Test-ViewCase 'effective-file-rights' {
  $violations=[Collections.Generic.List[string]]::new()
  foreach($i in 0..14){
   $viewBuild=Join-Path $views[$i] 'artifacts/windows/Release'
   $paths=@((Join-Path $build 'CapyCanvas.exe'),(Join-Path $build 'layer_windows.dll'),(Join-Path $viewBuild 'CapyCanvas.exe'),(Join-Path $viewBuild 'layer_windows.dll'))
   foreach($kind in $controls.Keys){$paths+=@((Join-Path ($build+'-'+$kind) 'CapyCanvas.exe'),(Join-Path ($viewBuild+'-'+$kind) 'CapyCanvas.exe'))}
   $token=[CapySessionViewTest]::Restrict([string[]]@($sids[$i].Value,'S-1-5-32-555'))
   try{foreach($message in [CapySessionViewTest]::CheckRights($token,$paths,(Join-Path $viewBuild ('ui-state-'+$i+'.json')))){$violations.Add($message)}}finally{[CapySessionViewTest]::CloseToken($token)}
  }
  if($violations.Count){throw ($violations -join '; ')}
 }
 Test-ViewCase 'canonical-acl-and-content' {
  if((Get-Acl -LiteralPath (Join-Path $build 'CapyCanvas.exe')).Sddl -cne $beforeAcl){throw 'Lease grant changed the shared inode ACL'}
  foreach($name in $files.Keys){if((Get-FileHash -LiteralPath (Join-Path $build $name)).Hash -cne $files[$name]){throw 'Lease changed canonical immutable bytes'}}
  $acl=Get-Acl -LiteralPath (Join-Path $build 'CapyCanvas.exe')
  if(!(Get-Acl -LiteralPath $root).AreAccessRulesProtected){throw 'Run root did not protect its production access policy'}
  $write=[Security.AccessControl.FileSystemRights]'WriteData,AppendData,WriteExtendedAttributes,WriteAttributes,Delete,ChangePermissions,TakeOwnership'
  foreach($rule in $acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])){
   if($rule.AccessControlType -eq [Security.AccessControl.AccessControlType]::Allow -and ($rule.FileSystemRights -band $write) -and $rule.IdentityReference.Value -notin @($maintainerSid.Value,'S-1-5-18','S-1-5-32-544')){throw 'Canonical inode grants another principal write access'}
  }
 }
 Test-ViewCase 'private-writes-and-unlink' {
  foreach($i in 0..14){
   $private=Join-Path $views[$i] ('artifacts/windows/Release/ui-state-'+$i+'.json')
   if([IO.File]::ReadAllText($private) -cne 'lease-private' -or (Test-Path -LiteralPath (Join-Path $build ('ui-state-'+$i+'.json')))){throw 'Lease output escaped its private directory'}
   foreach($other in @(0..14|Where-Object {$_ -ne $i})){if(Test-Path -LiteralPath (Join-Path $views[$other] ('artifacts/windows/Release/ui-state-'+$i+'.json'))){throw 'Lease outputs share writable state'}}
  }
  $alias=Join-Path $views[14] 'artifacts/windows/Release/layer_windows.dll';$token=[CapySessionViewTest]::Restrict([string[]]@($sids[14].Value,'S-1-5-32-555'))
  try{[CapySessionViewTest]::Unlink($token,$alias)}finally{[CapySessionViewTest]::CloseToken($token)}
  if(Test-Path -LiteralPath $alias){throw 'Lease alias did not unlink'}
  if((Get-FileHash -LiteralPath (Join-Path $build 'layer_windows.dll')).Hash -cne $files['layer_windows.dll']){throw 'Unlink changed the canonical file'}
  foreach($view in $views[0..13]){if((Get-FileHash -LiteralPath (Join-Path $view 'artifacts/windows/Release/layer_windows.dll')).Hash -cne $files['layer_windows.dll']){throw 'Unlink changed a sibling view'}}
 }
 Test-ViewCase 'whole-run-delete' {
  Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction Stop
  if(Test-Path -LiteralPath $root){throw 'The inherited SSH user could not delete the whole run'}
 }
 if($failures.Count){throw ($failures -join [Environment]::NewLine)}
 $completed=$true
 'Session views share immutable file identities, deny writes and keep generated files private'
}finally{
 Remove-Variable SessionViewAccount -Scope Script -ErrorAction SilentlyContinue
 if(!$completed -and (Test-Path -LiteralPath $root)){
  try{
   & icacls.exe $root /reset /T /C /Q|Out-Null
   if($LASTEXITCODE){throw ('Could not reset owned test run for cleanup: '+$LASTEXITCODE)}
   Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction Stop
  }catch{Write-Warning ('Owned test cleanup failed: '+$_.Exception.Message)}
 }
}
