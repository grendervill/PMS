//! Failed-unlock throttling.
//!
//! Argon2id prices a single guess, but nothing stops an attacker (or a script
//! driving the UI) from trying thousands of passphrases in a row: 70 ms per
//! attempt on the default profile is 14 guesses per second. The biometric
//! prompt had the same gap — the five-attempt limit lived in the renderer and
//! was reset by reloading the page.
//!
//! Policy: the first three failures are free (a human mistyping), then the delay
//! grows 5 s → 15 s → 60 s → 5 min and stays at 5 min. State is persisted, so
//! restarting the app does not reset it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::commands::{ApiError, ApiResult};

/// Failures that do not cost anything.
const FREE_ATTEMPTS: u32 = 3;
const MAX_FAILURES: u32 = 1_000;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Entry {
    failures: u32,
    /// Wall-clock deadline, so the delay survives a restart.
    locked_until_ms: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct StoredState {
    #[serde(default)]
    vaults: HashMap<Uuid, Entry>,
}

#[derive(Debug, Default)]
struct State {
    entries: HashMap<Uuid, Entry>,
    store: Option<PathBuf>,
}

#[derive(Default)]
pub struct UnlockGuard {
    inner: Mutex<State>,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Delay in seconds for the given number of failures.
fn delay_seconds(failures: u32) -> i64 {
    match failures {
        0..=FREE_ATTEMPTS => 0,
        4 => 5,
        5 => 15,
        6 => 60,
        _ => 300,
    }
}

impl UnlockGuard {
    /// Loads the persisted counters. Values from the future (clock changed) are
    /// clamped, so a wrong system clock cannot lock the user out forever.
    pub fn attach(&self, store: Option<PathBuf>) {
        let mut entries = HashMap::new();
        if let Some(path) = store.as_deref() {
            if let Ok(bytes) = std::fs::read(path) {
                if let Ok(stored) = serde_json::from_slice::<StoredState>(&bytes) {
                    let limit = now_ms() + 300_000;
                    for (vault_id, mut entry) in stored.vaults {
                        entry.failures = entry.failures.min(MAX_FAILURES);
                        entry.locked_until_ms = entry.locked_until_ms.min(limit);
                        entries.insert(vault_id, entry);
                    }
                }
            }
        }
        if let Ok(mut state) = self.inner.lock() {
            state.entries = entries;
            state.store = store;
        }
    }

    /// Refuses the attempt while the vault is in its cool-down window.
    pub fn check(&self, vault_id: Uuid) -> ApiResult<()> {
        let Ok(state) = self.inner.lock() else {
            return Ok(());
        };
        let Some(entry) = state.entries.get(&vault_id) else {
            return Ok(());
        };
        let remaining_ms = entry.locked_until_ms - now_ms();
        if remaining_ms <= 0 {
            return Ok(());
        }
        let seconds = (remaining_ms + 999) / 1000;
        Err(ApiError::new(
            "unlock_locked",
            format!("Слишком много попыток. Повторите через {seconds} с."),
        ))
    }

    /// Records a failed attempt and returns the new cool-down in seconds.
    pub fn record_failure(&self, vault_id: Uuid) -> i64 {
        let Ok(mut state) = self.inner.lock() else {
            return 0;
        };
        let entry = state.entries.entry(vault_id).or_default();
        entry.failures = entry.failures.saturating_add(1).min(MAX_FAILURES);
        let delay = delay_seconds(entry.failures);
        if delay > 0 {
            entry.locked_until_ms = now_ms() + delay * 1000;
        }
        let snapshot = snapshot(&state);
        let store = state.store.clone();
        drop(state);
        persist(store, &snapshot);
        delay
    }

    pub fn record_success(&self, vault_id: Uuid) {
        let Ok(mut state) = self.inner.lock() else {
            return;
        };
        if state.entries.remove(&vault_id).is_none() {
            return;
        }
        let snapshot = snapshot(&state);
        let store = state.store.clone();
        drop(state);
        persist(store, &snapshot);
    }

    #[cfg(test)]
    fn failures(&self, vault_id: Uuid) -> u32 {
        self.inner
            .lock()
            .ok()
            .and_then(|state| state.entries.get(&vault_id).map(|entry| entry.failures))
            .unwrap_or(0)
    }
}

fn snapshot(state: &State) -> StoredState {
    StoredState {
        vaults: state.entries.clone(),
    }
}

fn persist(store: Option<PathBuf>, snapshot: &StoredState) {
    let Some(path) = store else {
        return;
    };
    let Ok(json) = serde_json::to_vec(snapshot) else {
        return;
    };
    let atomic = atomicwrites::AtomicFile::new(&path, atomicwrites::AllowOverwrite);
    let _ = atomic.write(|file| {
        use std::io::Write;
        file.write_all(&json)?;
        file.sync_all()
    });
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn guard() -> UnlockGuard {
        UnlockGuard::default()
    }

    #[test]
    fn the_first_failures_are_free() {
        let guard = guard();
        let vault = Uuid::new_v4();
        for expected in [1, 2, 3] {
            assert_eq!(guard.record_failure(vault), 0);
            assert_eq!(guard.failures(vault), expected);
            assert!(guard.check(vault).is_ok(), "no cool-down yet");
        }
    }

    #[test]
    fn the_delay_grows_and_then_locks() {
        let guard = guard();
        let vault = Uuid::new_v4();
        for _ in 0..3 {
            guard.record_failure(vault);
        }
        assert_eq!(guard.record_failure(vault), 5);
        assert_eq!(guard.record_failure(vault), 15);
        assert_eq!(guard.record_failure(vault), 60);
        assert_eq!(guard.record_failure(vault), 300);
        assert_eq!(guard.record_failure(vault), 300);

        let error = guard.check(vault).expect_err("must be locked");
        assert_eq!(error.code, "unlock_locked");
        assert!(error.message.contains('с'));
    }

    #[test]
    fn a_successful_unlock_clears_the_counter() {
        let guard = guard();
        let vault = Uuid::new_v4();
        for _ in 0..5 {
            guard.record_failure(vault);
        }
        assert!(guard.check(vault).is_err());
        guard.record_success(vault);
        assert!(guard.check(vault).is_ok());
        assert_eq!(guard.failures(vault), 0);
    }

    #[test]
    fn counters_survive_a_restart() {
        let dir = tempdir().unwrap();
        let store = dir.path().join("unlock-state.json");
        let vault = Uuid::new_v4();
        {
            let guard = guard();
            guard.attach(Some(store.clone()));
            for _ in 0..5 {
                guard.record_failure(vault);
            }
        }
        let guard = guard();
        guard.attach(Some(store));
        assert_eq!(guard.failures(vault), 5);
        assert!(guard.check(vault).is_err(), "the cool-down must survive");
    }

    #[test]
    fn a_clock_in_the_future_cannot_lock_the_vault_forever() {
        let dir = tempdir().unwrap();
        let store = dir.path().join("unlock-state.json");
        let vault = Uuid::new_v4();
        let payload = format!(
            r#"{{"vaults":{{"{vault}":{{"failures":900,"locked_until_ms":{}}}}}}}"#,
            now_ms() + 10 * 365 * 24 * 3600 * 1000
        );
        std::fs::write(&store, payload).unwrap();

        let guard = guard();
        guard.attach(Some(store));
        // Clamped to at most five minutes from now.
        let error = guard.check(vault).expect_err("still locked, but bounded");
        assert!(error.message.contains("300") || error.message.contains("299"));
    }
}
