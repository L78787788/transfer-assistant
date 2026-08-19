$ErrorActionPreference = 'Stop'
$geminiDir = 'D:\草稿箱\Gemini\传输助手Gemini'
if (-not (Test-Path -LiteralPath $geminiDir)) {
    New-Item -ItemType Directory -Force -Path $geminiDir | Out-Null
}

$targetExe = 'T:\app\build\windows\x64\runner\Release\transfer_assistant.exe'
$shortcutPath = Join-Path $geminiDir '传输助手Gemini.lnk'

$wshShell = New-Object -ComObject WScript.Shell
$shortcut = $wshShell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = $targetExe
$shortcut.WorkingDirectory = 'T:\app\build\windows\x64\runner\Release'
$shortcut.Description = '传输助手桌面端 Release 极速版'
$shortcut.Save()
Write-Host "Created shortcut: $shortcutPath"

