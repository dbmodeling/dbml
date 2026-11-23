//! Caminhos da instalação: tudo derivado do diretório do app.

use std::path::{Path, PathBuf};

use crate::constants::{
    APP_EXE, FLAG_FILE, HELPER_EXE, INSTALL_DIR, LOG_FILE, ManagedFile, OLD_DIR, TOOLS_DIR,
    UPDATES_DIR,
};

/// Diretório de instalação (`{app}`) e os caminhos derivados dele.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    app_dir: PathBuf,
}

impl Layout {
    /// Instalação em `app_dir`.
    #[must_use]
    pub fn new(app_dir: impl Into<PathBuf>) -> Self {
        Self {
            app_dir: app_dir.into(),
        }
    }

    /// A partir do executável do **helper** (`{app}\tools\auto_update_helper.exe` → `{app}`).
    #[must_use]
    pub fn from_helper_exe(helper_exe: &Path) -> Option<Self> {
        let tools = helper_exe.parent()?;
        Some(Self::new(tools.parent()?))
    }

    /// A partir do executável do **app** (`{app}\db-modeling.exe` → `{app}`).
    #[must_use]
    pub fn from_app_exe(app_exe: &Path) -> Option<Self> {
        Some(Self::new(app_exe.parent()?))
    }

    /// `{app}`.
    #[must_use]
    pub fn app_dir(&self) -> &Path {
        &self.app_dir
    }

    /// `{app}\updates`.
    #[must_use]
    pub fn updates_dir(&self) -> PathBuf {
        self.app_dir.join(UPDATES_DIR)
    }

    /// `{app}\updates\versions.txt`.
    #[must_use]
    pub fn flag_file(&self) -> PathBuf {
        self.updates_dir().join(FLAG_FILE)
    }

    /// `{app}\install`.
    #[must_use]
    pub fn install_dir(&self) -> PathBuf {
        self.app_dir.join(INSTALL_DIR)
    }

    /// `{app}\old`.
    #[must_use]
    pub fn old_dir(&self) -> PathBuf {
        self.app_dir.join(OLD_DIR)
    }

    /// `{app}\db-modeling.exe`.
    #[must_use]
    pub fn app_exe(&self) -> PathBuf {
        self.app_dir.join(APP_EXE)
    }

    /// `{app}\tools\auto_update_helper.exe`.
    #[must_use]
    pub fn helper_exe(&self) -> PathBuf {
        self.app_dir.join(TOOLS_DIR).join(HELPER_EXE)
    }

    /// `{app}\tools\auto_update_helper.log`.
    #[must_use]
    pub fn log_file(&self) -> PathBuf {
        self.app_dir.join(TOOLS_DIR).join(LOG_FILE)
    }

    /// Caminho de um arquivo gerenciado sob `root` (`{app}`, `{app}\install` ou `{app}\old`).
    #[must_use]
    pub fn under(root: &Path, file: ManagedFile) -> PathBuf {
        file.iter()
            .fold(root.to_path_buf(), |path, part| path.join(part))
    }

    /// Arquivo gerenciado na instalação em uso.
    #[must_use]
    pub fn live(&self, file: ManagedFile) -> PathBuf {
        Self::under(&self.app_dir, file)
    }

    /// Arquivo gerenciado em `{app}\install`.
    #[must_use]
    pub fn staged(&self, file: ManagedFile) -> PathBuf {
        Self::under(&self.install_dir(), file)
    }

    /// Arquivo gerenciado em `{app}\old`.
    #[must_use]
    pub fn backup(&self, file: ManagedFile) -> PathBuf {
        Self::under(&self.old_dir(), file)
    }
}

/// Nome legível de um arquivo gerenciado (`tools\auto_update_helper.exe`).
#[must_use]
pub fn display_name(file: ManagedFile) -> String {
    file.join(crate::constants::DISPLAY_SEPARATOR)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{CLI_FILE, MANAGED_FILES};

    #[test]
    fn deriva_tudo_do_diretorio_do_app() {
        let layout = Layout::new("/app");
        assert_eq!(layout.flag_file(), Path::new("/app/updates/versions.txt"));
        assert_eq!(layout.install_dir(), Path::new("/app/install"));
        assert_eq!(layout.old_dir(), Path::new("/app/old"));
        assert_eq!(layout.live(CLI_FILE), Path::new("/app/dbml.exe"));
        assert_eq!(
            layout.staged(MANAGED_FILES[3]),
            Path::new("/app/install/tools/auto_update_helper.exe")
        );
        assert_eq!(
            layout.backup(MANAGED_FILES[3]),
            Path::new("/app/old/tools/auto_update_helper.exe")
        );
    }

    #[test]
    fn app_dir_vem_do_executavel() {
        let from_helper =
            Layout::from_helper_exe(Path::new("/app/tools/auto_update_helper.exe")).unwrap();
        let from_app = Layout::from_app_exe(Path::new("/app/db-modeling.exe")).unwrap();
        assert_eq!(from_helper, from_app);
        assert_eq!(
            from_helper.helper_exe(),
            Path::new("/app/tools/auto_update_helper.exe")
        );
        assert!(Layout::from_helper_exe(Path::new("auto_update_helper.exe")).is_none());
    }

    #[test]
    fn nome_para_exibicao_usa_barra_invertida() {
        assert_eq!(
            display_name(MANAGED_FILES[3]),
            "tools\\auto_update_helper.exe"
        );
        assert_eq!(display_name(CLI_FILE), "dbml.exe");
    }
}
