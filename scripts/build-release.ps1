param(
    [string]$Version = '1.0.0',
    [string]$Flutter,
    [switch]$SkipChecks
)

$ErrorActionPreference = 'Stop'
$targetRepoRoot = (Get-Location).Path
$targetDist = Join-Path $targetRepoRoot 'dist'

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
    param(
        [string]$SourceDir,
        [string]$TargetDir = 'E:\ta-build'
    )
    $excluded = @(
        '/XD', 'target', 'dist', 'build', '.dart_tool', '.gradle',
        '.git', '.reasonix', '.zcode', '.mimosa', 'node_modules'
    )
    $excluded += @('/XF', '*.jks', 'key.properties')
    if (-not (Test-Path -LiteralPath $TargetDir)) {
        New-Item -ItemType Directory -Force -Path $TargetDir | Out-Null
    }
    & robocopy.exe $SourceDir $TargetDir /E @excluded /NFL /NDL /NJH /NP /R:1 /W:1
    if ($LASTEXITCODE -gt 7) {
        throw "Unable to copy the repository to the ASCII workspace ($TargetDir)."
    }
    return @{ Root = $TargetDir }
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

$workspaceCopy = New-AsciiWorkspaceCopy -SourceDir $targetRepoRoot
$appRoot = Join-Path $workspaceCopy.Root 'app'

$keyProps = Join-Path $targetRepoRoot 'app\android\key.properties'
$keyStore = Join-Path $targetRepoRoot 'app\android\transassist-release.jks'
if (Test-Path -LiteralPath $keyProps) {
    Copy-Item -LiteralPath $keyProps -Destination (Join-Path $workspaceCopy.Root 'app\android\key.properties') -Force
}
if (Test-Path -LiteralPath $keyStore) {
    Copy-Item -LiteralPath $keyStore -Destination (Join-Path $workspaceCopy.Root 'app\android\transassist-release.jks') -Force
}

if (-not $SkipChecks) {
    Push-Location $targetRepoRoot
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

New-Item -ItemType Directory -Force -Path $targetDist | Out-Null
$apkSource = Join-Path $workspaceCopy.Root 'app\build\app\outputs\flutter-apk\app-release.apk'
if (-not (Test-Path -LiteralPath $apkSource)) {
    throw "Expected APK was not produced: $apkSource"
}
$apkTarget = Join-Path $targetDist "transfer-assistant-$Version-android-arm64.apk"
Copy-Item -LiteralPath $apkSource -Destination $apkTarget -Force

# 瀹夎鍣ㄥ繀椤诲湪 ASCII 鍓湰鐩綍涓嬬紪璇戯細MySourceDir/OutputDir 鐩稿褰撳墠鐩綍瑙ｆ瀽锛?# 鍦ㄥ壇鏈笅鎵嶈兘鎵撳寘鍒版湰娆℃瀯寤虹殑 Release 浜х墿銆?Push-Location $workspaceCopy.Root
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
Copy-Item -LiteralPath $installerSource -Destination $targetDist -Force

# Sync Release binaries to local repo
$localReleaseDir = Join-Path $targetRepoRoot 'app\build\windows\x64\runner\Release'
New-Item -ItemType Directory -Force -Path $localReleaseDir | Out-Null
$buildReleaseDir = Join-Path $workspaceCopy.Root 'app\build\windows\x64\runner\Release'
Copy-Item -Path "$buildReleaseDir\*" -Destination $localReleaseDir -Recurse -Force

$geminiDraftDir = 'D:\草稿箱\Gemini\传输助手Gemini'
if (Test-Path -Path $geminiDraftDir) {
    Copy-Item -LiteralPath $apkTarget -Destination $geminiDraftDir -Force
    Copy-Item -LiteralPath (Join-Path $targetDist "transfer-assistant-$Version-windows-x64-setup.exe") -Destination $geminiDraftDir -Force
}

$artifacts = Get-ChildItem -Path $targetDist -File |
    Where-Object Extension -In '.apk', '.exe' |
    Sort-Object Name
$hashLines = foreach ($artifact in $artifacts) {
    $hash = (Get-FileHash -LiteralPath $artifact.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $($artifact.Name)"
}
$hashPath = Join-Path $targetDist 'SHA256SUMS.txt'
[IO.File]::WriteAllLines($hashPath, $hashLines, [Text.UTF8Encoding]::new($false))
Write-Host 'Release artifacts:'
$artifacts | ForEach-Object { Write-Host $_.FullName }
Write-Host $hashPath
