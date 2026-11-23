; Script de instalação oficial Inno Setup para o DB Modeling no Windows 10/11
; Suporta instalação nova, atualização automática e desinstalação completa limpa.

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif

#define AppName "DB Modeling"
#define AppPublisher "DB Modeling Team"
#define AppURL "https://github.com/dbmodeling/dbml"
#define AppExeName "db-modeling.exe"
#define AppId "{{C5963E01-8A18-4DC5-8B29-4BE558F49221}}"
#define HelperExeName "auto_update_helper.exe"

; Modo de atualização automática (ticket 05 do auto-update; modelo do Zed). O app em execução
; roda este instalador com `/verysilent /update=true /nocloseapplications /norestart`:
;  - os arquivos vão para {app}\install (não sobrescrevem o app em uso);
;  - o instalador não espera o app fechar (GetAppMutex vazio) e não o fecha;
;  - no fim grava {app}\updates\versions.txt (só a existência importa; o conteúdo é a versão).
; A troca final é do tools\auto_update_helper.exe, ao sair do app ou em "Reiniciar para
; atualizar". Os nomes abaixo precisam bater com crates/auto-update-helper/src/constants.rs.
#define UpdateInstallDir "install"
#define UpdateFlagDir "updates"
#define UpdateFlagFile "versions.txt"

