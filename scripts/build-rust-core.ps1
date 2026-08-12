param(
    [Parameter(Mandatory = $true)]
    [string]$Configuration
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'

if (-not (Test-Path -LiteralPath $cargo)) {
    throw "Cargo was not found at $cargo"
}

$arguments = @('build', '--manifest-path', (Join-Path $repoRoot 'Cargo.toml'), '-p', 'transfer_core')
if ($Configuration -ne 'Debug') {
    $arguments += '--release'
}

& $cargo @arguments
if ($LASTEXITCODE -ne 0) {
    throw "Rust core build failed with exit code $LASTEXITCODE"
}
