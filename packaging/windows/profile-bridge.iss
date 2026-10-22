; Windows wizard/effect adapter only. Native Bridge owns delivery admission, download, staging,
; first activation and settings. Production builds require the publisher signing tool configured
; in ISCC (/Spublisher=...). InstallerTestFixture produces an explicitly unsigned CI artifact;
; it does not change any native CMS verification or runtime policy.
; Inno APIs: https://jrsoftware.org/ishelp/topic_isxfunc_execandcaptureoutput.htm

#ifndef InstallerInputDirectory
  #error InstallerInputDirectory is required
#endif
#ifndef InstallerOutputDirectory
  #error InstallerOutputDirectory is required
#endif
#include "{#InstallerInputDirectory}\installer-inputs.iss"

[Setup]
AppId=Alegria.ProfileBridge
AppName=Profile Bridge
AppVersion=1.0
AppPublisher=Alegria
DefaultDirName={localappdata}\Alegria\ProfileBridge
UsePreviousAppDir=no
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
DisableDirPage=yes
DisableProgramGroupPage=yes
DisableWelcomePage=yes
WizardStyle=modern
SetupLogging=yes
Compression=lzma2
SolidCompression=yes
OutputDir={#InstallerOutputDirectory}
#ifdef InstallerTestFixture
OutputBaseFilename=UNSIGNED-CI-FIXTURE-ProfileBridgeSetup
SignedUninstaller=no
#else
OutputBaseFilename=ProfileBridgeSetup-{#ReleaseSetId}
SignTool=publisher
SignedUninstaller=yes
#endif
UninstallDisplayIcon={app}\releases\{#ReleaseSetId}\profile-bridge\profile-bridge.exe

[Languages]
Name: "ru"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "{#InstallerInputDirectory}\profile-bridge.exe"; Flags: dontcopy
Source: "{#InstallerInputDirectory}\installation.json"; Flags: dontcopy
Source: "{#InstallerInputDirectory}\windows-delivery-manifest.json"; Flags: dontcopy
Source: "{#InstallerInputDirectory}\windows-delivery-signature.json"; Flags: dontcopy

[Registry]
Root: HKCU; Subkey: "Software\Classes\profilebridge"; ValueType: string; ValueName: ""; ValueData: "URL:Profile Bridge"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\profilebridge"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\profilebridge\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\releases\{#ReleaseSetId}\profile-bridge\profile-bridge.exe"" ""%1"""

[Run]
Filename: "{#ControlPlaneOrigin}/devices"; Description: "{cm:OpenApplication}"; Flags: shellexec postinstall skipifsilent nowait

[CustomMessages]
en.OpenApplication=Open Profile Operations
ru.OpenApplication=Открыть Profile Operations
en.InstallingBrowser=Installing and verifying the browser component. The first installation can take several minutes.
ru.InstallingBrowser=Загрузка и проверка браузера. Первая установка может занять несколько минут.

[Code]
function QuoteArgument(const Value: String): String;
begin
  Result := '"' + Value + '"';
end;

function RunBridge(const Filename, Parameters: String): String;
var
  Started: Boolean;
  Code, Index: Integer;
  Output: TExecOutput;
begin
  Result := '';
  try
    Started := ExecAndCaptureOutput(Filename, Parameters, '', SW_SHOWNORMAL,
      ewWaitUntilTerminated, Code, Output);
  except
    Result := 'BRIDGE_INSTALLATION_PROCESS_UNAVAILABLE';
    exit;
  end;
  { Capture the native owner's complete safe diagnostics BEFORE enforcing the exit gate. }
  for Index := 0 to GetArrayLength(Output.StdOut) - 1 do
    Log(Output.StdOut[Index]);
  for Index := 0 to GetArrayLength(Output.StdErr) - 1 do
    Log(Output.StdErr[Index]);
  if not Started or Output.Error then begin
    Result := 'BRIDGE_INSTALLATION_OUTPUT_UNAVAILABLE';
    exit;
  end;
  if Code <> 0 then begin
    if GetArrayLength(Output.StdErr) > 0 then
      Result := Output.StdErr[GetArrayLength(Output.StdErr) - 1]
    else
      Result := 'BRIDGE_INSTALLATION_PROCESS_FAILED';
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Target: String;
begin
  NeedsRestart := False;
  WizardForm.PreparingLabel.Caption := CustomMessage('InstallingBrowser');
  ExtractTemporaryFile('profile-bridge.exe');
  ExtractTemporaryFile('installation.json');
  ExtractTemporaryFile('windows-delivery-manifest.json');
  ExtractTemporaryFile('windows-delivery-signature.json');
  Result := RunBridge(ExpandConstant('{tmp}\profile-bridge.exe'),
    '--delivery-prepare ' + QuoteArgument(ExpandConstant('{tmp}')) + ' ' + QuoteArgument(ExpandConstant('{app}')));
  if Result <> '' then exit;
  Target := ExpandConstant('{app}\releases\{#ReleaseSetId}\profile-bridge\profile-bridge.exe');
  Result := RunBridge(Target, '--delivery-install-staged ' + QuoteArgument('{#ControlPlaneOrigin}'));
end;

{ No delete/reset entries: uninstall removes the protocol and wizard registration, preserving
  device bindings, the CNG key, delivery recovery history and authoritative local profile files. }
