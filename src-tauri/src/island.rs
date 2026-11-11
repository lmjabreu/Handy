//! Handy's recording state as a Live Activity in the Dynamic Island (macOS).
//!
//! When the overlay sits at the top of a Mac that has an island, the overlay
//! panel is shown as usual, which is what lets ActivityKit start an activity
//! from a background app, and the activity then carries the session: phase,
//! timer and live text. The overlay's own card is hidden while it runs. If
//! the activity can't start, the normal overlay stays.

#[cfg(target_os = "macos")]
mod imp {
    use crate::settings::{self, OverlayPosition};
    use serde::Serialize;
    use std::ffi::{c_char, c_int, CStr, CString};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    use tauri::{AppHandle, Emitter};

    extern "C" {
        fn handy_island_supported() -> c_int;
        fn handy_island_start(
            json: *const c_char,
            error_out: *mut c_char,
            error_len: c_int,
        ) -> c_int;
        fn handy_island_update(json: *const c_char);
        fn handy_island_end();
    }

    /// Live text updates are coalesced to this interval; ActivityKit was
    /// measured accepting four a second without ending the activity.
    const TEXT_INTERVAL_MS: u128 = 250;

    #[derive(Serialize, Clone)]
    #[serde(rename_all = "camelCase")]
    pub(super) struct ContentState {
        pub(super) phase: String,
        /// The phase's label in Handy's language; empty while listening.
        pub(super) label: String,
        pub(super) committed: String,
        pub(super) tentative: String,
        /// Seconds since 1970, decoded by the Swift side.
        pub(super) started_at: f64,
    }

    struct Session {
        state: ContentState,
        last_sent: Instant,
    }

    static SESSION: Mutex<Option<Session>> = Mutex::new(None);
    /// Handy's interface language when the session started (`app_language`).
    static LANGUAGE: Mutex<String> = Mutex::new(String::new());

    /// The overlay's own label for `phase`, from the same translations.
    pub(super) fn label_for(language: &str, phase: &str) -> String {
        let strings = crate::tray_i18n::get_tray_translations(Some(language.to_string()));
        match phase {
            "transcribing" => strings.overlay_transcribing,
            "processing" => strings.overlay_processing,
            _ => String::new(),
        }
    }

    fn current_label(phase: &str) -> String {
        label_for(&LANGUAGE.lock().unwrap_or_else(|e| e.into_inner()), phase)
    }

    fn json(state: &ContentState) -> Option<CString> {
        CString::new(serde_json::to_string(state).ok()?).ok()
    }

    fn send(state: &ContentState) {
        if let Some(payload) = json(state) {
            unsafe { handy_island_update(payload.as_ptr()) };
        }
    }

