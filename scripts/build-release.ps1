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

function New-AsciiWorkspaceCopy {
    # Gradle 对含中文的真实路径解析会失败（File.parent 为 null），
    # subst/junction 也会被 Gradle 解析回中文真实路径，因此把源码
    # 增量复制到纯 ASCII 路径构建。副本保留 Gradle/CMake 缓存以加速
    # 后续构建；产物目录与仓库元数据不复制。
    $copyRoot = 'E:\ta-build'
    $excluded = @(
        '/XD', 'target', 'dist', 'build', '.dart_tool', '.gradle',
        '.git', '.reasonix', '.zcode', '.mimosa', 'node_modules'
    )
    $excluded += @('/XF', '*.jks', 'key.properties')
    New-Item -ItemType Directory -Force -Path $copyRoot | Out-Null
    & robocopy.exe $repoRoot $copyRoot /E @excluded /NFL /NDL /NJH /NP /R:1 /W:1
    if ($LASTEXITCODE -gt 7) {
        throw "Unable to copy the repository to the ASCII workspace ($copyRoot)."
    }
    return @{ Root = $copyRoot }
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

$workspaceCopy = New-AsciiWorkspaceCopy
try {
    $appRoot = Join-Path $workspaceCopy.Root 'app'
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
    # 保留 ASCII 副本及其构建缓存，供下一次发布构建增量复用。
}

New-Item -ItemType Directory -Force -Path $dist | Out-Null
$apkSource = Join-Path $workspaceCopy.Root 'app\build\app\outputs\flutter-apk\app-release.apk'
if (-not (Test-Path -LiteralPath $apkSource)) {
    throw "Expected APK was not produced: $apkSource"
}
$apkTarget = Join-Path $dist "transfer-assistant-$Version-android-arm64.apk"
Copy-Item -LiteralPath $apkSource -Destination $apkTarget -Force

# 安装器必须在 ASCII 副本目录下编译：MySourceDir/OutputDir 相对当前目录解析，
# 在副本下才能打包到本次构建的 Release 产物。
Push-Location $workspaceCopy.Root
try {
    & $iscc "/DMyAppVersion=$Version" (Join-Path $workspaceCopy.Root 'installer\transfer-assistant.iss')
} finally {
    Pop-Location
}
if ($LASTEXITCODE -ne 0) {
    throw "Inno Setup failed with exit code $LASTEXITCODE"
}
$installerSource = Join-Path $workspaceCopy.Root "dist\transfer-assistant-$Version-windows-x64-setup.exe"
if (-not (Test-Path -LiteralPath $installerSource)) {
    throw "Expected installer was not produced: $installerSource"
}
Copy-Item -LiteralPath $installerSource -Destination $dist -Force

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
