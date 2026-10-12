#ifndef AppVersion
  #error Pass /DAppVersion from scripts/build-installer.ps1
#endif

[Setup]
AppId={{D2A2EE76-FF22-4BE5-AD26-0EBD7881B4DC}
AppName=likh
AppVersion={#AppVersion}
VersionInfoVersion={#FileVersion}
AppPublisher=likh
AppPublisherURL=https://getlikh.vercel.app
DefaultDirName={localappdata}\Programs\likh
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\target\installer
OutputBaseFilename=likh-setup-windows-x64
UninstallDisplayIcon={app}\likh.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
SetupIconFile=..\app\assets\icon.ico

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "..\target\release\likh.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\likh"; Filename: "{app}\likh.exe"
Name: "{userdesktop}\likh"; Filename: "{app}\likh.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "likh"; Flags: uninsdeletevalue

[Run]
Filename: "{app}\likh.exe"; Description: "Open likh"; Flags: nowait postinstall skipifsilent
; likh updating itself runs this installer silently with /relaunch=1 after quitting.
Filename: "{app}\likh.exe"; Flags: nowait; Check: Relaunching

[Code]
function Relaunching(): Boolean;
begin
  Result := ExpandConstant('{param:relaunch|0}') = '1';
end;

function InitializeUninstall(): Boolean;
var
  ResultCode: Integer;
begin
  { Release the executable before removing the tray app. }
  if FileExists(ExpandConstant('{app}\likh.exe')) then
    Exec(ExpandConstant('{app}\likh.exe'), 'ctl quit', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Result := True;
end;