    fn session<R>(f: impl FnOnce(&mut Option<Session>) -> R) -> R {
        let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard)
    }

    /// Whether this session should go to the island: overlay at the top, and
    /// a Mac whose display has a camera housing with Live Activities on.
    pub fn wanted(app: &AppHandle) -> bool {
        if settings::get_settings(app).overlay_position != OverlayPosition::Top {
            return false;
        }
        let supported = unsafe { handy_island_supported() } == 1;
        if !supported {
            log::debug!("Dynamic Island not used: no camera housing or Live Activities are off");
        }
        supported
    }

    /// Bumped by `end`, so start retries left over from a finished session stop.
    static GENERATION: AtomicU64 = AtomicU64::new(0);
    /// The phase a pending retry should start with, kept current by `show`/`set_phase`.
    static PENDING_PHASE: Mutex<Option<String>> = Mutex::new(None);

    /// macOS only counts the overlay panel as visible a moment after it is
    /// ordered in, and ActivityKit refuses to start until then. These are the
    /// waits before each retry; together they cover about half a second.
    const VISIBILITY_RETRIES_MS: [u64; 3] = [120, 150, 250];

    fn now_secs() -> f64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    }

    /// Starts an activity in `phase`. Main thread only.
    fn try_start(phase: &str, started_at: f64) -> Result<(), String> {
        let state = ContentState {
            phase: phase.to_string(),
            label: current_label(phase),
            committed: String::new(),
            tentative: String::new(),
            started_at,
        };
        let payload = json(&state).ok_or_else(|| "could not encode state".to_string())?;
        let mut error = vec![0 as c_char; 256];
        if unsafe { handy_island_start(payload.as_ptr(), error.as_mut_ptr(), 256) } == 1 {
            session(|s| {
                *s = Some(Session {
                    state,
                    last_sent: Instant::now(),
                })
            });
            log::info!("Dynamic Island activity started ({phase})");
            Ok(())
        } else {
            Err(unsafe { CStr::from_ptr(error.as_ptr()) }
                .to_string_lossy()
                .into_owned())
        }
    }

    /// Retries a start refused for `visibility` while the same session is
    /// still showing, then tells the overlay to hide its card.
    fn retry_when_visible(app: &AppHandle, started_at: f64) {
        let generation = GENERATION.load(Ordering::SeqCst);
        let app = app.clone();
        std::thread::spawn(move || {
            for wait in VISIBILITY_RETRIES_MS {
                std::thread::sleep(Duration::from_millis(wait));
                let (tx, rx) = std::sync::mpsc::channel();
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || {
                    let phase = PENDING_PHASE.lock().unwrap_or_else(|e| e.into_inner()).clone();
                    let outcome = match phase {
                        _ if GENERATION.load(Ordering::SeqCst) != generation => Some(false),
                        None => Some(false),
                        Some(phase) => match try_start(&phase, started_at) {
                            Ok(()) => {
                                let _ = handle.emit_to("recording_overlay", "overlay-island-active", true);
                                Some(true)
                            }
                            Err(e) if e.contains("visibility") => None,
                            Err(e) => {
                                log::warn!("Dynamic Island activity did not start, keeping the overlay: {e}");
                                Some(false)
                            }
                        },
                    };
                    let _ = tx.send(outcome);
                });
                if let Ok(Some(_)) = rx.recv() {
                    return;
                }
            }
            log::warn!("Dynamic Island activity did not start: overlay never counted as visible");
        });
    }

    /// Call on the main thread right after the overlay panel is shown. Starts
    /// the activity (or moves a running one to `phase`) and returns whether
    /// the island is carrying the session now. When macOS doesn't count the
    /// panel as visible yet, it retries shortly and hides the card on success.
    pub fn show(app: &AppHandle, phase: &str) -> bool {
        if !wanted(app) {
            end();
            return false;
        }
        *PENDING_PHASE.lock().unwrap_or_else(|e| e.into_inner()) = Some(phase.to_string());
        *LANGUAGE.lock().unwrap_or_else(|e| e.into_inner()) =
            settings::get_settings(app).app_language;
        let running = session(|s| {
            if let Some(s) = s.as_mut() {
                s.state.phase = phase.to_string();
                s.state.label = current_label(phase);
                send(&s.state);
                s.last_sent = Instant::now();
                true
            } else {
                false
            }
        });
        if running {
            return true;
        }

        let started_at = now_secs();
        match try_start(phase, started_at) {
            Ok(()) => true,
            Err(e) if e.contains("visibility") => {
                log::debug!("Dynamic Island start waiting for the overlay to count as visible");
                retry_when_visible(app, started_at);
                false
            }
            Err(e) => {
                log::warn!("Dynamic Island activity did not start, keeping the overlay: {e}");
                false
            }
        }
    }

    /// Live transcript for the running activity, coalesced to TEXT_INTERVAL_MS.
    pub fn set_text(committed: &str, tentative: &str) {
        session(|s| {
            let Some(s) = s.as_mut() else { return };
            s.state.committed = committed.to_string();
            s.state.tentative = tentative.to_string();
            if s.last_sent.elapsed().as_millis() >= TEXT_INTERVAL_MS {
                send(&s.state);
                s.last_sent = Instant::now();
            }
        });
    }

    pub fn set_phase(phase: &str) {
        if let Some(pending) = PENDING_PHASE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            *pending = phase.to_string();
        }
        session(|s| {
            let Some(s) = s.as_mut() else { return };
            s.state.phase = phase.to_string();
            s.state.label = current_label(phase);
            log::debug!("Dynamic Island phase -> {phase}");
            send(&s.state);
            s.last_sent = Instant::now();
        });
    }

    pub fn end() {
        GENERATION.fetch_add(1, Ordering::SeqCst);
        *PENDING_PHASE.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let had = session(|s| s.take().is_some());
        if had {
            log::info!("Dynamic Island activity ended");
            unsafe { handy_island_end() };
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::imp::{label_for, ContentState};

    /// The island reuses the overlay's translated labels, not its own copy.
    #[test]
    fn working_labels_come_from_the_overlay_translations() {
        assert_eq!(label_for("en", "transcribing"), "Transcribing...");
        assert_eq!(label_for("en", "processing"), "Processing...");
        assert_eq!(label_for("en", "recording"), "");
        let german = label_for("de", "transcribing");
        assert!(!german.is_empty());
        assert_ne!(
            german, "Transcribing...",
            "German should not fall back to English"
        );
    }

    /// The Swift side decodes this JSON into HandyIslandAttributes.ContentState
    /// (island-widget/Sources/HandyIslandAttributes.swift) with seconds-since-1970
    /// dates. A renamed or retyped field fails there silently: the island shows
    /// nothing. Keep these names in step with that struct.
    #[test]
    fn content_state_json_matches_the_swift_struct() {
        let state = ContentState {
            phase: "recording".into(),
            label: String::new(),
            committed: "Hello".into(),
            tentative: "wor".into(),
            started_at: 1_789_000_000.5,
        };
        let value = serde_json::to_value(&state).unwrap();
        let mut keys: Vec<_> = value.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            ["committed", "label", "phase", "startedAt", "tentative"]
        );
        assert_eq!(value["startedAt"], 1_789_000_000.5);
        assert_eq!(value["phase"], "recording");
    }

    #[test]
    fn swift_struct_declares_the_same_fields() {
        let swift = include_str!("../island-widget/Sources/HandyIslandAttributes.swift");
        for field in [
            "var phase: String",
            "var label: String",
            "var committed: String",
            "var tentative: String",
            "var startedAt: Date",
        ] {
            assert!(
                swift.contains(field),
                "HandyIslandAttributes.swift lost `{field}`"
            );
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use tauri::AppHandle;
    pub fn show(_app: &AppHandle, _phase: &str) -> bool {
        false
    }
    pub fn set_text(_committed: &str, _tentative: &str) {}
    pub fn set_phase(_phase: &str) {}
    pub fn end() {}
}

pub use imp::{end, set_phase, set_text, show};
