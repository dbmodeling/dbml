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

; Fecha o app automaticamente antes de atualizar
CloseApplications=yes
CloseApplicationsFilter=*.exe
RestartApplications=no

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
; Executável principal Desktop compilado pelo Cargo
Source: "..\..\target\release\app.exe"; DestDir: "{app}"; DestName: "{#AppExeName}"; Flags: ignoreversion
; Executável CLI Web Server (dbml.exe)
Source: "..\..\target\release\dbml.exe"; DestDir: "{app}"; DestName: "dbml.exe"; Flags: ignoreversion
; Ícone da aplicação para atalhos e desinstalador
Source: "..\..\crates\app\assets\brand\icon.ico"; DestDir: "{app}"; Flags: ignoreversion

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
