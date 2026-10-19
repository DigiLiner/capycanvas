$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot '../scripts/WindowsArchitecture.ps1')
. (Join-Path $PSScriptRoot '../scripts/PortablePackage.ps1')
$repo=(Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$run=Join-Path $repo ('artifacts/windows/architecture-tests/'+[Guid]::NewGuid().ToString('N'))
$payload=Join-Path $run 'payload'
[IO.Directory]::CreateDirectory($payload)|Out-Null
$previousNative=$env:PROCESSOR_ARCHITECTURE
$previousEmulated=$env:PROCESSOR_ARCHITEW6432
try {
    foreach($native in @('ARM64','AMD64')) {
        foreach($emulated in @($null,'ARM64')) {
            $env:PROCESSOR_ARCHITECTURE=$native;$env:PROCESSOR_ARCHITEW6432=$emulated
            $expected=if($emulated -eq 'ARM64' -or $native -eq 'ARM64'){'arm64'}else{'x64'}
            if((Resolve-WindowsArchitecture).architecture -ne $expected){throw 'Native architecture selection ignored Windows emulation.'}
            foreach($architecture in @('x64','arm64')) {
                $build=Resolve-WindowsArchitecture $architecture
                if($build.architecture -ne $architecture){throw 'Explicit architecture did not override the host.'}
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
    Write-Output 'Windows architecture selection and mixed-payload regression checks passed.'
} finally {
    $env:PROCESSOR_ARCHITECTURE=$previousNative;$env:PROCESSOR_ARCHITEW6432=$previousEmulated
    foreach($path in @((Join-Path $payload 'fixture.dll'),(Join-Path $payload 'package-manifest.json'),(Join-Path $run 'result.json'))){Remove-Item -LiteralPath $path -ErrorAction SilentlyContinue}
    [IO.Directory]::Delete($payload)
    [IO.Directory]::Delete($run)
}