[Setup]
AppId={#AppId}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} v{#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppURL}
AppSupportURL={#AppURL}
AppUpdatesURL={#AppURL}/releases
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
AllowNoIcons=yes
OutputDir=..\..\dist
OutputBaseFilename=db-modeling-windows-setup
SetupIconFile=..\..\crates\app\assets\brand\icon.ico
UninstallDisplayIcon={app}\{#AppExeName}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

; Permite ao usuário escolher instalar por usuário ou para toda a máquina
PrivilegesRequiredOverridesAllowed=commandline dialog
PrivilegesRequired=lowest

; Fecha o app automaticamente antes de uma (re)instalação manual. Na atualização automática o
; updater passa /nocloseapplications (que sobrepõe esta linha): o app em execução não pode ser
; fechado pelo instalador, ele só vai para {app}\install.
CloseApplications=yes
CloseApplicationsFilter=*.exe
RestartApplications=no
; Não espera o app fechar em /update=true (ver GetAppMutex)
AppMutex={code:GetAppMutex}
; Mantém as tarefas da instalação anterior (PATH, associação .dbml, atalhos) ao atualizar
UsePreviousTasks=yes
UsePreviousAppDir=yes

; Registro de Desinstalação Oficial do Windows
UninstallDisplayName={#AppName}
ChangesAssociations=yes

[Languages]
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "startmenuicon"; Description: "Criar atalho no Menu Iniciar"; GroupDescription: "{cm:AdditionalIcons}"
Name: "associate_dbml"; Description: "Associar arquivos DBML (.dbml) ao DB Modeling"; GroupDescription: "Associações de Arquivo:"
Name: "addtopath"; Description: "Adicionar CLI (dbml) ao PATH do sistema (permite rodar 'dbml run schema.dbml')"; GroupDescription: "Ferramentas de Linha de Comando (CLI Web Server):"

[Files]
; Em /update=true tudo vai para {app}\install (GetInstallDir). O auto_update_helper só troca os
; arquivos de MANAGED_FILES (crates/auto-update-helper/src/constants.rs): ao incluir um arquivo
; novo aqui, inclua-o lá também.
; Executável principal Desktop compilado pelo Cargo
Source: "..\..\target\release\app.exe"; DestDir: "{code:GetInstallDir|{app}}"; DestName: "{#AppExeName}"; Flags: ignoreversion
; Executável CLI Web Server (dbml.exe)
Source: "..\..\target\release\dbml.exe"; DestDir: "{code:GetInstallDir|{app}}"; DestName: "dbml.exe"; Flags: ignoreversion
; Ícone da aplicação para atalhos e desinstalador
Source: "..\..\crates\app\assets\brand\icon.ico"; DestDir: "{code:GetInstallDir|{app}}"; Flags: ignoreversion
; Helper da atualização automática (assinado no SignPath ANTES de entrar aqui; ver release.yml)
Source: "..\..\target\release\{#HelperExeName}"; DestDir: "{code:GetInstallDir|{app}}\tools"; Flags: ignoreversion

[UninstallDelete]
; O log do Inno de uma atualização registra os arquivos em {app}\install, que depois o helper move
; para o lugar; estas linhas garantem que o desinstalador remove tudo, inclusive restos de falha.
Type: filesandordirs; Name: "{app}\tools"
Type: filesandordirs; Name: "{app}\{#UpdateInstallDir}"
Type: filesandordirs; Name: "{app}\{#UpdateFlagDir}"
Type: filesandordirs; Name: "{app}\old"

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"; Tasks: startmenuicon
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; Tasks: desktopicon

[Registry]
; Associação da extensão de arquivo .dbml com o DB Modeling
Root: HKA; Subkey: "Software\Classes\.dbml"; ValueType: string; ValueName: ""; ValueData: "DBModeling.Document"; Flags: uninsdeletevalue; Tasks: associate_dbml
Root: HKA; Subkey: "Software\Classes\.dbml\OpenWithProgids"; ValueType: string; ValueName: "DBModeling.Document"; ValueData: ""; Flags: uninsdeletevalue; Tasks: associate_dbml
Root: HKA; Subkey: "Software\Classes\DBModeling.Document"; ValueType: string; ValueName: ""; ValueData: "Documento DBML"; Flags: uninsdeletekey; Tasks: associate_dbml
Root: HKA; Subkey: "Software\Classes\DBModeling.Document\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icon.ico,0"; Flags: uninsdeletekey; Tasks: associate_dbml
Root: HKA; Subkey: "Software\Classes\DBModeling.Document\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""; Flags: uninsdeletekey; Tasks: associate_dbml
; Adiciona {app} ao PATH do usuário para disponibilizar o comando `dbml` no terminal
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; Tasks: addtopath; Check: NeedsAddPath(ExpandConstant('{app}'))

[Code]
{ Atualização automática: instalação silenciosa chamada pelo app com /update=true. }
function IsUpdating(): Boolean;
begin
  Result := (ExpandConstant('{param:update|false}') = 'true') and WizardSilent();
end;

{ Pasta de destino dos arquivos: {app}\install na atualização, {app} na instalação normal. }
function GetInstallDir(Param: String): String;
begin
  if IsUpdating() then
    Result := AddBackslash(Param) + '{#UpdateInstallDir}'
  else
    Result := Param;
end;

{ Mutex que o instalador espera ficar livre. O app não cria mutex nenhum; vazio = não espera
  (na atualização o app continua aberto de propósito). Ponto único a mudar se um dia houver um. }
function GetAppMutex(Param: String): String;
begin
  Result := '';
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  FlagDir: String;
begin
  if not IsUpdating() then
    exit;
  FlagDir := AddBackslash(ExpandConstant('{app}')) + '{#UpdateFlagDir}';
  if CurStep = ssInstall then
  begin
    { a flag só vale com a cópia completa: apaga a de uma tentativa anterior antes de copiar }
    DeleteFile(AddBackslash(FlagDir) + '{#UpdateFlagFile}');
  end
  else if CurStep = ssPostInstall then
  begin
    ForceDirectories(FlagDir);
    SaveStringToFile(AddBackslash(FlagDir) + '{#UpdateFlagFile}', '{#AppVersion}', False);
  end;
end;

function NeedsAddPath(Param: string): boolean;
var
  OrigPath: string;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', OrigPath) then
  begin
    Result := True;
    exit;
  end;
  Result := Pos(';' + Param + ';', ';' + OrigPath + ';') = 0;
end;

[Run]
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(AppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent
