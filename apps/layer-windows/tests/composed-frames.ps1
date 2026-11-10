$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot '../scripts/ComposedFrames.ps1')
$baseline=[Drawing.Bitmap]::new(4,4);$changed=$null
try{
    $changed=[Drawing.Bitmap]::new(4,4)
    for($y=0;$y -lt 4;$y++){for($x=0;$x -lt 4;$x++){
        $baseline.SetPixel($x,$y,[Drawing.Color]::Black)
        $changed.SetPixel($x,$y,[Drawing.Color]::Black)
    }}
    $envelope=[TabFrames+Envelope]::new($baseline,$baseline)
    if($envelope.Difference($changed) -ne 0){throw 'Composed frames changed an identical black image'}
    $changed.SetPixel(1,1,[Drawing.Color]::White)
    if($envelope.Difference($changed) -ne 1){throw 'Composed frames did not identify exactly one changed pixel'}
}finally{
    if($changed){$changed.Dispose()}
    $baseline.Dispose()
}
Write-Output 'Composed frames actual helper compiled and compared memory pixels'
