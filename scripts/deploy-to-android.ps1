param(
    [string]$FlutterTool = 'E:\Developer\flutter\bin\flutter.bat',
    [string]$AndroidSdk = 'E:\Android\Sdk'
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$copyRoot = 'E:\ta-build'

Write-Host ">>> 1. Ensuring Android signing files..."
& (Join-Path $PSScriptRoot 'ensure-android-signing.ps1')

Write-Host ">>> 2. Syncing repo to ASCII workspace: $copyRoot..."
$excluded = @(
    '/XD', 'target', 'dist', 'build', '.dart_tool', '.gradle',
    '.git', '.reasonix', '.zcode', '.mimosa', 'node_modules'
)
$excluded += @('/XF', '*.jks', 'key.properties')
New-Item -ItemType Directory -Force -Path $copyRoot | Out-Null
& robocopy.exe $repoRoot $copyRoot /E @excluded /NFL /NDL /NJH /NP /R:1 /W:1

$keyProps = Join-Path $repoRoot 'app\android\key.properties'
$keyStore = Join-Path $repoRoot 'app\android\transassist-release.jks'
if (Test-Path -LiteralPath $keyProps) {
    Copy-Item -LiteralPath $keyProps -Destination (Join-Path $copyRoot 'app\android\key.properties') -Force
}
if (Test-Path -LiteralPath $keyStore) {
    Copy-Item -LiteralPath $keyStore -Destination (Join-Path $copyRoot 'app\android\transassist-release.jks') -Force
}

Write-Host ">>> 3. Configuring environment variables..."
$env:ANDROID_HOME = $AndroidSdk
$env:ANDROID_SDK_ROOT = $AndroidSdk
if (-not $env:FLUTTER_STORAGE_BASE_URL) {
    $env:FLUTTER_STORAGE_BASE_URL = 'https://storage.flutter-io.cn'
}
if (-not $env:PUB_HOSTED_URL) {
    $env:PUB_HOSTED_URL = 'https://pub.flutter-io.cn'
}

Write-Host ">>> 4. Building Android arm64 Release APK..."
Push-Location (Join-Path $copyRoot 'app')
try {
    & $FlutterTool build apk --release --target-platform android-arm64
    if ($LASTEXITCODE -ne 0) {
        throw "Flutter build apk failed with exit code $LASTEXITCODE"
    }
} finally {
    Pop-Location
}

$apkPath = Join-Path $copyRoot 'app\build\app\outputs\flutter-apk\app-release.apk'
if (-not (Test-Path -LiteralPath $apkPath)) {
    throw "APK was not found at: $apkPath"
}

$dist = Join-Path $repoRoot 'dist'
New-Item -ItemType Directory -Force -Path $dist | Out-Null
$apkDist = Join-Path $dist 'transfer-assistant-1.0.0-android-arm64.apk'
Copy-Item -LiteralPath $apkPath -Destination $apkDist -Force
Write-Host ">>> 5. Release APK generated: $apkDist"

$geminiFolder = 'D:\草稿箱\Gemini\传输助手Gemini'
if (Test-Path -LiteralPath $geminiFolder) {
    Copy-Item -LiteralPath $apkDist -Destination (Join-Path $geminiFolder 'transfer-assistant-1.0.0-android-arm64.apk') -Force
    Write-Host ">>> 6. Copied APK to Gemini folder: $geminiFolder"
}

$adb = Get-Command adb.exe -ErrorAction SilentlyContinue
if ($adb) {
    $devices = & adb.exe devices | Select-String -Pattern 'device$'
    if ($devices) {
        Write-Host ">>> 7. Detected Android device, reinstalling cleanly..."
        & adb.exe uninstall com.transassist.transfer_assistant | Out-Null
        & adb.exe uninstall com.transassist.app | Out-Null
        & adb.exe install -r -d $apkPath
        if ($LASTEXITCODE -eq 0) {
            Write-Host ">>> 8. Launching App on Android device..."
            & adb.exe shell am start -n com.transassist.app/com.transassist.transfer_assistant.MainActivity | Out-Null
            Write-Host ">>> Successfully deployed and updated on mobile device!"
        }
    } else {
        Write-Host ">>> (No online Android device detected via ADB, APK packaging complete.)"
    }
} else {
    Write-Host ">>> (ADB not in PATH, APK packaging complete.)"
}
