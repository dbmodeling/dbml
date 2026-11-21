//! Ciclo de vida das tasks em segundo plano (sockets e watcher): todas ficam num `JoinSet`, e o
//! encerramento sinaliza o fim, espera as tasks e só então devolve (as gravações pendentes
//! terminam porque seguram o lock do documento).

use std::future::Future;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinSet;

pub struct Lifecycle {
    tasks: Mutex<JoinSet<()>>,
    shutdown: watch::Sender<bool>,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self {
            tasks: Mutex::new(JoinSet::new()),
            shutdown: watch::channel(false).0,
        }
    }
}

impl Lifecycle {
    /// Lança `work` numa task rastreada; tasks já concluídas são colhidas aqui para o conjunto
    /// não crescer indefinidamente.
    ///
    /// # Panics
    ///
    /// Entra em pânico se o mutex interno estiver envenenado.
    pub fn spawn<F>(&self, work: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let mut tasks = self.tasks.lock().expect("lifecycle mutex poisoned");
        while tasks.try_join_next().is_some() {}
        tasks.spawn(work);
    }

    /// Receptor que muda para `true` quando o encerramento começa.
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<bool> {
        self.shutdown.subscribe()
    }

    /// Avisa as tasks de que o servidor vai parar (idempotente).
    pub fn begin_shutdown(&self) {
        self.shutdown.send_replace(true);
    }

    /// Número de tasks rastreadas ainda em andamento (aproximado).
    ///
    /// # Panics
    ///
    /// Entra em pânico se o mutex interno estiver envenenado.
    #[must_use]
    pub fn running(&self) -> usize {
        let mut tasks = self.tasks.lock().expect("lifecycle mutex poisoned");
        while tasks.try_join_next().is_some() {}
        tasks.len()
    }

    /// Sinaliza o encerramento e espera as tasks rastreadas por até `grace`; as que não
    /// terminarem nesse prazo são abortadas ao soltar o conjunto.
    ///
    /// # Panics
    ///
    /// Entra em pânico se o mutex interno estiver envenenado.
    pub async fn shutdown(&self, grace: Duration) {
        self.begin_shutdown();
        let mut tasks = std::mem::take(&mut *self.tasks.lock().expect("lifecycle mutex poisoned"));
        let _ =
            tokio::time::timeout(grace, async { while tasks.join_next().await.is_some() {} }).await;
        tasks.abort_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test]
    async fn shutdown_waits_for_tracked_tasks_and_signals_them() {
        let life = Arc::new(Lifecycle::default());
        let done = Arc::new(AtomicBool::new(false));
        let mut rx = life.subscribe();
        let flag = Arc::clone(&done);
        life.spawn(async move {
            let _ = rx.changed().await;
            tokio::time::sleep(Duration::from_millis(50)).await;
            flag.store(true, Ordering::SeqCst);
        });
        assert_eq!(life.running(), 1);
        life.shutdown(Duration::from_secs(2)).await;
        assert!(done.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn shutdown_gives_up_after_the_grace_period() {
        let life = Lifecycle::default();
        life.spawn(std::future::pending());
        let started = std::time::Instant::now();
        life.shutdown(Duration::from_millis(50)).await;
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
