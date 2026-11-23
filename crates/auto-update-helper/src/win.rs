//! Parte do helper que só existe no Windows: Restart Manager, espera por PID, janela
//! "Atualizando…", caixa de erro e a ligação com [`crate::driver`].
//!
//! **NÃO VERIFICADO no Windows** (escrito no Linux, sem alvo `windows-*` instalado): as
//! assinaturas de `windows-sys` 0.61.2 foram conferidas no código-fonte da crate; compilar e testar
//! no `windows-latest` do CI e manualmente (checklist do ticket 05).

use std::ffi::{OsStr, OsString};
use std::io;
use std::os::windows::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
use windows_sys::Win32::System::RestartManager::{
    CCH_RM_SESSION_KEY, RmEndSession, RmRegisterResources, RmShutdown, RmStartSession,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_SYNCHRONIZE, Sleep, WaitForSingleObject,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, DispatchMessageW, GetSystemMetrics, MB_ICONERROR, MB_OK, MSG,
    MessageBoxW, PM_REMOVE, PeekMessageW, SM_CXSCREEN, SM_CYSCREEN, TranslateMessage, WS_BORDER,
    WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

use crate::args;
use crate::constants::{
    DIALOG_CLASS, DIALOG_HEIGHT_PX, DIALOG_POLL_MS, DIALOG_WIDTH_PX, EXIT_FAILED, EXIT_OK,
    EXIT_USAGE, SS_CENTER, SS_CENTERIMAGE, USAGE_TEXT, WAIT_PID_TIMEOUT_MS,
};
use crate::driver::{self, Env, Outcome};
use crate::fsops::RealFs;
use crate::jobs::RetryPolicy;
use crate::layout::Layout;
use crate::logfile;
use crate::messages::{Lang, Problem, describe, error_title, updating};
use crate::recovery::{Startup, inspect};

/// UTF-16 terminado em nulo, como as APIs `W` esperam.
fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

/// Idioma da interface do usuário.
fn user_lang() -> Lang {
    // SAFETY: função sem argumentos nem ponteiros; só lê a configuração do usuário.
    Lang::from_langid(unsafe { GetUserDefaultUILanguage() })
}

/// Caixa de erro modal (sem janela dona).
fn message_box(text: &str, title: &str) {
    let text = wide(OsStr::new(text));
    let title = wide(OsStr::new(title));
    // SAFETY: os dois buffers são UTF-16 terminados em nulo e vivem até o fim da chamada.
    unsafe {
        MessageBoxW(
            ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

/// Espera o processo `pid` terminar (até [`WAIT_PID_TIMEOUT_MS`]). Se não der para abri-lo (já
/// saiu, ou sem permissão), segue: a troca funciona de qualquer jeito, só com mais tentativas.
fn wait_for_pid(pid: u32) {
    // SAFETY: `OpenProcess` devolve um HANDLE próprio (ou nulo); fechado logo depois.
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if handle.is_null() {
            return;
        }
        WaitForSingleObject(handle, WAIT_PID_TIMEOUT_MS);
        windows_sys::Win32::Foundation::CloseHandle(handle);
    }
}

/// Pede ao Restart Manager que feche quem estiver usando `files` (melhor esforço, como no Zed:
/// `RmShutdown` com flags 0, sem forçar). Devolve o código de erro do primeiro passo que falhar.
fn release_with_restart_manager(files: &[PathBuf]) -> Result<(), u32> {
    let wide_files: Vec<Vec<u16>> = files.iter().map(|file| wide(file.as_os_str())).collect();
    let pointers: Vec<*const u16> = wide_files.iter().map(Vec::as_ptr).collect();
    let count = u32::try_from(pointers.len()).map_err(|_| u32::MAX)?;
    let mut session = 0u32;
    let mut key = [0u16; CCH_RM_SESSION_KEY as usize + 1];
    // SAFETY: `session` e `key` (CCH_RM_SESSION_KEY + 1 posições, como a API exige) vivem durante
    // toda a sessão; os ponteiros de `pointers` apontam para `wide_files`, vivo até o fim.
    unsafe {
        let started = RmStartSession(&raw mut session, 0, key.as_mut_ptr());
        if started != ERROR_SUCCESS {
            return Err(started);
        }
        let registered = RmRegisterResources(
            session,
            count,
            pointers.as_ptr(),
            0,
            ptr::null(),
            0,
            ptr::null(),
        );
        let result = if registered == ERROR_SUCCESS {
            match RmShutdown(session, 0, None) {
                ERROR_SUCCESS => Ok(()),
                code => Err(code),
            }
        } else {
            Err(registered)
        };
        RmEndSession(session);
        result
    }
}

/// Janela "Atualizando…": um `STATIC` de topo (sem classe nem procedimento próprios), com a
/// bomba de mensagens numa thread. Fecha ao ser descartada.
struct Dialog {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Dialog {
    fn show(text: &str) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let text = text.to_owned();
        let thread = thread::Builder::new()
            .spawn(move || dialog_loop(&text, &flag))
            .ok();
        Self { stop, thread }
    }
}

impl Drop for Dialog {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn dialog_loop(text: &str, stop: &AtomicBool) {
    let class = wide(OsStr::new(DIALOG_CLASS));
    let title = wide(OsStr::new(text));
    // SAFETY: os buffers vivem durante a chamada; a janela é criada, bombeada e destruída nesta
    // mesma thread (requisito do Win32); `MSG` é preenchido por `PeekMessageW`.
    unsafe {
        let x = (GetSystemMetrics(SM_CXSCREEN) - DIALOG_WIDTH_PX) / 2;
        let y = (GetSystemMetrics(SM_CYSCREEN) - DIALOG_HEIGHT_PX) / 2;
        let window = CreateWindowExW(
            WS_EX_TOPMOST,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP | WS_BORDER | WS_VISIBLE | SS_CENTER | SS_CENTERIMAGE,
            x,
            y,
            DIALOG_WIDTH_PX,
            DIALOG_HEIGHT_PX,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null(),
        );
        if window.is_null() {
            return;
        }
        let mut message = MSG::default();
        while !stop.load(Ordering::Acquire) {
            while PeekMessageW(&raw mut message, ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&raw const message);
                DispatchMessageW(&raw const message);
            }
            Sleep(DIALOG_POLL_MS);
        }
        DestroyWindow(window);
    }
}

/// [`Env`] real: espera por PID, Restart Manager, `Command` e o log em `tools\`.
struct WinEnv {
    log_file: PathBuf,
}

impl Env for WinEnv {
    fn sleep(&mut self, duration: Duration) {
        thread::sleep(duration);
    }

    fn wait_for_exit(&mut self, pid: u32) {
        wait_for_pid(pid);
    }

    fn release_handles(&mut self, files: &[PathBuf]) {
        if let Err(code) = release_with_restart_manager(files) {
            self.log(&format!("Restart Manager: erro {code} (segue sem ele)"));
        }
    }

    fn launch(&mut self, exe: &Path, args: &[OsString]) -> io::Result<()> {
        Command::new(exe).args(args).spawn().map(drop)
    }

    fn log(&mut self, line: &str) {
        let _ = logfile::append(&self.log_file, line);
    }
}

/// Ponto de entrada do binário no Windows.
pub fn main_entry() -> ExitCode {
    let lang = user_lang();
    let options = match args::parse(std::env::args_os().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            message_box(&format!("{error}\n{USAGE_TEXT}"), error_title(lang));
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let Some(layout) = std::env::current_exe()
        .ok()
        .and_then(|exe| Layout::from_helper_exe(&exe))
    else {
        message_box(USAGE_TEXT, error_title(lang));
        return ExitCode::from(EXIT_USAGE);
    };
    let mut env = WinEnv {
        log_file: layout.log_file(),
    };
    // a janela só aparece se há troca a fazer (no encerramento sem atualização não pisca nada)
    let dialog =
        (inspect(&layout, &RealFs) == Startup::Pending).then(|| Dialog::show(updating(lang)));
    let outcome = driver::run(
        &layout,
        &options,
        &RetryPolicy::default(),
        &RealFs,
        &mut env,
    );
    drop(dialog);
    match outcome {
        Outcome::NothingToDo | Outcome::Applied { .. } => ExitCode::from(EXIT_OK),
        Outcome::Failed { problem, detail } => {
            message_box(&describe(lang, &problem, &detail), error_title(lang));
            // a troca em si deu certo quando só o app novo não abriu
            ExitCode::from(if problem == Problem::LaunchFailed {
                EXIT_OK
            } else {
                EXIT_FAILED
            })
        }
    }
}
