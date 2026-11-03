; Windows installer of ircnext (Inno Setup 6).
; Built by the release workflow: iscc /DAppVersion=X.Y.Z packaging\windows\ircnext.iss
; Paths are relative to this file.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
; Never change the AppId: it identifies the installation for updates.
AppId={{9C7E3B2A-4F0D-4C1E-9A57-3B8E2D6F1A40}
AppName=ircnext
AppVersion={#AppVersion}
AppVerName=ircnext {#AppVersion}
AppPublisher=BrystoQ
AppPublisherURL=https://github.com/BrystoQ/ircnext
AppSupportURL=https://github.com/BrystoQ/ircnext/issues
AppUpdatesURL=https://github.com/BrystoQ/ircnext/releases
DefaultDirName={autopf}\ircnext
DisableProgramGroupPage=yes
; Per-user by default (no administrator rights needed), or for everyone.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\..\dist
OutputBaseFilename=ircnext-{#AppVersion}-windows-x64-setup
SetupIconFile=..\..\assets\icon\ircnext.ico
UninstallDisplayIcon={app}\ircnext.exe
LicenseFile=..\..\LICENSE
WizardStyle=modern
Compression=lzma2
SolidCompression=yes
; An update closes the running ircnext and restarts it afterwards.
CloseApplications=yes
RestartApplications=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\..\target\release\ircnext.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\ircnext"; Filename: "{app}\ircnext.exe"
Name: "{autodesktop}\ircnext"; Filename: "{app}\ircnext.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\ircnext.exe"; Description: "{cm:LaunchProgram,ircnext}"; Flags: nowait postinstall skipifsilent
