param(
    [string]$Flutter,
    [string]$AndroidSdk = $(if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { 'E:\Android\Sdk' }),
    [switch]$RequireInstaller
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$env:ANDROID_HOME = $AndroidSdk
$env:ANDROID_SDK_ROOT = $AndroidSdk

function Resolve-Tool([string]$Explicit, [string]$CommandName, [string[]]$Candidates) {
    if ($Explicit -and (Test-Path -LiteralPath $Explicit)) {
        return (Resolve-Path -LiteralPath $Explicit).Path
    }
    $command = Get-Command $CommandName -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }
    foreach ($candidate in $Candidates) {
        if ($candidate -and (Test-Path -LiteralPath $candidate)) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }
    throw "$CommandName was not found."
}

$cargo = Resolve-Tool '' 'cargo.exe' @((Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'))
$rustup = Resolve-Tool '' 'rustup.exe' @((Join-Path $env:USERPROFILE '.cargo\bin\rustup.exe'))
$flutterCandidates = @(
    $(if ($env:FLUTTER_ROOT) { Join-Path $env:FLUTTER_ROOT 'bin\flutter.bat' }),
    'E:\Developer\flutter\bin\flutter.bat'
)
$flutterTool = Resolve-Tool $Flutter 'flutter.bat' $flutterCandidates

$ndkVersion = '29.0.14206865'
$ndkLinker = Join-Path $AndroidSdk "ndk\$ndkVersion\toolchains\llvm\prebuilt\windows-x86_64\bin\aarch64-linux-android28-clang.cmd"
if (-not (Test-Path -LiteralPath $ndkLinker)) {
    throw "Android NDK $ndkVersion was not found below $AndroidSdk"
}
$targets = & $rustup target list --installed
if ($LASTEXITCODE -ne 0 -or $targets -notcontains 'aarch64-linux-android') {
    throw 'Rust target aarch64-linux-android is not installed.'
}

& $cargo --version
& $rustup show active-toolchain
& $flutterTool --version
& $flutterTool doctor -v
if ($LASTEXITCODE -ne 0) {
    throw 'flutter doctor failed.'
}
Write-Host "Android SDK: $AndroidSdk"
Write-Host "Android NDK linker: $ndkLinker"

if ($RequireInstaller) {
    $isccCandidates = @(
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
        'E:\Program Files (x86)\Inno Setup 6\ISCC.exe',
        'E:\Program Files\Inno Setup 6\ISCC.exe'
    )
    $iscc = Resolve-Tool '' 'ISCC.exe' $isccCandidates
    Write-Host "Inno Setup compiler: $iscc"
}

Write-Host "Environment check passed for $repoRoot"
