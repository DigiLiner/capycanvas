$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot '../scripts/WindowsArchitecture.ps1')
. (Join-Path $PSScriptRoot '../scripts/PortablePackage.ps1')
$repo=(Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$run=Join-Path $repo ('artifacts/windows/architecture-tests/'+[Guid]::NewGuid().ToString('N'))
$payload=Join-Path $run 'payload'
[IO.Directory]::CreateDirectory($payload)|Out-Null
$previousNative=$env:PROCESSOR_ARCHITECTURE
$previousEmulated=$env:PROCESSOR_ARCHITEW6432
function Write-Arm64XFixture([string]$Path,[uint64]$Symbol=6,[uint16]$RelocationSection=1,[uint32]$PayloadSize=16,[switch]$Truncated) {
    $writer=[IO.BinaryWriter]::new([IO.File]::Create($Path))
    try {
        $writer.BaseStream.SetLength(0x600)
        $writer.Write([uint16]0x5A4D);$writer.BaseStream.Position=0x3C;$writer.Write([uint32]0x80)
        $writer.BaseStream.Position=0x80;$writer.Write([uint32]0x4550);$writer.Write([uint16]0xAA64);$writer.Write([uint16]1)
        $writer.BaseStream.Position=0x94;$writer.Write([uint16]0xF0)
        $writer.BaseStream.Position=0x98;$writer.Write([uint16]0x20B)
        $writer.BaseStream.Position=0x9C;$writer.Write([uint32]0x200)
        $writer.BaseStream.Position=0xA8;$writer.Write([uint32]0x1000)
        $writer.BaseStream.Position=0x104;$writer.Write([uint32]16)
        $writer.BaseStream.Position=0x158;$writer.Write([uint32]0x1000);$writer.Write([uint32]0x100)
        $writer.BaseStream.Position=0x188;$writer.Write([Text.Encoding]::ASCII.GetBytes(".rdata`0`0"))
        $writer.Write([uint32]0x400);$writer.Write([uint32]0x1000);$writer.Write([uint32]0x400);$writer.Write([uint32]0x200)
        $writer.BaseStream.Position=0x200;$writer.Write([uint32]0x100)
        $writer.BaseStream.Position=0x2E0;$writer.Write([uint32]0x180);$writer.Write($RelocationSection)
        $writer.BaseStream.Position=0x380;$writer.Write([uint32]1);$writer.Write($PayloadSize)
        $writer.Write($Symbol);$writer.Write([uint32]4);$writer.Write([uint32]0)
        if($Truncated){$writer.BaseStream.SetLength(0x38A)}
    } finally {$writer.Dispose()}
}
try {
    foreach($native in @('ARM64','AMD64')) {
        foreach($emulated in @($null,'ARM64')) {
            $env:PROCESSOR_ARCHITECTURE=$native;$env:PROCESSOR_ARCHITEW6432=$emulated
            $expected=if($emulated -eq 'ARM64' -or $native -eq 'ARM64'){'arm64'}else{'x64'}
            if((Resolve-WindowsArchitecture).architecture -ne $expected){throw 'Native architecture selection ignored Windows emulation.'}
            foreach($argument in @('x64','arm64','X64','ARM64')) {
                $architecture=$argument.ToLowerInvariant()
                $build=Resolve-WindowsArchitecture $argument
                if($build.architecture -cne $architecture){throw 'Explicit architecture did not normalize and override the host.'}
                $target=if($architecture -eq 'arm64'){'aarch64-pc-windows-msvc'}else{'x86_64-pc-windows-msvc'}
                if($build.target -ne $target){throw 'Rust target does not match the native architecture.'}
                if((Get-WindowsBuildDirectory $repo $build Release) -eq (Get-WindowsBuildDirectory $repo (Resolve-WindowsArchitecture $(if($architecture -eq 'x64'){'arm64'}else{'x64'})) Release)){throw 'Architectures share build output.'}
            }
        }
    }
    foreach($machine in @(0x8664,0xAA64,0xA64E)) {
        $path=Join-Path $payload 'fixture.dll'
        $writer=[IO.BinaryWriter]::new([IO.File]::Create($path))
        try {
            $writer.Write([uint16]0x5A4D);$writer.BaseStream.Position=0x3C;$writer.Write([uint32]0x80)
            $writer.BaseStream.Position=0x80;$writer.Write([uint32]0x4550);$writer.Write([uint16]$machine)
        } finally {$writer.Dispose()}
        $architecture=if($machine -eq 0x8664){'x64'}else{'arm64'}
        Assert-WindowsBinaryArchitecture $payload $architecture
        $manifest=[ordered]@{schema=1;architecture=$architecture;release_identity=$true;source_commit='architecture-fixture';files=@(@{path='fixture.dll';bytes=(Get-Item -LiteralPath $path).Length;sha256=(Get-FileHash -LiteralPath $path).Hash})}
        $manifestPath=Join-Path $payload 'package-manifest.json'
        $manifest|ConvertTo-Json -Depth 4|Set-Content -LiteralPath $manifestPath
        $resultPath=Join-Path $run 'result.json'
        @{payload=$payload;source_commit='architecture-fixture'}|ConvertTo-Json|Set-Content -LiteralPath $resultPath
        $portable=Read-PortablePackage $resultPath
        if($portable.manifest.architecture -ne $architecture){throw 'Portable package lost its architecture.'}
        $manifest.architecture=if($architecture -eq 'x64'){'arm64'}else{'x64'}
        $manifest|ConvertTo-Json -Depth 4|Set-Content -LiteralPath $manifestPath
        $rejected=$false
        try {Read-PortablePackage $resultPath|Out-Null} catch {$rejected=$true}
        if(!$rejected){throw 'Mixed-architecture runtime payload was accepted.'}
    }
    $path=Join-Path $payload 'fixture.dll'
    Write-Arm64XFixture $path
    foreach($architecture in @('x64','arm64')) {
        if(!(Test-WindowsBinaryArchitecture $path $architecture)){throw "ARM64X executable code was rejected for $architecture."}
    }
    Write-Arm64XFixture $path -Symbol 5
    if(Test-WindowsBinaryArchitecture $path x64){throw 'An ARM64 relocation symbol was accepted as ARM64X.'}
    if(!(Test-WindowsBinaryArchitecture $path arm64)){throw 'Pure ARM64 was rejected for ARM64.'}
    foreach($malformed in @(@{RelocationSection=2},@{PayloadSize=[uint32]::MaxValue},@{Truncated=$true})) {
        Write-Arm64XFixture $path @malformed
        if(Test-WindowsBinaryArchitecture $path x64){throw 'Malformed ARM64X metadata was accepted for x64.'}
    }
    Write-Output 'Windows architecture selection and mixed-payload regression checks passed.'
} finally {
    $env:PROCESSOR_ARCHITECTURE=$previousNative;$env:PROCESSOR_ARCHITEW6432=$previousEmulated
    foreach($path in @((Join-Path $payload 'fixture.dll'),(Join-Path $payload 'package-manifest.json'),(Join-Path $run 'result.json'))){Remove-Item -LiteralPath $path -ErrorAction SilentlyContinue}
    [IO.Directory]::Delete($payload)
    [IO.Directory]::Delete($run)
}
