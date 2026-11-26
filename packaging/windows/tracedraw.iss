; Inno Setup script for TraceDraw. Built by CI on windows-latest.
; Variables: /DVersion=x.y.z /DSrcDir=<dir with tracedraw.exe and tracedraw-cli.exe>
; /DArch=x86 for the 32-bit Windows build (i686-pc-windows-msvc, #1975);
; x86_64 when not given.

#ifndef Version
  #define Version "0.0.0"
#endif
#ifndef SrcDir
  #define SrcDir "..\..\target\release"
#endif
#ifndef Arch
  #define Arch "x86_64"
#endif

[Setup]
AppId={{7E6E3B5C-4C1E-4F7B-9D7A-2B9D5C1A0E11}
AppName=TraceDraw
AppVersion={#Version}
AppPublisher=TraceDraw
AppCopyright=Copyright (c) 2026 TraceDraw members and contributors
AppPublisherURL=https://github.com/tracedraw/tracedraw
DefaultDirName={autopf}\TraceDraw
DefaultGroupName=TraceDraw
UninstallDisplayIcon={app}\tracedraw.exe
OutputDir=..\..\dist
OutputBaseFilename=tracedraw-{#Version}-windows-{#Arch}-setup
SetupIconFile=..\..\assets\icon\tracedraw.ico
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
#if Arch == "x86"
; The 32-bit build installs on any Windows that runs 32-bit programs. On
; 64-bit Windows it first recommends the x86_64 setup (InitializeSetup
; below) and goes on when asked to: refusing showed only Windows's
; message that the program does not support this version of Windows.
ArchitecturesAllowed=x86compatible
#else
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif
PrivilegesRequiredOverridesAllowed=dialog
; Close only TraceDraw's own programs that hold files to be replaced, not
; Explorer, which loads the thumbnail handler (that file is moved aside
; instead, see MoveAsideInUse below, #3816).
CloseApplicationsFilter=*.exe
; A TraceDraw still running when the setup starts (after Restart for an
; update, its window already closed while the program ends) has no window
; to be asked to close, so the quiet setup could not replace its files,
; undid its changes and closed without a word; ended by hand, the same
; setup installed at once (#3816). Programs that do not close when asked
; are now ended, as /FORCECLOSEAPPLICATIONS does.
CloseApplications=force
; The setup always writes a log (Setup Log <date> #<n>.txt in the user's
; Temp folder), also when an older TraceDraw starts it without /LOG: a
; quiet update that rolled back left nothing to say why (#3816).
SetupLogging=yes
ChangesAssociations=yes
ChangesEnvironment=yes
; The wizard runs in the system's language when Inno Setup has it, else
; in English. TraceDraw's own language is asked on the "TraceDraw's
; language" page below (System language first), not by Inno Setup's
; language dialog, which cannot offer System language.
ShowLanguageDialog=no

[Languages]
; The wizard's own language. These are the translations that ship with
; Inno Setup 6; the editor itself has more interface languages than this,
; all offered on the "TraceDraw's language" page. The six the editor has
; but Inno Setup does not ship (Arabic, Bengali, Hindi, Indonesian,
; Simplified and Traditional Chinese) run the wizard in English and need
; their .isl committed here; that is a follow-up.
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "czech"; MessagesFile: "compiler:Languages\Czech.isl"
Name: "dutch"; MessagesFile: "compiler:Languages\Dutch.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"
Name: "german"; MessagesFile: "compiler:Languages\German.isl"
Name: "italian"; MessagesFile: "compiler:Languages\Italian.isl"
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"
Name: "polish"; MessagesFile: "compiler:Languages\Polish.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "turkish"; MessagesFile: "compiler:Languages\Turkish.isl"

[CustomMessages]
; The 32-bit setup run on 64-bit Windows (InitializeSetup).
Use64Setup=This is the 32-bit TraceDraw for 32-bit Windows. Your Windows is 64-bit: the x86_64 setup (tracedraw-...-windows-x86_64-setup.exe) runs faster and can use all the memory. Install the 32-bit version anyway?
brazilianportuguese.Use64Setup=Este é o TraceDraw de 32 bits, para Windows de 32 bits. Seu Windows é de 64 bits: o instalador x86_64 (tracedraw-...-windows-x86_64-setup.exe) é mais rápido e pode usar toda a memória. Instalar a versão de 32 bits mesmo assim?
english.AddToPath=Add tracedraw-cli to PATH
english.CommandLine=Command line:
brazilianportuguese.AddToPath=Adicionar tracedraw-cli ao PATH
brazilianportuguese.CommandLine=Linha de comando:
czech.AddToPath=Přidat tracedraw-cli do proměnné PATH
czech.CommandLine=Příkazový řádek:
dutch.AddToPath=tracedraw-cli aan PATH toevoegen
dutch.CommandLine=Opdrachtregel:
french.AddToPath=Ajouter tracedraw-cli au PATH
french.CommandLine=Ligne de commande :
german.AddToPath=tracedraw-cli zum PATH hinzufügen
german.CommandLine=Befehlszeile:
italian.AddToPath=Aggiungi tracedraw-cli al PATH
italian.CommandLine=Riga di comando:
japanese.AddToPath=tracedraw-cli を PATH に追加する
japanese.CommandLine=コマンドライン:
polish.AddToPath=Dodaj tracedraw-cli do zmiennej PATH
polish.CommandLine=Wiersz poleceń:
russian.AddToPath=Добавить tracedraw-cli в PATH
russian.CommandLine=Командная строка:
spanish.AddToPath=Añadir tracedraw-cli al PATH
spanish.CommandLine=Línea de comandos:
turkish.AddToPath=tracedraw-cli'yi PATH'e ekle
turkish.CommandLine=Komut satırı:
english.AppLanguageTitle=TraceDraw's language
english.AppLanguageDescription=Choose the language TraceDraw starts in. System language follows the language of Windows. You can change it later in Tools > Options > Global > General.
english.AppLanguageSystem=System language
brazilianportuguese.AppLanguageTitle=Idioma do TraceDraw
brazilianportuguese.AppLanguageDescription=Escolha o idioma em que o TraceDraw inicia. Idioma do sistema segue o idioma do Windows. Você pode mudá-lo depois em Ferramentas > Opções > Global > Geral.
brazilianportuguese.AppLanguageSystem=Idioma do sistema
czech.AppLanguageTitle=Jazyk aplikace TraceDraw
czech.AppLanguageDescription=Vyberte jazyk, ve kterém se TraceDraw spustí. Jazyk systému se řídí jazykem Windows. Později jej můžete změnit v Nástroje > Možnosti > Globální > Obecné.
czech.AppLanguageSystem=Jazyk systému
dutch.AppLanguageTitle=Taal van TraceDraw
dutch.AppLanguageDescription=Kies de taal waarin TraceDraw start. Systeemtaal volgt de taal van Windows. U kunt dit later wijzigen via Extra > Opties > Algemeen > Algemeen.
dutch.AppLanguageSystem=Systeemtaal
french.AppLanguageTitle=Langue de TraceDraw
french.AppLanguageDescription=Choisissez la langue dans laquelle TraceDraw démarre. Langue du système suit la langue de Windows. Vous pourrez la changer plus tard dans Outils > Options > Global > Général.
french.AppLanguageSystem=Langue du système
german.AppLanguageTitle=Sprache von TraceDraw
german.AppLanguageDescription=Wählen Sie die Sprache, in der TraceDraw startet. Systemsprache folgt der Sprache von Windows. Sie können sie später unter Extras > Optionen > Global > Allgemein ändern.
german.AppLanguageSystem=Systemsprache
italian.AppLanguageTitle=Lingua di TraceDraw
italian.AppLanguageDescription=Scegli la lingua in cui si avvia TraceDraw. Lingua del sistema segue la lingua di Windows. Potrai cambiarla in seguito in Strumenti > Opzioni > Globale > Generale.
italian.AppLanguageSystem=Lingua del sistema
japanese.AppLanguageTitle=TraceDraw の言語
japanese.AppLanguageDescription=TraceDraw を起動する言語を選んでください。「システムの言語」は Windows の言語に従います。あとから [ツール] > [オプション] > [グローバル] > [全般] で変更できます。
japanese.AppLanguageSystem=システムの言語
polish.AppLanguageTitle=Język programu TraceDraw
polish.AppLanguageDescription=Wybierz język, w którym uruchomi się TraceDraw. Język systemu podąża za językiem Windows. Możesz go później zmienić w Narzędzia > Opcje > Globalne > Ogólne.
polish.AppLanguageSystem=Język systemu
russian.AppLanguageTitle=Язык TraceDraw
russian.AppLanguageDescription=Выберите язык, на котором запускается TraceDraw. «Язык системы» следует языку Windows. Его можно изменить позже: Сервис > Параметры > Общие > Основные.
russian.AppLanguageSystem=Язык системы
spanish.AppLanguageTitle=Idioma de TraceDraw
spanish.AppLanguageDescription=Elija el idioma en que se inicia TraceDraw. Idioma del sistema sigue el idioma de Windows. Puede cambiarlo más tarde en Herramientas > Opciones > Global > General.
spanish.AppLanguageSystem=Idioma del sistema
turkish.AppLanguageTitle=TraceDraw dili
turkish.AppLanguageDescription=TraceDraw'ın hangi dilde başlayacağını seçin. Sistem dili, Windows'un dilini izler. Daha sonra Araçlar > Seçenekler > Genel > Genel bölümünden değiştirebilirsiniz.
turkish.AppLanguageSystem=Sistem dili

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "addtopath"; Description: "{cm:AddToPath}"; GroupDescription: "{cm:CommandLine}"

[Files]
Source: "{#SrcDir}\tracedraw.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SrcDir}\tracedraw-cli.exe"; DestDir: "{app}"; Flags: ignoreversion
; Explorer thumbnails of .tdraw drawings (the first page)
Source: "{#SrcDir}\tracedraw_thumbnailer.dll"; DestDir: "{app}"; Flags: ignoreversion restartreplace uninsrestartdelete
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\TraceDraw"; Filename: "{app}\tracedraw.exe"
Name: "{group}\{cm:UninstallProgram,TraceDraw}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\TraceDraw"; Filename: "{app}\tracedraw.exe"; Tasks: desktopicon

[Registry]
; .tdraw file association
Root: HKA; Subkey: "Software\Classes\.tdraw"; ValueType: string; ValueName: ""; ValueData: "TraceDraw.Document"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\TraceDraw.Document"; ValueType: string; ValueName: ""; ValueData: "TraceDraw Document"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\TraceDraw.Document\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\tracedraw.exe,0"
Root: HKA; Subkey: "Software\Classes\TraceDraw.Document\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\tracedraw.exe"" ""%1"""
; Explorer shows a .tdraw file as a picture of its first page: the
; thumbnail provider in tracedraw_thumbnailer.dll (apps/tracedraw-thumbnailer)
Root: HKA; Subkey: "Software\Classes\CLSID\{{6B1E5D3A-2F41-4C8E-9B7A-1D2C3E4F5A60}"; ValueType: string; ValueName: ""; ValueData: "TraceDraw thumbnails"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\CLSID\{{6B1E5D3A-2F41-4C8E-9B7A-1D2C3E4F5A60}\InprocServer32"; ValueType: string; ValueName: ""; ValueData: "{app}\tracedraw_thumbnailer.dll"
Root: HKA; Subkey: "Software\Classes\CLSID\{{6B1E5D3A-2F41-4C8E-9B7A-1D2C3E4F5A60}\InprocServer32"; ValueType: string; ValueName: "ThreadingModel"; ValueData: "Apartment"
Root: HKA; Subkey: "Software\Classes\CLSID\{{6B1E5D3A-2F41-4C8E-9B7A-1D2C3E4F5A60}"; ValueType: dword; ValueName: "DisableProcessIsolation"; ValueData: "0"
Root: HKA; Subkey: "Software\Classes\.tdraw\ShellEx\{{e357fccd-a995-4576-b01f-234630154e96}"; ValueType: string; ValueName: ""; ValueData: "{{6B1E5D3A-2F41-4C8E-9B7A-1D2C3E4F5A60}"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\TraceDraw.Document\ShellEx\{{e357fccd-a995-4576-b01f-234630154e96}"; ValueType: string; ValueName: ""; ValueData: "{{6B1E5D3A-2F41-4C8E-9B7A-1D2C3E4F5A60}"; Flags: uninsdeletekey
; Kept when another program takes .tdraw's default (Open With, a
; UserChoice): Explorer reads this key whichever program opens the file.
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.tdraw\ShellEx\{{e357fccd-a995-4576-b01f-234630154e96}"; ValueType: string; ValueName: ""; ValueData: "{{6B1E5D3A-2F41-4C8E-9B7A-1D2C3E4F5A60}"; Flags: uninsdeletekey
; Thumbnails of .cdr files too (#3239), in the fallback key alone:
; Explorer asks the program that owns .cdr first, so one that draws its
; own thumbnails keeps doing so.
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.cdr\ShellEx\{{e357fccd-a995-4576-b01f-234630154e96}"; ValueType: string; ValueName: ""; ValueData: "{{6B1E5D3A-2F41-4C8E-9B7A-1D2C3E4F5A60}"; Flags: uninsdeletekey
; "Open with TraceDraw" for .cdr, without stealing the default
Root: HKA; Subkey: "Software\Classes\.cdr\OpenWithProgids"; ValueType: string; ValueName: "TraceDraw.Document"; ValueData: ""; Flags: uninsdeletevalue
; "Open with TraceDraw" for pictures, without stealing their default: a
; picture opened so becomes a new drawing holding it (#2600)
Root: HKA; Subkey: "Software\Classes\TraceDraw.Picture"; ValueType: string; ValueName: ""; ValueData: "Picture"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\TraceDraw.Picture\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\tracedraw.exe,0"
Root: HKA; Subkey: "Software\Classes\TraceDraw.Picture\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\tracedraw.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.png\OpenWithProgids"; ValueType: string; ValueName: "TraceDraw.Picture"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.jpg\OpenWithProgids"; ValueType: string; ValueName: "TraceDraw.Picture"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.jpeg\OpenWithProgids"; ValueType: string; ValueName: "TraceDraw.Picture"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.bmp\OpenWithProgids"; ValueType: string; ValueName: "TraceDraw.Picture"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.gif\OpenWithProgids"; ValueType: string; ValueName: "TraceDraw.Picture"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.webp\OpenWithProgids"; ValueType: string; ValueName: "TraceDraw.Picture"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.tif\OpenWithProgids"; ValueType: string; ValueName: "TraceDraw.Picture"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.tiff\OpenWithProgids"; ValueType: string; ValueName: "TraceDraw.Picture"; ValueData: ""; Flags: uninsdeletevalue
; TraceDraw's language as chosen on the "TraceDraw's language" page:
; "system" (System language) or one of the app's codes (en, pt-BR,
; zh-TW...). The app reads it at start while no language was picked in
; its Options (docs/behavior/language.md); HKA is HKCU for an install for
; the user alone, HKLM for one for all users.
Root: HKA; Subkey: "Software\TraceDraw"; ValueType: string; ValueName: "InstallerLanguage"; ValueData: "{code:ChosenAppLanguage}"; Flags: uninsdeletevalue uninsdeletekeyifempty
; PATH
Root: HKA; Subkey: "{code:PathRegKey}"; ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; Tasks: addtopath; Check: NeedsAddPath('{app}')

[Run]
Filename: "{app}\tracedraw.exe"; Description: "{cm:LaunchProgram,TraceDraw}"; Flags: nowait postinstall skipifsilent shellexec runasoriginaluser
; Help > Updates > Install runs this setup quietly with /relaunch=1 once TraceDraw has
; exited; the new version then starts by itself (the entry above is for the wizard).
Filename: "{app}\tracedraw.exe"; Flags: nowait shellexec runasoriginaluser; Check: RelaunchRequested

[Code]
var
  LanguagePage: TWizardPage;
  LanguageCombo: TNewComboBox;
  LanguageCodes: TStringList;

// TraceDraw's language for a Windows language id (GetUILanguage): the
// same table as language_for_langid in apps/tracedraw/src/i18n.rs (a
// test there reads this one). '' for a language TraceDraw does not have.
function AppLanguageForLangId(Id: Integer): String;
begin
  case Id of
    $0404: Result := 'zh-TW';
    $0C04: Result := 'zh-TW';
    $1404: Result := 'zh-TW';
    $7C04: Result := 'zh-TW';
  else
    case Id and $3FF of
      $09: Result := 'en';
      $04: Result := 'zh-CN';
      $39: Result := 'hi';
      $0A: Result := 'es';
      $0C: Result := 'fr';
      $01: Result := 'ar';
      $45: Result := 'bn';
      $19: Result := 'ru';
      $16: Result := 'pt-BR';
      $21: Result := 'id';
      $07: Result := 'de';
      $11: Result := 'ja';
      $10: Result := 'it';
      $13: Result := 'nl';
      $15: Result := 'pl';
      $1F: Result := 'tr';
      $05: Result := 'cs';
      $54: Result := 'lo';
      $2A: Result := 'vi';
      $22: Result := 'uk';
    else
      Result := '';
    end;
  end;
end;

procedure AddAppLanguage(Code, Name: String);
begin
  LanguageCodes.Add(Code);
  LanguageCombo.Items.Add(Name);
end;

// The "TraceDraw's language" page: System language (with the language it
// gives here) first and selected, then every interface language. A
// reinstall selects what the last install recorded; /APPLANGUAGE=<code>
// or /APPLANGUAGE=system on the command line selects that (for silent
// installs).
procedure InitializeWizard;
var
  SystemCode, Wanted: String;
  Index: Integer;
begin
  LanguagePage := CreateCustomPage(wpSelectTasks,
    CustomMessage('AppLanguageTitle'), CustomMessage('AppLanguageDescription'));
  LanguageCombo := TNewComboBox.Create(LanguagePage);
  LanguageCombo.Parent := LanguagePage.Surface;
  LanguageCombo.Style := csDropDownList;
  LanguageCombo.Width := LanguagePage.SurfaceWidth;
  LanguageCodes := TStringList.Create;
  LanguageCodes.Add('system');
  LanguageCombo.Items.Add(CustomMessage('AppLanguageSystem'));
  // The same languages and order as LANGUAGES in i18n.rs.
  AddAppLanguage('en', 'English');
  AddAppLanguage('zh-CN', '中文（简体）');
  AddAppLanguage('hi', 'हिन्दी');
  AddAppLanguage('es', 'Español');
  AddAppLanguage('fr', 'Français');
  AddAppLanguage('ar', 'العربية');
  AddAppLanguage('bn', 'বাংলা');
  AddAppLanguage('ru', 'Русский');
  AddAppLanguage('pt-BR', 'Português (Brasil)');
  AddAppLanguage('id', 'Bahasa Indonesia');
  AddAppLanguage('de', 'Deutsch');
  AddAppLanguage('ja', '日本語');
  AddAppLanguage('zh-TW', '中文（繁體）');
  AddAppLanguage('it', 'Italiano');
  AddAppLanguage('nl', 'Nederlands');
  AddAppLanguage('pl', 'Polski');
  AddAppLanguage('tr', 'Türkçe');
  AddAppLanguage('cs', 'Čeština');
  AddAppLanguage('lo', 'ພາສາລາວ');
  AddAppLanguage('vi', 'Tiếng Việt');
  AddAppLanguage('eo', 'Esperanto');
  AddAppLanguage('uk', 'Українська');
  SystemCode := AppLanguageForLangId(GetUILanguage);
  if SystemCode = '' then
    SystemCode := 'en';
  Index := LanguageCodes.IndexOf(SystemCode);
  if Index > 0 then
    LanguageCombo.Items[0] := CustomMessage('AppLanguageSystem') + ' (' + LanguageCombo.Items[Index] + ')';
  Wanted := ExpandConstant('{param:APPLANGUAGE|}');
  if Wanted = '' then
    if not RegQueryStringValue(HKA, 'Software\TraceDraw', 'InstallerLanguage', Wanted) then
      Wanted := '';
  Index := LanguageCodes.IndexOf(Trim(Wanted));
  if Index < 0 then
    Index := 0;
  LanguageCombo.ItemIndex := Index;
end;

// What the [Registry] entry records: 'system' or a language code.
function ChosenAppLanguage(Param: String): String;
begin
  Result := 'system';
  if (LanguageCombo <> nil) and (LanguageCombo.ItemIndex >= 0) and
     (LanguageCombo.ItemIndex < LanguageCodes.Count) then
    Result := LanguageCodes[LanguageCombo.ItemIndex];
end;

// The thumbnail handler of the copy installed before is loaded by Explorer
// whenever it shows .cdr or .tdraw pictures, and a file in use stopped a
// quiet update with no word (Help > Updates runs this setup with
// /SUPPRESSMSGBOXES, which answers Abort; restartreplace needs
// administrative rights, which an install for the current user has not,
// #3816). A loaded file can still be renamed, so it is moved aside before
// the files go in; the copies moved aside are deleted at the end when
// nothing holds them any more, else by TraceDraw at its next start.
procedure MoveAsideInUse(Name: String);
var
  Path, Aside: String;
  N: Integer;
begin
  Path := ExpandConstant('{app}\') + Name;
  if not FileExists(Path) then
    exit;
  if DeleteFile(Path) then
    exit;
  N := 0;
  repeat
    N := N + 1;
    Aside := Path + '.' + IntToStr(N) + '.old';
  until (not FileExists(Aside)) or (N > 99);
  if RenameFile(Path, Aside) then
    Log('Moved ' + Path + ' aside to ' + Aside)
  else
    Log('Could not move ' + Path + ' aside');
end;

procedure DeleteMovedAside(Name: String);
var
  FindRec: TFindRec;
begin
  if FindFirst(ExpandConstant('{app}\') + Name + '.*.old', FindRec) then
  begin
    try
      repeat
        DeleteFile(ExpandConstant('{app}\') + FindRec.Name);
      until not FindNext(FindRec);
    finally
      FindClose(FindRec);
    end;
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssInstall then
    MoveAsideInUse('tracedraw_thumbnailer.dll')
  else if CurStep = ssPostInstall then
    DeleteMovedAside('tracedraw_thumbnailer.dll');
end;

// Help > Updates > Install runs this setup with /relaunch=1: the new
// version then starts by itself (the [Run] entry that checks this).
function RelaunchRequested: Boolean;
begin
  Result := ExpandConstant('{param:relaunch|0}') = '1';
end;

function PathRegKey(Param: string): string;
begin
  if IsAdminInstallMode then
    Result := 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment'
  else
    Result := 'Environment';
end;

function NeedsAddPath(Param: string): boolean;
var
  OrigPath: string;
  Root: Integer;
begin
  if IsAdminInstallMode then Root := HKLM else Root := HKCU;
  if not RegQueryStringValue(Root, PathRegKey(''), 'Path', OrigPath) then
  begin
    Result := True;
    exit;
  end;
  Result := Pos(';' + Uppercase(ExpandConstant(Param)) + ';', ';' + Uppercase(OrigPath) + ';') = 0;
end;

#if Arch == "x86"
// The 32-bit setup on 64-bit Windows: recommend the x86_64 setup, go on
// when the user says so.
function InitializeSetup(): Boolean;
begin
  Result := True;
  if IsWin64 then
    Result := MsgBox(CustomMessage('Use64Setup'), mbConfirmation, MB_YESNO) = IDYES;
end;
#endif
