param([ValidateSet('Debug','Release')][string]$Configuration = 'Release')
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$packages = 'layer-core','layer-engine','layer-ui','layer-workspace','layer-host','layer-windows' | ForEach-Object { '-p', $_ }
$cargoProfileArgs = @(if ($Configuration -eq 'Release') { '--release' })
& (Join-Path $PSScriptRoot 'build.ps1') -Configuration $Configuration
& (Join-Path $PSScriptRoot 'test-input.ps1') -Configuration $Configuration
Push-Location $repo
$previousSoftwareGpu = $env:LAYER_TEST_SOFTWARE_GPU
$previousAdapter = $env:WGPU_ADAPTER_NAME
$previousGpuIndex = $env:LAYER_GPU_INDEX
$previousStorage = $env:CAPY_STORAGE_DIR
$previousPath = $env:PATH
try {
    $env:PATH = (Join-Path $repo "artifacts/windows/$Configuration")+';'+$previousPath
    $env:LAYER_TEST_SOFTWARE_GPU = '1'
    $env:WGPU_ADAPTER_NAME = 'Microsoft Basic Render Driver'
    $env:LAYER_GPU_INDEX = $null
    $env:CAPY_STORAGE_DIR = Join-Path $repo ('artifacts/windows/test-storage/'+[Guid]::NewGuid().ToString('N'))
    & cargo test --locked @cargoProfileArgs @packages --lib --features layer-render-wgpu/software-adapter-tests -- --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed.' }
    $env:CAPY_STORAGE_DIR = Join-Path $repo ('artifacts/windows/test-storage/'+[Guid]::NewGuid().ToString('N'))
    & cargo test --locked @cargoProfileArgs -p layer-windows --lib --features layer-render-wgpu/software-adapter-tests d3d12_ -- --ignored --test-threads=1 --skip hdr --skip native_color
    if ($LASTEXITCODE -ne 0) { throw 'D3D12 document tests failed.' }
} finally {
    $env:LAYER_TEST_SOFTWARE_GPU = $previousSoftwareGpu
    $env:WGPU_ADAPTER_NAME = $previousAdapter
    $env:LAYER_GPU_INDEX = $previousGpuIndex
    $env:CAPY_STORAGE_DIR = $previousStorage
    $env:PATH = $previousPath
    Pop-Location
}
