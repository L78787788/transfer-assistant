param(
    [string]$AndroidSdk = $(if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { 'E:\Android\Sdk' })
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
$ndkRoot = Join-Path $AndroidSdk 'ndk\29.0.14206865'
$toolchain = Join-Path $ndkRoot 'toolchains\llvm\prebuilt\windows-x86_64\bin'
$linker = Join-Path $toolchain 'aarch64-linux-android28-clang.cmd'

if (-not (Test-Path -LiteralPath $cargo)) {
    throw "Cargo was not found at $cargo"
}
if (-not (Test-Path -LiteralPath $linker)) {
    throw "Android NDK linker was not found at $linker"
}

$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = $linker
$env:CC_aarch64_linux_android = $linker
$env:AR_aarch64_linux_android = Join-Path $toolchain 'llvm-ar.exe'

& $cargo build --manifest-path (Join-Path $repoRoot 'Cargo.toml') -p transfer_core --target aarch64-linux-android --release
if ($LASTEXITCODE -ne 0) {
    throw "Android Rust core build failed with exit code $LASTEXITCODE"
}

$destination = Join-Path $repoRoot 'app\build\rustJniLibs\arm64-v8a'
New-Item -ItemType Directory -Force -Path $destination | Out-Null
Copy-Item -LiteralPath (Join-Path $repoRoot 'target\aarch64-linux-android\release\libtransfer_core.so') -Destination $destination -Force
