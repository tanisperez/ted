; Inno Setup 6 script for ted. Compile with ISCC, defines on the command line:
;   ISCC.exe /DAppVersion=0.1.0-rc.1 /DSourceExe=C:\path\ted.exe /DOutputDir=C:\out ted.iss
;   AppVersion  full version, may carry a pre-release suffix (X.Y.Z or X.Y.Z-rc.1)
;   SourceExe   path to the release ted.exe
;   OutputDir   folder for ted-<version>-windows-x86_64-setup.exe (default: .\Output)
; The LICENSE and the icon are found relative to this file (repo root and crates/ted-app).

#ifndef AppVersion
  #error Pass /DAppVersion=<version>
#endif
#ifndef SourceExe
  #error Pass /DSourceExe=<path to ted.exe>
#endif
#ifndef OutputDir
  #define OutputDir "Output"
#endif

; VersionInfoVersion must be numeric: keep X.Y.Z, drop any "-prerelease", append ".0".
#define NumericVersion Copy(AppVersion, 1, Pos("-", AppVersion + "-") - 1)

#define AppName "ted"
#define AppExe "ted.exe"
#define AppId "codes.tanis.ted"
#define Publisher "Estanislao Pérez Nartallo"
#define AppURL "https://github.com/tanisperez/ted"

