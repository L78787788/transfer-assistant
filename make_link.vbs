Set oWS = WScript.CreateObject(\" "WScript.Shell\)  
sLinkFile = \D:\草稿箱\Gemini\传输助手Gemini\传输助手Gemini.lnk\  
Set oLink = oWS.CreateShortcut(sLinkFile)  
oLink.TargetPath = \T:\app\build\windows\x64\runner\Release\transfer_assistant.exe\  
oLink.WorkingDirectory = \T:\app\build\windows\x64\runner\Release\  
oLink.Description = \Transfer" Assistant "Release\  
oLink.Save  
