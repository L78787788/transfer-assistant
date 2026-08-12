param(
    [string]$Version = '1.0.0',
    [string]$Flutter,
    [switch]$SkipChecks
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$dist = Join-Path $repoRoot 'dist'

function Resolve-Flutter {
    if ($Flutter -and (Test-Path -LiteralPath $Flutter)) {
        return (Resolve-Path -LiteralPath $Flutter).Path
    }
    $command = Get-Command flutter.bat -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }
    $candidate = 'E:\Developer\flutter\bin\flutter.bat'
    if (Test-Path -LiteralPath $candidate) {
        return $candidate
    }
    throw 'flutter.bat was not found. Pass -Flutter with its absolute path.'
}

function Resolve-Iscc {
    $command = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }
    foreach ($candidate in @(
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
        'E:\Program Files (x86)\Inno Setup 6\ISCC.exe',
        'E:\Program Files\Inno Setup 6\ISCC.exe'
    )) {
        if (Test-Path -LiteralPath $candidate) {
            return $candidate
        }
    }
    throw 'ISCC.exe was not found. Install Inno Setup 6.'
}

function New-AsciiWorkspaceDrive {
    $repositoryMarker = Join-Path $repoRoot 'Cargo.toml'
    $repositoryMarkerHash = (Get-FileHash -LiteralPath $repositoryMarker -Algorithm SHA256).Hash
    foreach ($letter in 'T', 'S', 'R', 'Q', 'P') {
        $drive = "$letter`:"
        if (-not (Test-Path "$drive\")) {
            & subst.exe $drive $repoRoot
            if ($LASTEXITCODE -ne 0) {
                throw "Unable to map $drive to the repository."
            }
            return @{ Root = "$drive\"; Created = $true }
        }
        $mappedMarker = "$drive\Cargo.toml"
        if ((Test-Path -LiteralPath $mappedMarker) -and
            (Get-FileHash -LiteralPath $mappedMarker -Algorithm SHA256).Hash -eq $repositoryMarkerHash) {
            return @{ Root = "$drive\"; Created = $false }
        }
    }
    throw 'No free ASCII drive letter is available for the Flutter build.'
}

$flutterTool = Resolve-Flutter
$iscc = Resolve-Iscc
& (Join-Path $PSScriptRoot 'check-environment.ps1') -Flutter $flutterTool -RequireInstaller
& (Join-Path $PSScriptRoot 'ensure-android-signing.ps1')

$env:ANDROID_HOME = $(if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { 'E:\Android\Sdk' })
$env:ANDROID_SDK_ROOT = $env:ANDROID_HOME
if (-not $env:FLUTTER_STORAGE_BASE_URL) {
    $env:FLUTTER_STORAGE_BASE_URL = 'https://storage.flutter-io.cn'
}
if (-not $env:PUB_HOSTED_URL) {
    $env:PUB_HOSTED_URL = 'https://pub.flutter-io.cn'
}

$workspaceDrive = New-AsciiWorkspaceDrive
try {
    $appRoot = Join-Path $workspaceDrive.Root 'app'
    if (-not $SkipChecks) {
        Push-Location $repoRoot
        try {
            & cargo.exe fmt --all -- --check
            if ($LASTEXITCODE -ne 0) { throw 'cargo fmt failed.' }
            & cargo.exe clippy -p transfer_core --all-targets -- -D warnings
            if ($LASTEXITCODE -ne 0) { throw 'cargo clippy failed.' }
            & cargo.exe test --workspace
            if ($LASTEXITCODE -ne 0) { throw 'cargo test failed.' }
        } finally {
            Pop-Location
        }
        Push-Location $appRoot
        try {
            & $flutterTool analyze
            if ($LASTEXITCODE -ne 0) { throw 'flutter analyze failed.' }
            & $flutterTool test
            if ($LASTEXITCODE -ne 0) { throw 'flutter test failed.' }
        } finally {
            Pop-Location
        }
    }

    Push-Location $appRoot
    try {
        & $flutterTool build apk --release --target-platform android-arm64
        if ($LASTEXITCODE -ne 0) { throw 'Android Release build failed.' }
        & $flutterTool build windows --release
        if ($LASTEXITCODE -ne 0) { throw 'Windows Release build failed.' }
    } finally {
        Pop-Location
    }
} finally {
    if ($workspaceDrive.Created) {
        $mappedDrive = $workspaceDrive.Root.Substring(0, 2)
        & subst.exe $mappedDrive /D
    }
}

New-Item -ItemType Directory -Force -Path $dist | Out-Null
$apkSource = Join-Path $repoRoot 'app\build\app\outputs\flutter-apk\app-release.apk'
if (-not (Test-Path -LiteralPath $apkSource)) {
    throw "Expected APK was not produced: $apkSource"
}
$apkTarget = Join-Path $dist "transfer-assistant-$Version-android-arm64.apk"
Copy-Item -LiteralPath $apkSource -Destination $apkTarget -Force

& $iscc "/DMyAppVersion=$Version" (Join-Path $repoRoot 'installer\transfer-assistant.iss')
if ($LASTEXITCODE -ne 0) {
    throw "Inno Setup failed with exit code $LASTEXITCODE"
}

$artifacts = Get-ChildItem -LiteralPath $dist -File |
    Where-Object Extension -In '.apk', '.exe' |
    Sort-Object Name
$hashLines = foreach ($artifact in $artifacts) {
    $hash = (Get-FileHash -LiteralPath $artifact.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $($artifact.Name)"
}
$hashPath = Join-Path $dist 'SHA256SUMS.txt'
[IO.File]::WriteAllLines($hashPath, $hashLines, [Text.UTF8Encoding]::new($false))
Write-Host 'Release artifacts:'
$artifacts | ForEach-Object { Write-Host $_.FullName }
Write-Host $hashPath
