#ifndef AppVersion
  #error AppVersion must be supplied by scripts/build-installer.ps1
#endif

[Setup]
AppId={{781147A4-7EF7-4AAC-A65D-8161D7FD45B0}
AppName=Picora
AppVersion={#AppVersion}
AppPublisher=Party4Bread
AppPublisherURL=https://github.com/Party4Bread/Picora
DefaultDirName={localappdata}\Programs\Picora
DefaultGroupName=Picora
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir=..\dist
OutputBaseFilename=picora-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\picora.exe
CloseApplications=yes
RestartApplications=no
DisableProgramGroupPage=yes

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "..\target\package\Windows\Picora\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Picora"; Filename: "{app}\picora.exe"; Parameters: "--library"
Name: "{group}\Picora in tray"; Filename: "{app}\picora.exe"; Parameters: "--background"
Name: "{autodesktop}\Picora"; Filename: "{app}\picora.exe"; Parameters: "--library"; Tasks: desktopicon

[Run]
Filename: "{app}\picora.exe"; Parameters: "--library"; Description: "Open Picora"; Flags: nowait postinstall skipifsilent

; App data is outside {app} and is deliberately preserved on uninstall.

[Code]
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Command: String;
begin
  if CurUninstallStep = usUninstall then
    if RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Picora', Command) then
      if CompareText(Command, '"' + ExpandConstant('{app}\picora.exe') + '" --background') = 0 then
        RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Picora');
end;
