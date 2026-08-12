#ifndef MyAppVersion
  #define MyAppVersion "1.0.0"
#endif

#define MyAppName "传输助手"
#define MyAppPublisher "TransAssist"
#define MyAppExeName "transfer_assistant.exe"
#define MySourceDir "..\app\build\windows\x64\runner\Release"
#define MyFirewallRule "传输助手（专用网络）"

[Setup]
AppId={{D72870BA-758D-42C4-9AA7-1D61C05F8A4D}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\传输助手
DefaultGroupName=传输助手
DisableProgramGroupPage=yes
OutputDir=..\dist
OutputBaseFilename=transfer-assistant-{#MyAppVersion}-windows-x64-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=yes
RestartApplications=no
UninstallDisplayIcon={app}\{#MyAppExeName}
VersionInfoVersion={#MyAppVersion}
VersionInfoProductName={#MyAppName}
VersionInfoProductVersion={#MyAppVersion}

[Languages]
Name: "chinesesimplified"; MessagesFile: "Languages\ChineseSimplified.isl"

[Tasks]
Name: "desktopicon"; Description: "创建桌面快捷方式"; GroupDescription: "附加任务："; Flags: unchecked

[Files]
Source: "{#MySourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\传输助手"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\传输助手"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""{#MyFirewallRule}"" program=""{app}\{#MyAppExeName}"""; Flags: runhidden waituntilterminated
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall add rule name=""{#MyFirewallRule}"" dir=in action=allow program=""{app}\{#MyAppExeName}"" enable=yes profile=private"; Flags: runhidden waituntilterminated
Filename: "{app}\{#MyAppExeName}"; Description: "启动传输助手"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""{#MyFirewallRule}"" program=""{app}\{#MyAppExeName}"""; Flags: runhidden waituntilterminated; RunOnceId: "RemovePrivateFirewallRule"
