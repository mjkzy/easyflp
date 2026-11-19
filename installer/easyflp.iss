#define AppName "easyflp"
#define CargoToml AddBackslash(SourcePath) + "..\Cargo.toml"
#define FH FileOpen(CargoToml)
#define Line ""
#define AppVersion ""
#sub ReadVersion
  #expr Line = FileRead(FH)
  #expr (AppVersion == "" && Pos('version = "', Line) == 1) ? AppVersion = Copy(Line, 12, Len(Line) - 12) : 0
#endsub
#for {0; !FileEof(FH); 0} ReadVersion
#expr FileClose(FH)
#if AppVersion == ""
  #error Cannot read the package version from Cargo.toml
#endif

[Setup]
AppId={{A3936A99-6E75-46D7-954C-DFA2CB2E796E}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=mjkzy
AppPublisherURL=https://github.com/mjkzy/easyflp
AppSupportURL=https://github.com/mjkzy/easyflp/issues
AppUpdatesURL=https://github.com/mjkzy/easyflp/releases/tag/latest
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesEnvironment=yes
CloseApplications=yes
UninstallDisplayIcon={app}\easyflp.exe
UninstallDisplayName={#AppName}
LicenseFile=..\LICENSE
OutputDir=..\target\installer
OutputBaseFilename=easyflp-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "openwith"; Description: "Add easyflp to the ""Open with"" menu for .flp and .fst files"; GroupDescription: "Integration:"
Name: "addtopath"; Description: "Add easyflp-cli to PATH (for the command line)"; GroupDescription: "Integration:"; Flags: unchecked

[Files]
Source: "..\target\release\easyflp.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\easyflp-cli.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\easyflp.exe"; AppUserModelID: "mjkzy.easyflp"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\easyflp.exe"; Tasks: desktopicon

[Registry]
Root: HKA; Subkey: "Software\Classes\Applications\easyflp.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{#AppName}"; Flags: uninsdeletekey; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\Applications\easyflp.exe\shell\open\command"; ValueType: string; ValueData: """{app}\easyflp.exe"" ""%1"""; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\Applications\easyflp.exe\SupportedTypes"; ValueType: string; ValueName: ".flp"; ValueData: ""; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\Applications\easyflp.exe\SupportedTypes"; ValueType: string; ValueName: ".fst"; ValueData: ""; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\.flp\OpenWithList\easyflp.exe"; Flags: uninsdeletekey; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\.fst\OpenWithList\easyflp.exe"; Flags: uninsdeletekey; Tasks: openwith

[Run]
Filename: "{app}\easyflp.exe"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

[Code]
const
  EnvKey = 'Environment';

function PathRoot: Integer;
begin
  if IsAdminInstallMode then
    Result := HKEY_LOCAL_MACHINE
  else
    Result := HKEY_CURRENT_USER;
end;

function PathSubkey: String;
begin
  if IsAdminInstallMode then
    Result := 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment'
  else
    Result := EnvKey;
end;

function PathHasDir(const Paths, Dir: String): Boolean;
begin
  Result := Pos(';' + Uppercase(Dir) + ';', ';' + Uppercase(Paths) + ';') > 0;
end;

procedure AddDirToPath(const Dir: String);
var
  Paths: String;
begin
  if not RegQueryStringValue(PathRoot, PathSubkey, 'Path', Paths) then
    Paths := '';
  if PathHasDir(Paths, Dir) then
    exit;
  if Paths = '' then
    Paths := Dir
  else if Copy(Paths, Length(Paths), 1) = ';' then
    Paths := Paths + Dir + ';'
  else
    Paths := Paths + ';' + Dir;
  RegWriteExpandStringValue(PathRoot, PathSubkey, 'Path', Paths);
end;

procedure RemoveDirFromPath(const Dir: String);
var
  Paths: String;
  P: Integer;
begin
  if not RegQueryStringValue(PathRoot, PathSubkey, 'Path', Paths) then
    exit;
  if not PathHasDir(Paths, Dir) then
    exit;
  Paths := ';' + Paths + ';';
  P := Pos(';' + Uppercase(Dir) + ';', Uppercase(Paths));
  Delete(Paths, P, Length(Dir) + 1);
  Paths := Copy(Paths, 2, Length(Paths) - 2);
  RegWriteExpandStringValue(PathRoot, PathSubkey, 'Path', Paths);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if (CurStep = ssPostInstall) and WizardIsTaskSelected('addtopath') then
    AddDirToPath(ExpandConstant('{app}'));
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RemoveDirFromPath(ExpandConstant('{app}'));
end;