[Setup]
; Never change this GUID, it identifies the installed app for upgrades and the uninstaller.
; The doubled brace is Inno's escape for a literal one.
AppId={{5D4DD9F7-5D9D-4AF3-B6EA-0BAD85E90D5B}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#Publisher}
AppPublisherURL={#AppURL}
AppSupportURL={#AppURL}
AppUpdatesURL={#AppURL}
VersionInfoVersion={#NumericVersion}.0
VersionInfoProductVersion={#NumericVersion}.0
VersionInfoCompany={#Publisher}
VersionInfoDescription={#AppName} setup
VersionInfoProductName={#AppName}
; Per user install, no UAC. With PrivilegesRequired=lowest, {autopf} resolves to
; {localappdata}\Programs, so it is the same folder as writing it out; explicit is clearer.
PrivilegesRequired=lowest
DefaultDirName={localappdata}\Programs\{#AppName}
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesAssociations=yes
; The PATH task changes the user environment: tell running programs (new terminals) about it.
ChangesEnvironment=yes
; Close a running ted through the Restart Manager instead of asking for a reboot.
CloseApplications=yes
RestartApplications=no
SetupIconFile=..\..\crates\ted-app\assets\ted.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
OutputDir={#OutputDir}
OutputBaseFilename=ted-{#AppVersion}-windows-x86_64-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern

; The languages of ted itself.
[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"

[CustomMessages]
english.AddToPath=Add ted to the PATH (run "ted file" or "ted ." from a terminal)
spanish.AddToPath=Añadir ted al PATH (ejecutar "ted archivo" o "ted ." desde una terminal)
english.ContextMenu=Add "Open with ted" to the Explorer menu of files and folders
spanish.ContextMenu=Añadir "Abrir con ted" al menú del Explorador en archivos y carpetas
english.OpenWith=Open with ted
spanish.OpenWith=Abrir con ted
english.Integration=Integration:
spanish.Integration=Integración:

[Tasks]
Name: "addtopath"; Description: "{cm:AddToPath}"; GroupDescription: "{cm:Integration}"
Name: "contextmenu"; Description: "{cm:ContextMenu}"; GroupDescription: "{cm:Integration}"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "{#AppExe}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\NOTICE"; DestDir: "{app}"; Flags: ignoreversion
; The document icons of the ted.<ext> ProgIDs below.
Source: "..\icons\documents\*.ico"; DestDir: "{app}\icons"; Flags: ignoreversion

; The app sets the explicit AppUserModelID codes.tanis.ted, so the shortcuts must carry the same
; one or taskbar pinning and grouping break.
[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; AppUserModelID: "{#AppId}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; AppUserModelID: "{#AppId}"; Tasks: desktopicon

; HKA is HKCU here (lowest privileges), so nothing needs admin.
[Registry]
; The user PATH, only when ted is not on it yet; [Code] removes it on uninstall.
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; Tasks: addtopath; Check: NeedsAddPath(ExpandConstant('{app}'))
; "Open with ted" on every file, on folders and on the background of an open folder.
Root: HKA; Subkey: "Software\Classes\*\shell\ted"; ValueType: string; ValueName: ""; ValueData: "{cm:OpenWith}"; Tasks: contextmenu; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\*\shell\ted"; ValueType: string; ValueName: "Icon"; ValueData: "{app}\{#AppExe},0"; Tasks: contextmenu
Root: HKA; Subkey: "Software\Classes\*\shell\ted\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""; Tasks: contextmenu
Root: HKA; Subkey: "Software\Classes\Directory\shell\ted"; ValueType: string; ValueName: ""; ValueData: "{cm:OpenWith}"; Tasks: contextmenu; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Directory\shell\ted"; ValueType: string; ValueName: "Icon"; ValueData: "{app}\{#AppExe},0"; Tasks: contextmenu
Root: HKA; Subkey: "Software\Classes\Directory\shell\ted\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%V"""; Tasks: contextmenu
Root: HKA; Subkey: "Software\Classes\Directory\Background\shell\ted"; ValueType: string; ValueName: ""; ValueData: "{cm:OpenWith}"; Tasks: contextmenu; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Directory\Background\shell\ted"; ValueType: string; ValueName: "Icon"; ValueData: "{app}\{#AppExe},0"; Tasks: contextmenu
Root: HKA; Subkey: "Software\Classes\Directory\Background\shell\ted\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%V"""; Tasks: contextmenu
; ted in the "Open with" list of the usual text files, without taking any of them over.
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{#AppName}"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".txt"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".md"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".log"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".json"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".toml"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".yaml"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".yml"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".xml"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".html"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".js"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".rs"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".ini"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".cfg"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".csv"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".mk"; ValueData: ""
; One ProgID per extension (ted.md, ted.txt, ...) with ted's document icon for its language,
; offered in the "Open with" list of the extension through OpenWithProgids. The installer never
; sets an extension's default value nor its UserChoice: files keep their app and their icon until
; the user picks ted in "Open with" with "Always". Windows then remembers ted.<ext>, and Explorer
; shows its icon. packaging/icons/documents/generate.py makes the .ico files; this list is the
; extensions ted highlights (crates/ted-syntax/src/language.rs) plus YAML, INI and CSV.
Root: HKA; Subkey: "Software\Classes\ted.txt"; ValueType: string; ValueName: ""; ValueData: "Text document"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.txt\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\txt.ico"
Root: HKA; Subkey: "Software\Classes\ted.txt\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.txt\OpenWithProgids"; ValueType: string; ValueName: "ted.txt"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.md"; ValueType: string; ValueName: ""; ValueData: "Markdown document"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.md\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\md.ico"
Root: HKA; Subkey: "Software\Classes\ted.md\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.md\OpenWithProgids"; ValueType: string; ValueName: "ted.md"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.markdown"; ValueType: string; ValueName: ""; ValueData: "Markdown document"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.markdown\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\md.ico"
Root: HKA; Subkey: "Software\Classes\ted.markdown\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.markdown\OpenWithProgids"; ValueType: string; ValueName: "ted.markdown"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.mdown"; ValueType: string; ValueName: ""; ValueData: "Markdown document"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.mdown\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\md.ico"
Root: HKA; Subkey: "Software\Classes\ted.mdown\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.mdown\OpenWithProgids"; ValueType: string; ValueName: "ted.mdown"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.log"; ValueType: string; ValueName: ""; ValueData: "Log file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.log\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\log.ico"
Root: HKA; Subkey: "Software\Classes\ted.log\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.log\OpenWithProgids"; ValueType: string; ValueName: "ted.log"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.rs"; ValueType: string; ValueName: ""; ValueData: "Rust source file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.rs\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\rs.ico"
Root: HKA; Subkey: "Software\Classes\ted.rs\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.rs\OpenWithProgids"; ValueType: string; ValueName: "ted.rs"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.js"; ValueType: string; ValueName: ""; ValueData: "JavaScript file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.js\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\js.ico"
Root: HKA; Subkey: "Software\Classes\ted.js\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.js\OpenWithProgids"; ValueType: string; ValueName: "ted.js"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.mjs"; ValueType: string; ValueName: ""; ValueData: "JavaScript file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.mjs\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\js.ico"
Root: HKA; Subkey: "Software\Classes\ted.mjs\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.mjs\OpenWithProgids"; ValueType: string; ValueName: "ted.mjs"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.cjs"; ValueType: string; ValueName: ""; ValueData: "JavaScript file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.cjs\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\js.ico"
Root: HKA; Subkey: "Software\Classes\ted.cjs\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.cjs\OpenWithProgids"; ValueType: string; ValueName: "ted.cjs"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.jsx"; ValueType: string; ValueName: ""; ValueData: "JavaScript file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.jsx\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\js.ico"
Root: HKA; Subkey: "Software\Classes\ted.jsx\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.jsx\OpenWithProgids"; ValueType: string; ValueName: "ted.jsx"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.json"; ValueType: string; ValueName: ""; ValueData: "JSON file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.json\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\json.ico"
Root: HKA; Subkey: "Software\Classes\ted.json\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.json\OpenWithProgids"; ValueType: string; ValueName: "ted.json"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.jsonc"; ValueType: string; ValueName: ""; ValueData: "JSON file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.jsonc\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\json.ico"
Root: HKA; Subkey: "Software\Classes\ted.jsonc\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.jsonc\OpenWithProgids"; ValueType: string; ValueName: "ted.jsonc"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.geojson"; ValueType: string; ValueName: ""; ValueData: "JSON file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.geojson\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\json.ico"
Root: HKA; Subkey: "Software\Classes\ted.geojson\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.geojson\OpenWithProgids"; ValueType: string; ValueName: "ted.geojson"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.webmanifest"; ValueType: string; ValueName: ""; ValueData: "JSON file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.webmanifest\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\json.ico"
Root: HKA; Subkey: "Software\Classes\ted.webmanifest\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.webmanifest\OpenWithProgids"; ValueType: string; ValueName: "ted.webmanifest"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.toml"; ValueType: string; ValueName: ""; ValueData: "TOML file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.toml\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\toml.ico"
Root: HKA; Subkey: "Software\Classes\ted.toml\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.toml\OpenWithProgids"; ValueType: string; ValueName: "ted.toml"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.yaml"; ValueType: string; ValueName: ""; ValueData: "YAML file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.yaml\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\yaml.ico"
Root: HKA; Subkey: "Software\Classes\ted.yaml\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.yaml\OpenWithProgids"; ValueType: string; ValueName: "ted.yaml"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.yml"; ValueType: string; ValueName: ""; ValueData: "YAML file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.yml\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\yaml.ico"
Root: HKA; Subkey: "Software\Classes\ted.yml\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.yml\OpenWithProgids"; ValueType: string; ValueName: "ted.yml"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.xml"; ValueType: string; ValueName: ""; ValueData: "XML file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.xml\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\xml.ico"
Root: HKA; Subkey: "Software\Classes\ted.xml\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.xml\OpenWithProgids"; ValueType: string; ValueName: "ted.xml"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.svg"; ValueType: string; ValueName: ""; ValueData: "XML file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.svg\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\xml.ico"
Root: HKA; Subkey: "Software\Classes\ted.svg\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.svg\OpenWithProgids"; ValueType: string; ValueName: "ted.svg"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.xsd"; ValueType: string; ValueName: ""; ValueData: "XML file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.xsd\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\xml.ico"
Root: HKA; Subkey: "Software\Classes\ted.xsd\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.xsd\OpenWithProgids"; ValueType: string; ValueName: "ted.xsd"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.xsl"; ValueType: string; ValueName: ""; ValueData: "XML file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.xsl\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\xml.ico"
Root: HKA; Subkey: "Software\Classes\ted.xsl\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.xsl\OpenWithProgids"; ValueType: string; ValueName: "ted.xsl"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.xslt"; ValueType: string; ValueName: ""; ValueData: "XML file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.xslt\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\xml.ico"
Root: HKA; Subkey: "Software\Classes\ted.xslt\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.xslt\OpenWithProgids"; ValueType: string; ValueName: "ted.xslt"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.plist"; ValueType: string; ValueName: ""; ValueData: "XML file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.plist\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\xml.ico"
Root: HKA; Subkey: "Software\Classes\ted.plist\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.plist\OpenWithProgids"; ValueType: string; ValueName: "ted.plist"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.csproj"; ValueType: string; ValueName: ""; ValueData: "XML file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.csproj\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\xml.ico"
Root: HKA; Subkey: "Software\Classes\ted.csproj\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.csproj\OpenWithProgids"; ValueType: string; ValueName: "ted.csproj"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.xaml"; ValueType: string; ValueName: ""; ValueData: "XML file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.xaml\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\xml.ico"
Root: HKA; Subkey: "Software\Classes\ted.xaml\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.xaml\OpenWithProgids"; ValueType: string; ValueName: "ted.xaml"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.rss"; ValueType: string; ValueName: ""; ValueData: "XML file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.rss\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\xml.ico"
Root: HKA; Subkey: "Software\Classes\ted.rss\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.rss\OpenWithProgids"; ValueType: string; ValueName: "ted.rss"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.atom"; ValueType: string; ValueName: ""; ValueData: "XML file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.atom\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\xml.ico"
Root: HKA; Subkey: "Software\Classes\ted.atom\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.atom\OpenWithProgids"; ValueType: string; ValueName: "ted.atom"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.html"; ValueType: string; ValueName: ""; ValueData: "HTML file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.html\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\html.ico"
Root: HKA; Subkey: "Software\Classes\ted.html\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.html\OpenWithProgids"; ValueType: string; ValueName: "ted.html"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.htm"; ValueType: string; ValueName: ""; ValueData: "HTML file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.htm\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\html.ico"
Root: HKA; Subkey: "Software\Classes\ted.htm\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.htm\OpenWithProgids"; ValueType: string; ValueName: "ted.htm"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.xhtml"; ValueType: string; ValueName: ""; ValueData: "HTML file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.xhtml\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\html.ico"
Root: HKA; Subkey: "Software\Classes\ted.xhtml\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.xhtml\OpenWithProgids"; ValueType: string; ValueName: "ted.xhtml"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.css"; ValueType: string; ValueName: ""; ValueData: "CSS file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.css\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\css.ico"
Root: HKA; Subkey: "Software\Classes\ted.css\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.css\OpenWithProgids"; ValueType: string; ValueName: "ted.css"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.mk"; ValueType: string; ValueName: ""; ValueData: "Makefile"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.mk\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\mk.ico"
Root: HKA; Subkey: "Software\Classes\ted.mk\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.mk\OpenWithProgids"; ValueType: string; ValueName: "ted.mk"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.mak"; ValueType: string; ValueName: ""; ValueData: "Makefile"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.mak\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\mk.ico"
Root: HKA; Subkey: "Software\Classes\ted.mak\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.mak\OpenWithProgids"; ValueType: string; ValueName: "ted.mak"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.sh"; ValueType: string; ValueName: ""; ValueData: "Shell script"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.sh\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\sh.ico"
Root: HKA; Subkey: "Software\Classes\ted.sh\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.sh\OpenWithProgids"; ValueType: string; ValueName: "ted.sh"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.bash"; ValueType: string; ValueName: ""; ValueData: "Shell script"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.bash\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\sh.ico"
Root: HKA; Subkey: "Software\Classes\ted.bash\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.bash\OpenWithProgids"; ValueType: string; ValueName: "ted.bash"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.zsh"; ValueType: string; ValueName: ""; ValueData: "Shell script"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.zsh\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\sh.ico"
Root: HKA; Subkey: "Software\Classes\ted.zsh\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.zsh\OpenWithProgids"; ValueType: string; ValueName: "ted.zsh"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.ksh"; ValueType: string; ValueName: ""; ValueData: "Shell script"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.ksh\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\sh.ico"
Root: HKA; Subkey: "Software\Classes\ted.ksh\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.ksh\OpenWithProgids"; ValueType: string; ValueName: "ted.ksh"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.py"; ValueType: string; ValueName: ""; ValueData: "Python file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.py\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\py.ico"
Root: HKA; Subkey: "Software\Classes\ted.py\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.py\OpenWithProgids"; ValueType: string; ValueName: "ted.py"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.pyw"; ValueType: string; ValueName: ""; ValueData: "Python file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.pyw\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\py.ico"
Root: HKA; Subkey: "Software\Classes\ted.pyw\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.pyw\OpenWithProgids"; ValueType: string; ValueName: "ted.pyw"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.pyi"; ValueType: string; ValueName: ""; ValueData: "Python file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.pyi\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\py.ico"
Root: HKA; Subkey: "Software\Classes\ted.pyi\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.pyi\OpenWithProgids"; ValueType: string; ValueName: "ted.pyi"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.ps1"; ValueType: string; ValueName: ""; ValueData: "PowerShell script"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.ps1\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\ps1.ico"
Root: HKA; Subkey: "Software\Classes\ted.ps1\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.ps1\OpenWithProgids"; ValueType: string; ValueName: "ted.ps1"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.psm1"; ValueType: string; ValueName: ""; ValueData: "PowerShell script"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.psm1\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\ps1.ico"
Root: HKA; Subkey: "Software\Classes\ted.psm1\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.psm1\OpenWithProgids"; ValueType: string; ValueName: "ted.psm1"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.psd1"; ValueType: string; ValueName: ""; ValueData: "PowerShell script"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.psd1\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\ps1.ico"
Root: HKA; Subkey: "Software\Classes\ted.psd1\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.psd1\OpenWithProgids"; ValueType: string; ValueName: "ted.psd1"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.bat"; ValueType: string; ValueName: ""; ValueData: "Batch file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.bat\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\bat.ico"
Root: HKA; Subkey: "Software\Classes\ted.bat\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.bat\OpenWithProgids"; ValueType: string; ValueName: "ted.bat"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.cmd"; ValueType: string; ValueName: ""; ValueData: "Batch file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.cmd\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\bat.ico"
Root: HKA; Subkey: "Software\Classes\ted.cmd\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.cmd\OpenWithProgids"; ValueType: string; ValueName: "ted.cmd"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.ini"; ValueType: string; ValueName: ""; ValueData: "Configuration file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.ini\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\ini.ico"
Root: HKA; Subkey: "Software\Classes\ted.ini\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.ini\OpenWithProgids"; ValueType: string; ValueName: "ted.ini"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.cfg"; ValueType: string; ValueName: ""; ValueData: "Configuration file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.cfg\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\ini.ico"
Root: HKA; Subkey: "Software\Classes\ted.cfg\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.cfg\OpenWithProgids"; ValueType: string; ValueName: "ted.cfg"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ted.csv"; ValueType: string; ValueName: ""; ValueData: "CSV file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ted.csv\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icons\csv.ico"
Root: HKA; Subkey: "Software\Classes\ted.csv\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.csv\OpenWithProgids"; ValueType: string; ValueName: "ted.csv"; ValueData: ""; Flags: uninsdeletevalue

; Interactively, a checkbox on the last page. Silent installs (winget) do not launch ted.
[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

[Code]
const
  EnvironmentKey = 'Environment';

{ Whether Dir is missing from the user PATH, compared without case and trailing backslash. }
function NeedsAddPath(Dir: string): Boolean;
var
  Path: string;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, EnvironmentKey, 'Path', Path) then
  begin
    Result := True;
    exit;
  end;
  Result := Pos(';' + Uppercase(RemoveBackslashUnlessRoot(Dir)) + ';',
    ';' + Uppercase(Path) + ';') = 0;
end;

{ Removes the install folder from the user PATH when ted is uninstalled. }
procedure RemovePath(Dir: string);
var
  Path, Upper, Entry: string;
  Index: Integer;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, EnvironmentKey, 'Path', Path) then
    exit;
  { Upper is Path with a ';' in front, so a match at Index there starts at Index - 1 in Path:
    the ';' before the folder, or nothing when the folder is the first entry. }
  Entry := ';' + Uppercase(RemoveBackslashUnlessRoot(Dir));
  Upper := ';' + Uppercase(Path);
  Index := Pos(Entry + ';', Upper + ';');
  if Index = 0 then
    exit;
  if Index > 1 then
    Delete(Path, Index - 1, Length(Entry))
  else
  begin
    { The first entry: the folder without a ';' before it, then the ';' after it, if any. }
    Delete(Path, 1, Length(Entry) - 1);
    if (Length(Path) > 0) and (Path[1] = ';') then
      Delete(Path, 1, 1);
  end;
  RegWriteExpandStringValue(HKEY_CURRENT_USER, EnvironmentKey, 'Path', Path);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RemovePath(ExpandConstant('{app}'));
end;
