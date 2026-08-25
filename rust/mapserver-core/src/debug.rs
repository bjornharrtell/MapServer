//! Debug-level scaffolding ported from `src/mapdebug.c`.

use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Default)]
struct DebugState {
    global_debug_level: i32,
    error_file: Option<String>,
}

fn state() -> &'static Mutex<DebugState> {
    static STATE: OnceLock<Mutex<DebugState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(DebugState::default()))
}

pub fn set_error_file(error_file: Option<&str>) {
    let mut state = state().lock().expect("debug state lock poisoned");
    state.error_file = error_file.map(ToOwned::to_owned);
}

pub fn error_file() -> Option<String> {
    state()
        .lock()
        .expect("debug state lock poisoned")
        .error_file
        .clone()
}

pub fn set_global_debug_level(level: i32) {
    state()
        .lock()
        .expect("debug state lock poisoned")
        .global_debug_level = level;
}

pub fn global_debug_level() -> i32 {
    state()
        .lock()
        .expect("debug state lock poisoned")
        .global_debug_level
}

pub fn init_from_env() {
    if let Ok(path) = std::env::var("MS_ERRORFILE") {
        set_error_file(Some(&path));
    }
    if let Ok(level) = std::env::var("MS_DEBUGLEVEL") {
        // C uses atoi(): invalid values become 0.
        set_global_debug_level(level.parse::<i32>().unwrap_or(0));
    }
}

pub fn should_log(level: i32) -> bool {
    level <= global_debug_level()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn global_level_round_trips() {
        let _guard = test_lock().lock().expect("test lock poisoned");
        set_global_debug_level(5);
        assert_eq!(global_debug_level(), 5);
    }

    #[test]
    fn should_log_respects_global_level() {
        let _guard = test_lock().lock().expect("test lock poisoned");
        set_global_debug_level(20);
        assert!(should_log(1));
        assert!(!should_log(21));
    }

    #[test]
    fn error_file_round_trips() {
        let _guard = test_lock().lock().expect("test lock poisoned");
        set_error_file(Some("stderr"));
        assert_eq!(error_file(), Some("stderr".to_string()));

        set_error_file(None);
        assert_eq!(error_file(), None);
    }
}
