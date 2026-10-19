param([ValidateSet('Debug','Release')][string]$Configuration = 'Release', [ValidateSet('native','x64','arm64')][string]$Architecture = 'native')
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
. (Join-Path $PSScriptRoot 'WindowsArchitecture.ps1')
$build = Resolve-WindowsArchitecture $Architecture
$packages = 'layer-core','layer-engine','layer-ui','layer-workspace','layer-host','layer-windows' | ForEach-Object { '-p', $_ }
$cargoProfileArgs = @(if ($Configuration -eq 'Release') { '--release' })
& (Join-Path $PSScriptRoot 'build.ps1') -Configuration $Configuration -Architecture $build.architecture
& (Join-Path $PSScriptRoot 'test-input.ps1') -Configuration $Configuration -OutputDirectory (Join-Path $repo "artifacts/windows/tests/$($build.architecture)")
& (Join-Path $PSScriptRoot '../tests/architecture.ps1')
& (Join-Path $PSScriptRoot '../tests/application-menu-query.ps1')
& (Join-Path $PSScriptRoot '../tests/pen-capture.ps1')
Push-Location $repo
$previousSoftwareGpu = $env:LAYER_TEST_SOFTWARE_GPU
$previousAdapter = $env:WGPU_ADAPTER_NAME
$previousGpuIndex = $env:LAYER_GPU_INDEX
$previousStorage = $env:CAPY_STORAGE_DIR
$previousPath = $env:PATH
try {
    $env:PATH = (Get-WindowsBuildDirectory $repo $build $Configuration)+';'+$previousPath
    $env:LAYER_TEST_SOFTWARE_GPU = '1'
    $env:WGPU_ADAPTER_NAME = 'Microsoft Basic Render Driver'
    $env:LAYER_GPU_INDEX = $null
    $env:CAPY_STORAGE_DIR = Join-Path $repo ('artifacts/windows/test-storage/'+[Guid]::NewGuid().ToString('N'))
    & cargo test --locked --no-fail-fast --target $build.target @cargoProfileArgs @packages --lib --features layer-render-wgpu/software-adapter-tests -- --test-threads=1 --nocapture
    $failed = $LASTEXITCODE -ne 0
    $env:CAPY_STORAGE_DIR = Join-Path $repo ('artifacts/windows/test-storage/'+[Guid]::NewGuid().ToString('N'))
    & cargo test --locked --target $build.target @cargoProfileArgs -p layer-windows --lib --features layer-render-wgpu/software-adapter-tests d3d12_ -- --ignored --test-threads=1 --nocapture --skip hdr --skip native_color
    if ($failed -or $LASTEXITCODE -ne 0) { throw 'Windows Rust or D3D12 document tests failed.' }
} finally {
    $env:LAYER_TEST_SOFTWARE_GPU = $previousSoftwareGpu
    $env:WGPU_ADAPTER_NAME = $previousAdapter
    $env:LAYER_GPU_INDEX = $previousGpuIndex
    $env:CAPY_STORAGE_DIR = $previousStorage
    $env:PATH = $previousPath
    Pop-Location
}
