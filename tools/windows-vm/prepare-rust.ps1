param([Parameter(Mandatory)][string]$Manifest)
$ErrorActionPreference = 'Stop'
$contents = Get-Content -LiteralPath $Manifest -Raw
$sections = [regex]::Matches($contents, '(?ms)^[ \t]*\[workspace\.package\][ \t]*(?:#[^\r\n]*)?\r?\n(?<body>.*?)(?=^[ \t]*\[|\z)')
if ($sections.Count -ne 1) { throw "Expected one [workspace.package] section in $Manifest" }
$versions = [regex]::Matches($sections[0].Groups['body'].Value, '(?m)^[ \t]*rust-version[ \t]*=[ \t]*(?<quote>["''])(?<version>\d+\.\d+(?:\.\d+)?)\k<quote>[ \t]*(?:#.*)?\r?$')
$minimum = $null
if ($versions.Count -ne 1 -or ![version]::TryParse($versions[0].Groups['version'].Value, [ref]$minimum)) { throw "Expected one numeric workspace rust-version in $Manifest" }
function Invoke-RustTool([string]$Command, [string[]]$Arguments) {
    $previous = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $output = @(& $Command @Arguments 2>&1)
        $status = $LASTEXITCODE
    } finally { $ErrorActionPreference = $previous }
    @{output=$output;status=$status}
}
function Read-RustCompilerVersion {
    $result = Invoke-RustTool -Command 'rustc' -Arguments @('--version')
    if ($result.status -ne 0) { throw "Could not read the active Rust compiler (exit $($result.status)): $($result.output -join ' ')" }
    $match = [regex]::Match(($result.output -join ' '), '^rustc\s+(?<version>\d+\.\d+\.\d+)(?:-[^\s]+)?(?:\s+\([^)]+\))?\s*$')
    $version = $null
    if (!$match.Success -or ![version]::TryParse($match.Groups['version'].Value, [ref]$version)) { throw "Invalid rustc --version output: $($result.output -join ' ')" }
    $version
}
$before = Read-RustCompilerVersion
$after = $before
$updated = $before -lt $minimum
if ($updated) {
    $result = Invoke-RustTool -Command 'rustup' -Arguments @('update','stable')
    if ($result.status -ne 0) { throw "Could not update stable Rust (exit $($result.status)): $($result.output -join ' ')" }
    $after = Read-RustCompilerVersion
    if ($after -lt $minimum) { throw "Active Rust compiler $after is below workspace rust-version $minimum after updating stable" }
}
@{required=$minimum.ToString();before=$before.ToString();after=$after.ToString();updated=$updated} | ConvertTo-Json -Compress
