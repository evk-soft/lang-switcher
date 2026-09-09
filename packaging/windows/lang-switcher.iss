; Per-user installer for lang-switcher (ADR-0024).
;
; Build (Inno Setup 6.3 or newer, for ArchitecturesAllowed=x64compatible):
;   ISCC.exe /DAppVersion=0.1.0-alpha.1 packaging\windows\lang-switcher.iss
;
; Every path below is relative to this file. The compiler is expected to run from the
; repository root; SourceDir makes that explicit rather than implied.

#define AppName "lang-switcher"
#ifndef AppVersion
  #define AppVersion "0.0.0-dev"
#endif
; Where the release build put the binary. Overridden by packaging/build-release.ps1,
; which builds for an explicit target triple and therefore a different directory.
#ifndef BuildDir
  #define BuildDir "target\release"
#endif
; Where the generated third-party notice was written.
#ifndef StageDir
  #define StageDir "target\packaging"
#endif
#define AppPublisher "evk-soft"
#define AppURL "https://github.com/evk-soft/lang-switcher"
#define AppExeName "lang-switcher.exe"
; Must equal switcher_windows::single_instance::APP_MUTEX_NAME. A unit test in that
; module reads this file and fails if the two ever drift apart.
#define AppMutexName "lang-switcher-single-instance"

[Setup]
; Never change AppId: it is what lets a new version upgrade an existing installation
; instead of appearing next to it in Apps & Features.
AppId={{8C67EF71-F802-4367-BAE8-EE62951858EA}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppURL}
AppSupportURL={#AppURL}/issues
AppUpdatesURL={#AppURL}/releases
VersionInfoVersion=0.1.0.0
VersionInfoTextVersion={#AppVersion}

; No administrator prompt, ever. With lowest privileges {autopf} resolves to
; {localappdata}\Programs, a stable per-user directory the application can also be
; updated in place from.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
DisableDirPage=auto
AllowNoIcons=yes
UninstallDisplayName={#AppName} {#AppVersion}
UninstallDisplayIcon={app}\{#AppExeName}

; The application declares this mutex at startup, so Setup and Uninstall can ask the user
; to close a running copy instead of failing to replace a locked executable.
AppMutex={#AppMutexName}
CloseApplications=yes
RestartApplications=no

ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.22000

SourceDir=..\..
OutputDir={#StageDir}
OutputBaseFilename={#AppName}-{#AppVersion}-windows-x64-setup
SetupIconFile=crates\switcher-app\assets\icons\lang-switcher.ico
LicenseFile=LICENSE-MIT
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern

[Languages]
; The five interface languages of the application that Inno Setup ships translations for.
; Simplified Chinese is only available as an unofficial translation, so the installer
; falls back to English there while the application itself is still translated.
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "german"; MessagesFile: "compiler:Languages\German.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Files]
Source: "{#BuildDir}\{#AppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageDir}\THIRD-PARTY-LICENSES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "docs\guide\installation.en.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "docs\guide\installation.ru.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"

[Run]
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

[Registry]
; Autostart stays a tray option, so this entry is never created here: ValueType none plus
; dontcreatekey write nothing at install time. It exists only so that uninstalling removes
; the Run entry the application may have added, instead of leaving Windows trying to start
; a deleted executable at every logon.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "{#AppName}"; Flags: dontcreatekey uninsdeletevalue

; Configuration and logs under %APPDATA% and %LOCALAPPDATA% are deliberately left alone by
; both upgrade and uninstall: they are the user's data, not ours (ADR-0024).
