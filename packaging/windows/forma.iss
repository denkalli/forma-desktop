; Inno Setup script for the Forma installer. Run through build.ps1, which passes the version
; and the folder holding forma.exe:
;   ISCC.exe /DAppVersion=0.5.0 /DStageDir=...\Forma-0.5.0-windows-x64 /DOutputDir=... forma.iss

#ifndef AppVersion
  #error Pass /DAppVersion=<version>
#endif
#ifndef StageDir
  #error Pass /DStageDir=<folder with forma.exe>
#endif
#ifndef OutputDir
  #define OutputDir "."
#endif

[Setup]
; Never change AppId: Windows uses it to find earlier installations to upgrade.
AppId={{5C508519-A647-425D-A3B9-EBB0F3804EA7}
AppName=Forma
AppVersion={#AppVersion}
AppVerName=Forma {#AppVersion}
AppPublisher=Forma contributors
AppPublisherURL=https://github.com/denkalli/forma-desktop
AppSupportURL=https://github.com/denkalli/forma-desktop/issues
DefaultDirName={autopf}\Forma
DefaultGroupName=Forma
DisableProgramGroupPage=yes
; Installs for the current user without administrator rights; the dialog offers all users.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
LicenseFile={#StageDir}\LICENSE.txt
SetupIconFile=forma.ico
UninstallDisplayIcon={app}\forma.exe
UninstallDisplayName=Forma
ChangesAssociations=yes
OutputDir={#OutputDir}
OutputBaseFilename=Forma-{#AppVersion}-windows-x64-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[CustomMessages]
english.FormaDesign=Forma design
brazilianportuguese.FormaDesign=Design do Forma
english.AssociateFiles=Open .forma files with Forma
brazilianportuguese.AssociateFiles=Abrir arquivos .forma com o Forma
english.FileTypes=File types:
brazilianportuguese.FileTypes=Tipos de arquivo:

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "associate"; Description: "{cm:AssociateFiles}"; GroupDescription: "{cm:FileTypes}"

[Files]
Source: "{#StageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\Forma"; Filename: "{app}\forma.exe"
Name: "{autodesktop}\Forma"; Filename: "{app}\forma.exe"; Tasks: desktopicon

[Registry]
; .forma files: the same MIME type as on Linux, opened by forma.exe "<file>".
; Uninstalling runs these in reverse order, so OpenWithProgids is emptied before .forma.
Root: HKA; Subkey: "Software\Classes\.forma"; ValueType: string; ValueName: ""; ValueData: "Forma.Design"; Flags: uninsdeletevalue uninsdeletekeyifempty; Tasks: associate
Root: HKA; Subkey: "Software\Classes\.forma"; ValueType: string; ValueName: "Content Type"; ValueData: "application/vnd.forma+zip"; Flags: uninsdeletevalue uninsdeletekeyifempty; Tasks: associate
Root: HKA; Subkey: "Software\Classes\.forma\OpenWithProgids"; ValueType: string; ValueName: "Forma.Design"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Forma.Design"; ValueType: string; ValueName: ""; ValueData: "{cm:FormaDesign}"; Flags: uninsdeletekey; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Forma.Design\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\forma.exe,0"; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Forma.Design\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\forma.exe"" ""%1"""; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Applications\forma.exe"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\forma.exe\SupportedTypes"; ValueType: string; ValueName: ".forma"; ValueData: ""

[Run]
Filename: "{app}\forma.exe"; Description: "{cm:LaunchProgram,Forma}"; Flags: nowait postinstall skipifsilent
