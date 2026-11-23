//! Espera com prazo (o app usa para o instalador silencioso; o relógio entra por parâmetro).

use std::time::Duration;

/// Repete `done` a cada `interval` até devolver `true` ou estourar `timeout`. Devolve `true` se
/// `done` chegou a ser verdadeiro, `false` se o prazo acabou antes.
pub fn wait_until(
    timeout: Duration,
    interval: Duration,
    sleep: &mut dyn FnMut(Duration),
    done: &mut dyn FnMut() -> bool,
) -> bool {
    let mut waited = Duration::ZERO;
    loop {
        if done() {
            return true;
        }
        if waited >= timeout {
            return false;
        }
        sleep(interval);
        waited += interval;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn termina_assim_que_a_condicao_vale() {
        let mut checks = 0;
        let mut slept = Duration::ZERO;
        let ok = wait_until(
            Duration::from_secs(10),
            Duration::from_secs(1),
            &mut |d| slept += d,
            &mut || {
                checks += 1;
                checks == 3
            },
        );
        assert!(ok);
        assert_eq!(slept, Duration::from_secs(2));
    }

    #[test]
    fn desiste_no_prazo() {
        let mut slept = Duration::ZERO;
        let ok = wait_until(
            Duration::from_secs(3),
            Duration::from_secs(1),
            &mut |d| slept += d,
            &mut || false,
        );
        assert!(!ok);
        assert_eq!(slept, Duration::from_secs(3));
    }
}
