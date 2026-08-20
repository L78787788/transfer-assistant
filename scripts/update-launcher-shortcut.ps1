$ErrorActionPreference = 'Stop'
$geminiDir = 'D:\草稿箱\Gemini\传输助手Gemini'
if (-not (Test-Path -LiteralPath $geminiDir)) {
    New-Item -ItemType Directory -Force -Path $geminiDir | Out-Null
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$targetExe = Join-Path $repoRoot 'app\build\windows\x64\runner\Release\transfer_assistant.exe'

$wshShell = New-Object -ComObject WScript.Shell

# 创建 互传.lnk 与 传输助手Gemini.lnk
foreach ($name in @('互传.lnk', '传输助手Gemini.lnk')) {
    $shortcutPath = Join-Path $geminiDir $name
    $shortcut = $wshShell.CreateShortcut($shortcutPath)
    $shortcut.TargetPath = $targetExe
    $shortcut.WorkingDirectory = Join-Path $repoRoot 'app\build\windows\x64\runner\Release'
    $shortcut.Description = '互传桌面端 Release 极速版'
    $shortcut.Save()
    Write-Host "Created shortcut: $shortcutPath"
}

# 复制产物
$dist = Join-Path $repoRoot 'dist'
if (Test-Path -LiteralPath $dist) {
    Copy-Item -LiteralPath (Join-Path $dist '*') -Destination $geminiDir -Force
    Write-Host "Synced release artifacts to $geminiDir"
}
