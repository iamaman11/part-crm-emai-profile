; Per-user Windows wizard. Native Bridge owns CMS admission, download, staging and activation.
; Initial installations intentionally have no verified OS publisher. The embedded application
; integrity signature stays mandatory; no certificate/root import or security override is used.
; Inno APIs: https://jrsoftware.org/ishelp/topic_isxfunc_execandlogoutput.htm

#ifndef InstallerInputDirectory
  #error InstallerInputDirectory is required
#endif
#ifndef InstallerOutputDirectory
  #error InstallerOutputDirectory is required
#endif
#include AddBackslash(InstallerInputDirectory) + "installer-inputs.iss"

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
OutputBaseFilename=ProfileBridgeSetup
SignedUninstaller=no
#endif

[Languages]
Name: "ru"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "{#InstallerInputDirectory}\profile-bridge.exe"; Flags: dontcopy notimestamp
Source: "{#InstallerInputDirectory}\installation.json"; Flags: dontcopy notimestamp
Source: "{#InstallerInputDirectory}\windows-delivery-manifest.json"; Flags: dontcopy notimestamp
Source: "{#InstallerInputDirectory}\windows-delivery-signature.json"; Flags: dontcopy notimestamp

[Registry]
Root: HKCU; Subkey: "Software\Classes\profilebridge"; ValueType: string; ValueName: ""; ValueData: "URL:Profile Bridge"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\profilebridge"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\profilebridge\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{code:InstalledBridgePath}"" ""%1"""

[Run]
Filename: "{#ControlPlaneOrigin}/devices"; Description: "{cm:OpenApplication}"; Flags: shellexec postinstall skipifsilent nowait

[CustomMessages]
en.OpenApplication=Open Profile Operations
ru.OpenApplication=Открыть Profile Operations
en.InstallingBrowser=Installing and verifying the browser component. The first installation can take several minutes.
ru.InstallingBrowser=Загрузка и проверка браузера. Первая установка может занять несколько минут.

en.PreparingTitle=Preparing Profile Bridge
en.PreparingDescription=Downloading, unpacking and verifying the browser component.
ru.PreparingTitle=Подготовка Profile Bridge
ru.PreparingDescription=Загрузка, распаковка и проверка браузера.
en.FinishingInstallation=Finishing installation and checking the installed components.
ru.FinishingInstallation=Завершение установки и проверка установленных компонентов.
en.InstallationFailed=Installation could not finish. Check the message below, then retry this installer. Keep the setup log if the problem repeats.
ru.InstallationFailed=Не удалось завершить установку. Проверьте сообщение ниже и повторите установку. Если ошибка повторяется, сохраните журнал установки.

[Code]
const
  DirectoryPrefix = 'PROFILE_BRIDGE_PREPARED_DIRECTORY_V1=';
var
  BridgeOutputError: Boolean;
  BridgeLastLine: String;
  PreparedDirectory: String;

function SafePreparedDirectory(const Value: String): Boolean;
var
  Index: Integer;
begin
  Result := False;
  if (Length(Value) < 1) or (Length(Value) > 128) then exit;
  for Index := 1 to Length(Value) do
    if not (((Value[Index] >= 'a') and (Value[Index] <= 'z')) or
            ((Value[Index] >= '0') and (Value[Index] <= '9')) or
            (Value[Index] = '-')) then exit;
  Result := True;
end;

function InstalledBridgePath(Param: String): String;
begin
  if not SafePreparedDirectory(PreparedDirectory) then
    RaiseException('BRIDGE_INSTALLATION_OUTPUT_UNAVAILABLE');
  Result := ExpandConstant('{app}\releases\') + PreparedDirectory + '\profile-bridge\profile-bridge.exe';
end;

procedure CaptureBridgeOutput(const S: String; const Error, FirstLine: Boolean);
var
  Directory: String;
begin
  { Preserve native diagnostics as they arrive, before enforcing any exit gate. }
  Log(S);
  if not Error and (Pos(DirectoryPrefix, S) = 1) then begin
    Directory := Copy(S, Length(DirectoryPrefix) + 1, Length(S));
    if not SafePreparedDirectory(Directory) or (PreparedDirectory <> '') then
      BridgeOutputError := True
    else
      PreparedDirectory := Directory;
    exit;
  end;
  if Error then
    BridgeOutputError := True
  else if Trim(S) <> '' then
    BridgeLastLine := Trim(S);
end;

function QuoteArgument(const Value: String): String;
begin
  Result := '"' + Value + '"';
end;

function RunBridge(const Filename, Parameters: String): String;
var
  Started: Boolean;
  Code: Integer;
begin
  Result := '';
  BridgeOutputError := False;
  BridgeLastLine := '';
  Code := -1;
  try
    Started := ExecAndLogOutput(Filename, Parameters, '', SW_SHOWNORMAL,
      ewWaitUntilTerminated, Code, @CaptureBridgeOutput);
  except
    Result := 'BRIDGE_INSTALLATION_PROCESS_UNAVAILABLE';
    exit;
  end;
  if not Started then begin
    Result := 'BRIDGE_INSTALLATION_PROCESS_UNAVAILABLE';
    exit;
  end;
  if BridgeOutputError then begin
    Result := 'BRIDGE_INSTALLATION_OUTPUT_UNAVAILABLE';
    exit;
  end;
  if Code <> 0 then begin
    if BridgeLastLine <> '' then
      Result := BridgeLastLine
    else
      Result := 'BRIDGE_INSTALLATION_PROCESS_FAILED';
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Target: String;
  Progress: TOutputMarqueeProgressWizardPage;
begin
  NeedsRestart := False;
  Result := '';
  PreparedDirectory := '';
  Progress := CreateOutputMarqueeProgressPage(CustomMessage('PreparingTitle'),
    CustomMessage('PreparingDescription'));
  Progress.SetText(CustomMessage('InstallingBrowser'), '');
  Progress.Show;
  try
    Progress.Animate;
    ExtractTemporaryFile('profile-bridge.exe');
    ExtractTemporaryFile('installation.json');
    ExtractTemporaryFile('windows-delivery-manifest.json');
    ExtractTemporaryFile('windows-delivery-signature.json');
    Result := RunBridge(ExpandConstant('{tmp}\profile-bridge.exe'),
      '--delivery-prepare ' + QuoteArgument(ExpandConstant('{tmp}')) + ' ' + QuoteArgument(ExpandConstant('{app}')));
    if (Result = '') and (PreparedDirectory = '') then
      Result := 'BRIDGE_INSTALLATION_OUTPUT_UNAVAILABLE';
    if Result = '' then begin
      Progress.SetText(CustomMessage('FinishingInstallation'), '');
      Target := InstalledBridgePath('');
      Result := RunBridge(Target, '--delivery-install-staged ' + QuoteArgument('{#ControlPlaneOrigin}'));
    end;
  finally
    Progress.Hide;
  end;
  if Result <> '' then
    Result := CustomMessage('InstallationFailed') + #13#10 + Result;
end;

{ No delete/reset entries: uninstall removes the protocol and wizard registration, preserving
  device bindings, the CNG key, delivery recovery history and authoritative local profile files. }
