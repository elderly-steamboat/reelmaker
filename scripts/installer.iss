; Inno Setup script: wraps dist\reelmaker-windows-x86_64 into one ReelMaker-Setup-<version>.exe.
; Built by scripts\build.ps1 (passes /DAppVersion=...). Per-user install, no admin rights needed.

#define AppName "ReelMaker"
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{B5A1C2D4-7E3F-4A6B-9C1D-2E8F0A1B3C4D}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=ReelMaker
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\dist
OutputBaseFilename=ReelMaker-Setup-{#AppVersion}
UninstallDisplayIcon={app}\reelmaker.exe
Compression=lzma2/normal
SolidCompression=yes
WizardStyle=modern

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: unchecked

[Files]
Source: "..\dist\reelmaker-windows-x86_64\*"; DestDir: "{app}"; Flags: recursesubdirs ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\reelmaker.exe"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\reelmaker.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\reelmaker.exe"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent
