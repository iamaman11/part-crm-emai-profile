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
en.DownloadBytes=Downloaded bytes
ru.DownloadBytes=Загружено байт
en.LastByteAgo=Seconds since last received byte
ru.LastByteAgo=Секунд с получения последнего байта
en.DownloadFailure=Download failure category
ru.DownloadFailure=Категория ошибки загрузки
en.DownloadVerifying=Verifying downloaded archive integrity.
ru.DownloadVerifying=Проверка целостности загруженного архива.
en.DownloadCached=Using previously verified local archive.
ru.DownloadCached=Используется ранее проверенный локальный архив.
en.DownloadComplete=Archive verified. Preparing browser components.
ru.DownloadComplete=Архив проверен. Подготовка компонентов браузера.
en.AdmissionVerified=Signed release admission verified.
ru.AdmissionVerified=Подпись и допуск релиза проверены.
en.UnpackVerify=Unpacking components and verifying installed files.
ru.UnpackVerify=Распаковка компонентов и проверка установленных файлов.
en.StagedReady=Verified components staged for safe activation.
ru.StagedReady=Проверенные компоненты подготовлены к безопасной активации.

[Code]
const
  DirectoryPrefix = 'PROFILE_BRIDGE_PREPARED_DIRECTORY_V1=';
  DownloadProgressPrefix = 'PROFILE_BRIDGE_DOWNLOAD_PROGRESS_V1=';
  DownloadStagePrefix = 'PROFILE_BRIDGE_DOWNLOAD_STAGE_V1=';
  DownloadFailurePrefix = 'PROFILE_BRIDGE_DOWNLOAD_FAILURE_V1=';
  InstallStagePrefix = 'PROFILE_BRIDGE_INSTALL_STAGE_V1=';
var
  BridgeOutputError: Boolean;
  BridgeLastLine: String;
  BridgeOwnerVerdict: String;
  BridgeDownloadFailure: String;
  PreparedDirectory: String;
  InstallerProgress: TOutputMarqueeProgressWizardPage;

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

function TakeField(var Tail: String): String;
var
  Delimiter: Integer;
begin
  Delimiter := Pos('|', Tail);
  if Delimiter = 0 then begin
    Result := Tail;
    Tail := '';
  end else begin
    Result := Copy(Tail, 1, Delimiter - 1);
    Delete(Tail, 1, Delimiter);
  end;
end;

function DecimalBytes(const Value: String): Boolean;
var
  Index: Integer;
begin
  Result := False;
  if (Length(Value) = 0) or (Length(Value) > 20) then exit;
  for Index := 1 to Length(Value) do
    if (Value[Index] < '0') or (Value[Index] > '9') then exit;
  Result := True;
end;

function SafeDownloadAsset(const Value: String): Boolean;
begin
  Result := (Value = 'profile-bridge.zip') or (Value = 'runtime-bundle.tar');
end;

function SafeSignedDeliveryPolicyVerdict(const Value: String): Boolean;
begin
  { Closed semantic values from the existing native CMS admission owner.
    Never retain an arbitrary Debug payload, URL, pathname, credential or key. }
  Result := (Value = 'Preparation(Policy(SignatureInvalid))') or
            (Value = 'Preparation(Policy(SignatureVerificationFailed))') or
            (Value = 'Preparation(Policy(UnknownSigner))') or
            (Value = 'Preparation(Policy(RevokedSigner))') or
            (Value = 'Preparation(Policy(DowngradeRejected))') or
            (Value = 'Preparation(Policy(ReplayConflict))');
end;

function SafeNativeFailureCode(const Value: String): Boolean;
var
  Index: Integer;
begin
  Result := False;
  if (Length(Value) < 8) or (Length(Value) > 80) or
     (Pos('BRIDGE_', Value) <> 1) then exit;
  for Index := 1 to Length(Value) do
    if not (((Value[Index] >= 'A') and (Value[Index] <= 'Z')) or
            ((Value[Index] >= '0') and (Value[Index] <= '9')) or
            (Value[Index] = '_')) then exit;
  Result := True;
end;

procedure DisplayDownloadProgress(const Line: String);
var
  Tail, Asset, Bytes, Total, Idle: String;
begin
  if Length(Line) > 120 then exit;
  Tail := Line;
  Asset := TakeField(Tail);
  Bytes := TakeField(Tail);
  Total := TakeField(Tail);
  Idle := TakeField(Tail);
  if not SafeDownloadAsset(Asset) or not DecimalBytes(Bytes) or
     not DecimalBytes(Total) or not DecimalBytes(Idle) or (Tail <> '') then exit;
  InstallerProgress.SetText(
    Asset + ': ' + CustomMessage('DownloadBytes') + ' ' + Bytes + ' / ' + Total,
    CustomMessage('LastByteAgo') + ': ' + Idle);
end;

procedure DisplayDownloadStage(const Line: String);
var
  Tail, Asset, Stage, Description: String;
begin
  if Length(Line) > 100 then exit;
  Tail := Line;
  Asset := TakeField(Tail);
  Stage := TakeField(Tail);
  if not SafeDownloadAsset(Asset) or (Tail <> '') then exit;
  Description := '';
  if Stage = 'download' then Description := CustomMessage('InstallingBrowser')
  else if Stage = 'cached' then Description := CustomMessage('DownloadCached')
  else if Stage = 'verifying' then Description := CustomMessage('DownloadVerifying')
  else if Stage = 'verified' then Description := CustomMessage('DownloadComplete');
  if Description <> '' then InstallerProgress.SetText(Asset, Description);
end;

procedure DisplayInstallStage(const Line: String);
var
  Description: String;
begin
  if Length(Line) > 40 then exit;
  Description := '';
  if Line = 'ADMISSION_VERIFIED' then Description := CustomMessage('AdmissionVerified')
  else if Line = 'UNPACK_VERIFY' then Description := CustomMessage('UnpackVerify')
  else if Line = 'STAGED' then Description := CustomMessage('StagedReady');
  if Description <> '' then InstallerProgress.SetText(Description, '');
end;

procedure CaptureDownloadFailure(const Line: String);
var
  Tail, Asset, Category: String;
begin
  if Length(Line) > 100 then exit;
  Tail := Line;
  Asset := TakeField(Tail);
  Category := TakeField(Tail);
  if not SafeDownloadAsset(Asset) or (Tail <> '') then exit;
  if (Category = 'DNS') or (Category = 'CONNECT') or
     (Category = 'TLS') or (Category = 'HTTP') or
     (Category = 'READ_STALLED') or (Category = 'TIMEOUT') or
     (Category = 'SIZE') or (Category = 'DISK') or
     (Category = 'INSECURE_REDIRECT') or (Category = 'NETWORK_OR_PROCESS') or
     (Category = 'INVALID_REQUEST') or (Category = 'PLATFORM') then begin
    BridgeDownloadFailure := Asset + ': ' + Category;
    InstallerProgress.SetText(CustomMessage('DownloadFailure'), BridgeDownloadFailure);
  end;
end;

procedure CaptureSignedDeliveryVerdict(const Line: String);
var
  Prefix, Tail, Candidate: String;
  Position, Terminator: Integer;
begin
  if Length(Line) > 2048 then exit;
  Prefix := '"owner_verdict":"';
  Position := Pos(Prefix, Line);
  if Position = 0 then exit;
  Tail := Copy(Line, Position + Length(Prefix), Length(Line));
  Terminator := Pos('"', Tail);
  if Terminator = 0 then exit;
  Candidate := Copy(Tail, 1, Terminator - 1);
  if SafeSignedDeliveryPolicyVerdict(Candidate) then
    BridgeOwnerVerdict := Candidate;
end;

procedure CaptureBridgeOutput(const S: String; const Error, FirstLine: Boolean);
var
  Directory: String;
begin
  { Only bounded allowlisted download frames reach user-facing UI. Never
    display raw HTTP exceptions, URLs, proxy credentials or request payloads. }
  if not Error and (Pos(DownloadProgressPrefix, S) = 1) then begin
    DisplayDownloadProgress(Copy(S, Length(DownloadProgressPrefix) + 1, Length(S)));
    exit;
  end;
  if not Error and (Pos(DownloadStagePrefix, S) = 1) then begin
    DisplayDownloadStage(Copy(S, Length(DownloadStagePrefix) + 1, Length(S)));
    exit;
  end;
  if not Error and (Pos(InstallStagePrefix, S) = 1) then begin
    DisplayInstallStage(Copy(S, Length(InstallStagePrefix) + 1, Length(S)));
    exit;
  end;
  if not Error and (Pos(DownloadFailurePrefix, S) = 1) then begin
    CaptureDownloadFailure(Copy(S, Length(DownloadFailurePrefix) + 1, Length(S)));
    exit;
  end;
  if not Error and (Pos(DirectoryPrefix, S) = 1) then begin
    Directory := Copy(S, Length(DirectoryPrefix) + 1, Length(S));
    if not SafePreparedDirectory(Directory) or (PreparedDirectory <> '') then
      BridgeOutputError := True
    else
      PreparedDirectory := Directory;
    exit;
  end;
  if Error then begin
    BridgeOutputError := True;
    { stderr may include process-local details; log only that it occurred. }
    Log('BRIDGE_INSTALLATION_STDERR_OBSERVED');
  end else if Trim(S) <> '' then begin
    { Recover only a closed typed signing verdict from native installation
      outcome JSON, never the surrounding raw error/debug payload. }
    CaptureSignedDeliveryVerdict(S);
    { Preserve a bounded symbolic prefix even if a static safe explanation
      follows a colon. Never surface the explanation or unrelated text. }
    Directory := Trim(S);
    if Pos(':', Directory) > 0 then
      Directory := Copy(Directory, 1, Pos(':', Directory) - 1);
    if SafeNativeFailureCode(Directory) then
      BridgeLastLine := Directory
    else if BridgeLastLine = '' then
      BridgeLastLine := 'BRIDGE_INSTALLATION_OWNER_REJECTED';
  end;
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
  BridgeOwnerVerdict := '';
  BridgeDownloadFailure := '';
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
    if BridgeDownloadFailure <> '' then
      Result := Result + ' (' + BridgeDownloadFailure + ')';
    exit;
  end;
  if Code <> 0 then begin
    if BridgeDownloadFailure <> '' then
      Result := 'BRIDGE_INSTALLATION_DOWNLOAD_FAILED (' + BridgeDownloadFailure + ')'
    else if BridgeLastLine <> '' then
      Result := BridgeLastLine
    else
      Result := 'BRIDGE_INSTALLATION_PROCESS_FAILED';
    if BridgeOwnerVerdict <> '' then
      Result := Result + ' (' + BridgeOwnerVerdict + ')';
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Target: String;
 begin
  NeedsRestart := False;
  Result := '';
  PreparedDirectory := '';
  InstallerProgress := CreateOutputMarqueeProgressPage(CustomMessage('PreparingTitle'),
    CustomMessage('PreparingDescription'));
  InstallerProgress.SetText(CustomMessage('InstallingBrowser'), '');
  InstallerProgress.Show;
  try
    InstallerProgress.Animate;
    ExtractTemporaryFile('profile-bridge.exe');
    ExtractTemporaryFile('installation.json');
    ExtractTemporaryFile('windows-delivery-manifest.json');
    ExtractTemporaryFile('windows-delivery-signature.json');
    Result := RunBridge(ExpandConstant('{tmp}\profile-bridge.exe'),
      '--delivery-prepare ' + QuoteArgument(ExpandConstant('{tmp}')) + ' ' + QuoteArgument(ExpandConstant('{app}')));
    if (Result = '') and (PreparedDirectory = '') then
      Result := 'BRIDGE_INSTALLATION_OUTPUT_UNAVAILABLE';
    if Result = '' then begin
      InstallerProgress.SetText(CustomMessage('FinishingInstallation'), '');
      Target := InstalledBridgePath('');
      Result := RunBridge(Target, '--delivery-install-staged ' + QuoteArgument('{#ControlPlaneOrigin}'));
    end;
  finally
    InstallerProgress.Hide;
  end;
  if Result <> '' then
    Result := CustomMessage('InstallationFailed') + #13#10 + Result;
end;

{ No delete/reset entries: uninstall removes the protocol and wizard registration, preserving
  device bindings, the CNG key, delivery recovery history and authoritative local profile files. }
