#ifndef AppVersion
  #define AppVersion "0.1.1"
#endif
#define ProjectRoot SourcePath + ".."

[Setup]
AppId=com.tellyouwhy.desktop
AppName=Tell You Why
AppVersion={#AppVersion}
AppPublisher=Tell You Why contributors
AppPublisherURL=https://github.com/Hccccy2002/Tell-You-Why
DefaultDirName={localappdata}\Programs\Tell You Why
DefaultGroupName=Tell You Why
UninstallDisplayIcon={app}\tell-you-why.exe
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.18362
OutputDir={#ProjectRoot}\release-artifacts
OutputBaseFilename=Tell-You-Why_{#AppVersion}_windows-x64-full-setup
SetupIconFile={#ProjectRoot}\src-tauri\icons\icon.ico
Compression=lzma2/fast
SolidCompression=yes
LZMANumBlockThreads=4
WizardStyle=modern
CloseApplications=no
RestartApplications=no
Uninstallable=yes
SetupLogging=yes

[Languages]
Name: "chinesesimp"; MessagesFile: "compiler:Languages\ChineseSimplified.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "{#ProjectRoot}\src-tauri\target\release\tell-you-why.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#ProjectRoot}\src-tauri\windows-crt\*.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#ProjectRoot}\src-tauri\pdf-runtime\*"; DestDir: "{app}\pdf-runtime"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#ProjectRoot}\tmp\release-downloads\WebView2-offline.exe"; DestDir: "{tmp}"; Flags: deleteafterinstall; Check: NeedsWebView2

[Icons]
Name: "{group}\Tell You Why"; Filename: "{app}\tell-you-why.exe"
Name: "{autodesktop}\Tell You Why"; Filename: "{app}\tell-you-why.exe"

[Run]
Filename: "{tmp}\WebView2-offline.exe"; Parameters: "/silent /install"; Flags: runhidden waituntilterminated; Check: NeedsWebView2
Filename: "{app}\tell-you-why.exe"; Description: "启动 Tell You Why"; Flags: nowait postinstall skipifsilent

[Code]
function NeedsWebView2: Boolean;
var
  Version: String;
  Key: String;
begin
  Key := 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';
  Result := not (
    (RegQueryStringValue(HKCU, Key, 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0')) or
    (RegQueryStringValue(HKLM32, Key, 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0')) or
    (RegQueryStringValue(HKLM64, Key, 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0')));
end;
