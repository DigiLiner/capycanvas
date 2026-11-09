param([string]$Fixture=(Join-Path $PSScriptRoot '../scripts/exercise-hdr.ps1'))
$ErrorActionPreference='Stop'
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($Fixture,[ref]$tokens,[ref]$errors)
if(@($errors).Count){throw ($errors|Out-String)}
$oracle=@($ast.FindAll({param($node)$node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Assert-HdrOutputFormat'},$true))
if($oracle.Count -ne 1){throw 'Missing actual HDR output format oracle'}
$calls=@($ast.FindAll({param($node)$node -is [Management.Automation.Language.CommandAst] -and $node.GetCommandName() -eq 'Assert-HdrOutputFormat'},$true))
if(!$calls.Count){throw 'HDR fixture does not call its output format oracle'}
. ([scriptblock]::Create($oracle[0].Extent.Text))
$checks=0
function New-HdrModel([string]$Depth,[bool]$Hdr){
    $headroom=if($Hdr){5}else{1}
    [pscustomobject]@{
        color_panel=[pscustomobject]@{hdr=$true;document_depth=$Depth}
        state=[pscustomobject]@{hdr_display_available=$Hdr}
        windows_display=[pscustomobject]@{
            format=$(if($Hdr){'Rgba16Float'}else{'Bgra8UnormSrgb'});headroom=$headroom
            output=[pscustomobject]@{hdr=$Hdr;headroom=$headroom;peak_nits=$(if($Hdr){1015}else{0});error=$null}
        }
    }
}
function Field-Owner($Model,[string]$Path){
    $parts=$Path.Split('.');$node=$Model
    for($index=0;$index -lt $parts.Length-1;$index++){$node=$node.($parts[$index])}
    @{node=$node;name=$parts[-1]}
}
function Reject-Output([string]$Name,$Model,[string]$Phase,[string]$Message='HDR output state is incomplete or invalid'){
    $observed=$null
    try{Assert-HdrOutputFormat $Model $Phase}catch{$observed=$_.Exception.Message}
    if($observed -cne $Message){throw "$Name expected '$Message', observed '$observed'"}
    $script:checks++
}
foreach($depth in @('F16','F32')){
    foreach($case in @(@{phase='Observed';hdr=$false},@{phase='SDR';hdr=$false},@{phase='Observed';hdr=$true},@{phase='HDR';hdr=$true})){
        Assert-HdrOutputFormat (New-HdrModel $depth $case.hdr) $case.phase
        $checks++
    }
    $lowHdr=New-HdrModel $depth $true;$lowHdr.windows_display.headroom=1;$lowHdr.windows_display.output.headroom=1;$lowHdr.windows_display.output.peak_nits=80;$lowHdr.state.hdr_display_available=$false
    Assert-HdrOutputFormat $lowHdr 'Observed';$checks++
    $syntheticSdr=New-HdrModel $depth $false;$syntheticSdr.windows_display.output.peak_nits=80
    Assert-HdrOutputFormat $syntheticSdr 'SDR';$checks++
    $wrongSdr=New-HdrModel $depth $false;$wrongSdr.windows_display.format='Rgba16Float'
    Reject-Output "$depth SDR rejects FP16" $wrongSdr 'SDR' 'Expected native 8-bit sRGB swap chain'
    Reject-Output "$depth observed SDR rejects FP16" $wrongSdr 'Observed' 'Expected native 8-bit sRGB swap chain'
    $wrongHdr=New-HdrModel $depth $true;$wrongHdr.windows_display.format='Bgra8UnormSrgb'
    Reject-Output "$depth HDR rejects SDR8" $wrongHdr 'HDR' 'Expected native floating-point swap chain'
    Reject-Output "$depth observed HDR rejects SDR8" $wrongHdr 'Observed' 'Expected native floating-point swap chain'
    Reject-Output "$depth HDR needs HDR output" (New-HdrModel $depth $false) 'HDR'
    Reject-Output "$depth SDR needs SDR output" (New-HdrModel $depth $true) 'SDR'
    Reject-Output "$depth missing model" $null 'Observed'
    foreach($path in @('windows_display','windows_display.format','windows_display.headroom','windows_display.output','windows_display.output.hdr','windows_display.output.headroom','windows_display.output.peak_nits','windows_display.output.error')){
        $missing=New-HdrModel $depth $true;$owner=Field-Owner $missing $path;$owner.node.PSObject.Properties.Remove($owner.name)
        Reject-Output "$depth missing $path" $missing 'HDR'
        if($path -ne 'windows_display.output.error'){
            $nullValue=New-HdrModel $depth $true;$owner=Field-Owner $nullValue $path;$owner.node.PSObject.Properties[$owner.name].Value=$null
            Reject-Output "$depth null $path" $nullValue 'HDR'
        }
    }
    foreach($invalid in @(
        @{path='windows_display';value=@('bad')},@{path='windows_display.output';value=@('bad')},
        @{path='windows_display.format';value='bgra8unormsrgb'},@{path='windows_display.format';value=''},@{path='windows_display.format';value=@('Bgra8UnormSrgb')},
        @{path='windows_display.headroom';value='1'},@{path='windows_display.headroom';value=0},@{path='windows_display.headroom';value=2},
        @{path='windows_display.headroom';value=[double]::NaN},@{path='windows_display.headroom';value=[double]::PositiveInfinity},
        @{path='windows_display.output.hdr';value='false'},@{path='windows_display.output.hdr';value=0},
        @{path='windows_display.output.headroom';value='1'},@{path='windows_display.output.headroom';value=0},@{path='windows_display.output.headroom';value=2},
        @{path='windows_display.output.headroom';value=[double]::NaN},@{path='windows_display.output.headroom';value=[double]::PositiveInfinity},
        @{path='windows_display.output.peak_nits';value='0'},@{path='windows_display.output.peak_nits';value=-1},
        @{path='windows_display.output.peak_nits';value=[double]::NaN},@{path='windows_display.output.peak_nits';value=[double]::PositiveInfinity},
        @{path='windows_display.output.error';value='DXGI output unavailable'}
    )){
        $model=New-HdrModel $depth $false;$owner=Field-Owner $model $invalid.path;$owner.node.PSObject.Properties[$owner.name].Value=$invalid.value
        Reject-Output "$depth malformed $($invalid.path)=$($invalid.value)" $model 'Observed'
    }
    $hdrWithoutPeak=New-HdrModel $depth $true;$hdrWithoutPeak.windows_display.output.peak_nits=0
    Reject-Output "$depth HDR needs valid luminance" $hdrWithoutPeak 'HDR'
}
Write-Output "HDR output format passed $checks cases: actual fixture oracle, exact SDR8/HDR16 phases, F16/F32 independence, inverse formats and malformed prerequisites; no window or GPU."
