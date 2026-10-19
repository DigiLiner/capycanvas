function Install-WindowsSoftwareTestRuntime([string]$PackagesDirectory,[string]$Directory,[ValidateSet('x64','arm64')][string]$Architecture) {
    $destination=Join-Path ([IO.Path]::GetFullPath($Directory)) 'd3d10warp.dll'
    if($Architecture -eq 'x64') {
        if(Test-Path -LiteralPath $destination){throw 'x64 software tests require system WARP; an app-local runtime occupies the test directory.'}
        $system=Join-Path ([Environment]::SystemDirectory) 'd3d10warp.dll'
        if(!(Test-WindowsBinaryArchitecture $system x64)){throw 'System WARP has the wrong architecture for x64 tests.'}
        $file=Get-Item -LiteralPath $system
        Write-Host "System WARP: $system; version=$($file.VersionInfo.FileVersion); sha256=$((Get-FileHash -LiteralPath $system).Hash)"
        return
    }
    $package=([xml](Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot '../tests/packages.config'))).packages.package | Where-Object id -eq 'Microsoft.Direct3D.WARP'
    $source=Join-Path $PackagesDirectory ("$($package.id).$($package.version)/build/native/bin/$Architecture/d3d10warp.dll")
    if(!(Test-WindowsBinaryArchitecture $source $Architecture)){throw 'Software test runtime has the wrong architecture.'}
    $hash=(Get-FileHash -LiteralPath $source).Hash
    if(Test-Path -LiteralPath $destination) {
        if((Get-Item -LiteralPath $destination).Attributes -band [IO.FileAttributes]::ReparsePoint -or (Get-FileHash -LiteralPath $destination).Hash -ne $hash){throw 'A different software runtime already occupies the test directory.'}
        return [pscustomobject]@{path=$destination;sha256=$hash;created=$false}
    }
    [IO.Directory]::CreateDirectory($Directory)|Out-Null
    [IO.File]::Copy($source,$destination,$false)
    [pscustomobject]@{path=$destination;sha256=$hash;created=$true}
}

function Remove-WindowsSoftwareTestRuntime($Runtime) {
    if(!$Runtime -or !$Runtime.created){return}
    if((Get-Item -LiteralPath $Runtime.path).Attributes -band [IO.FileAttributes]::ReparsePoint -or (Get-FileHash -LiteralPath $Runtime.path).Hash -ne $Runtime.sha256){throw 'The staged software test runtime changed before cleanup.'}
    Remove-Item -LiteralPath $Runtime.path
}
