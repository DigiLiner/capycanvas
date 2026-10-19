function Resolve-WindowsArchitecture([ValidateSet('native','x64','arm64')][string]$Architecture='native') {
    $native = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
    $hostArchitecture = if ($native -eq 'ARM64') { 'arm64' } else { 'x64' }
    if ($Architecture -eq 'native') { $Architecture = $hostArchitecture }
    $target = if ($Architecture -eq 'arm64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
    $component = if ($Architecture -eq 'arm64') { 'Microsoft.VisualStudio.Component.VC.Tools.ARM64' } else { 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64' }
    $platform = if ($Architecture -eq 'arm64') { 'ARM64' } else { 'x64' }
    [pscustomobject]@{architecture=$Architecture;platform=$platform;target=$target;hostArchitecture=$hostArchitecture;component=$component}
}

function Get-WindowsBuildDirectory([string]$Repo,$Build,[string]$Configuration) {
    if ($Build.architecture -eq 'arm64') { Join-Path $Repo "artifacts/windows/arm64/$Configuration" }
    else { Join-Path $Repo "artifacts/windows/$Configuration" }
}

function Assert-WindowsBinaryArchitecture([string]$Directory,[ValidateSet('x64','arm64')][string]$Architecture) {
    $machines = if ($Architecture -eq 'arm64') { @(0xAA64,0xA64E) } else { @(0x8664) }
    foreach ($file in Get-ChildItem -LiteralPath $Directory -File -Recurse | Where-Object Extension -In @('.exe','.dll')) {
        $reader = [IO.BinaryReader]::new([IO.File]::OpenRead($file.FullName))
        try {
            if ($reader.ReadUInt16() -ne 0x5A4D) { throw "Invalid Windows binary: $($file.FullName)" }
            $reader.BaseStream.Position = 0x3C
            $reader.BaseStream.Position = $reader.ReadUInt32()
            if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -notin $machines) { throw "Expected $Architecture binary: $($file.FullName)" }
        } finally { $reader.Dispose() }
    }
}
