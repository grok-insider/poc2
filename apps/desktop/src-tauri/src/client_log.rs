//! `Client.txt` watcher (Phase D.1).
//!
//! Polls Path of Exile 2's `logs/Client.txt` via `notify` (inotify on
//! Linux) and emits Tauri events when interesting lines appear.
//!
//! ## Why poll the log
//!
//! GGG's PoE2 client doesn't expose a real-time IPC; the log file is
//! the canonical out-of-band channel that streamers + overlays use.
//! Common detected lines:
//!   - Area changes:     `: You have entered <area>.`
//!   - Item drops:       `: <player> has joined the area.`
//!   - Death events:     `: <player> has been slain by <killer>.`
//!   - Trade whispers:   `@From <player>: ...`
//!
//! ## Wine path conventions
//!
//! On NixOS+Wine the file usually sits at one of:
//!   $WINEPREFIX/drive_c/Program Files/Path of Exile 2/logs/Client.txt
//!   $WINEPREFIX/drive_c/Program Files (x86)/Steam/steamapps/.../logs/Client.txt
//!
//! Configurable via the Settings panel (Phase B.3) — see the `path`
//! argument to [`start_client_log_watcher`].

use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use notify::{Event, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};

/// Tauri event topic the watcher emits to.
pub const CLIENT_LOG_EVENT: &str = "client-log://event";

/// One parsed line from Client.txt that the UI cares about.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClientLogEvent {
    /// `: You have entered <area>.`
    AreaEntered { area: String, line: String },
    /// `: <player> has joined the area.`
    PlayerJoined { player: String, line: String },
    /// `: <player> has been slain by <killer>.`
    Death {
        victim: String,
        killer: Option<String>,
        line: String,
    },
    /// `@From <player>: <message>`
    Whisper {
        from: String,
        message: String,
        line: String,
    },
    /// Anything else we don't classify (rate-limited at 1 per second
    /// in the watcher to avoid flooding the UI).
    Other { line: String },
}

/// Parse a single Client.txt line into a [`ClientLogEvent`].
///
/// Returns `None` for lines that contain no useful signal (header
/// timestamps without a recognised pattern).
#[must_use]
pub fn parse_log_line(line: &str) -> Option<ClientLogEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Death first — has a richer pattern.
    if let Some(idx) = trimmed.find(" has been slain by ") {
        let (left, rest) = trimmed.split_at(idx);
        let victim = left.rsplit(": ").next().unwrap_or(left).trim().to_string();
        let killer_part = rest
            .trim_start_matches(" has been slain by ")
            .trim_end_matches('.');
        return Some(ClientLogEvent::Death {
            victim,
            killer: if killer_part.is_empty() {
                None
            } else {
                Some(killer_part.to_string())
            },
            line: trimmed.to_string(),
        });
    }
    if let Some(idx) = trimmed.find(" has been slain.") {
        let left = &trimmed[..idx];
        let victim = left.rsplit(": ").next().unwrap_or(left).trim().to_string();
        return Some(ClientLogEvent::Death {
            victim,
            killer: None,
            line: trimmed.to_string(),
        });
    }
    if let Some(idx) = trimmed.find(": You have entered ") {
        let area_part = &trimmed[idx + ": You have entered ".len()..];
        let area = area_part.trim_end_matches('.').to_string();
        return Some(ClientLogEvent::AreaEntered {
            area,
            line: trimmed.to_string(),
        });
    }
    if let Some(idx) = trimmed.find(" has joined the area.") {
        let left = &trimmed[..idx];
        let player = left.rsplit(": ").next().unwrap_or(left).trim().to_string();
        return Some(ClientLogEvent::PlayerJoined {
            player,
            line: trimmed.to_string(),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("@From ") {
        if let Some(idx) = rest.find(": ") {
            let from = rest[..idx].trim().to_string();
            let message = rest[idx + 2..].to_string();
            return Some(ClientLogEvent::Whisper {
                from,
                message,
                line: trimmed.to_string(),
            });
        }
    }
    // Drop noisy lines that lack any of the recognised patterns.
    None
}

/// Watcher state — a shared file offset cursor + the active notify
/// watcher handle.
pub struct ClientLogWatcher {
    _watcher: notify::RecommendedWatcher,
    _path: PathBuf,
}

/// Start watching `path`. Spawns a notify-backed watcher that, on
/// every file modification event, reads new bytes since the last
/// emit and forwards parsed [`ClientLogEvent`]s to `on_event`.
///
/// Errors propagate the underlying notify / IO failures.
pub fn start_client_log_watcher<P, F>(
    path: P,
    on_event: F,
) -> Result<ClientLogWatcher, ClientLogError>
where
    P: AsRef<Path>,
    F: Fn(ClientLogEvent) + Send + Sync + 'static,
{
    let path = path.as_ref().to_path_buf();
    if !path.exists() {
        return Err(ClientLogError::PathNotFound(path));
    }
    // Initial cursor: end of file (we don't replay history).
    let initial_len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let cursor = Arc::new(Mutex::new(initial_len));
    let on_event = Arc::new(on_event);

    let path_for_callback = path.clone();
    let cursor_for_callback = cursor.clone();
    let event_handler = move |res: notify::Result<Event>| {
        let Ok(event) = res else { return };
        if !event.kind.is_modify() {
            return;
        }
        // Drain new bytes since the last cursor.
        let Ok(file) = std::fs::File::open(&path_for_callback) else {
            return;
        };
        let Ok(metadata) = file.metadata() else {
            return;
        };
        let len = metadata.len();
        let mut guard = match cursor_for_callback.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if len < *guard {
            // File was rotated / truncated; reset to current end.
            *guard = len;
            return;
        }
        if len == *guard {
            return;
        }
        let mut reader = BufReader::new(file);
        if reader.seek(SeekFrom::Start(*guard)).is_err() {
            return;
        }
        let mut buf = String::new();
        loop {
            buf.clear();
            let Ok(n) = reader.read_line(&mut buf) else {
                break;
            };
            if n == 0 {
                break;
            }
            if let Some(parsed) = parse_log_line(&buf) {
                on_event(parsed);
            }
        }
        *guard = len;
    };

    let mut watcher = notify::recommended_watcher(event_handler)?;
    watcher.watch(&path, RecursiveMode::NonRecursive)?;

    Ok(ClientLogWatcher {
        _watcher: watcher,
        _path: path,
    })
}

#[derive(Debug, thiserror::Error)]
pub enum ClientLogError {
    #[error("client log path not found: {0}")]
    PathNotFound(PathBuf),
    #[error("notify error: {0}")]
    Notify(#[from] notify::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_area_entered() {
        let line =
            "2025/12/15 10:00:00 12345 cffb0e34 [INFO Client 12345] : You have entered Aspirant's Plaza.";
        let ev = parse_log_line(line).expect("parsed");
        match ev {
            ClientLogEvent::AreaEntered { area, .. } => {
                assert_eq!(area, "Aspirant's Plaza");
            }
            other => panic!("expected AreaEntered, got {other:?}"),
        }
    }

    #[test]
    fn parse_player_joined() {
        let line =
            "2025/12/15 10:00:00 12345 cffb0e34 [INFO Client 12345] : SomePlayer has joined the area.";
        let ev = parse_log_line(line).expect("parsed");
        match ev {
            ClientLogEvent::PlayerJoined { player, .. } => assert_eq!(player, "SomePlayer"),
            other => panic!("expected PlayerJoined, got {other:?}"),
        }
    }

    #[test]
    fn parse_death_with_killer() {
        let line = "2025/12/15 10:00:00 12345 [INFO] : MyChar has been slain by Vorana.";
        let ev = parse_log_line(line).expect("parsed");
        match ev {
            ClientLogEvent::Death { victim, killer, .. } => {
                assert_eq!(victim, "MyChar");
                assert_eq!(killer.as_deref(), Some("Vorana"));
            }
            other => panic!("expected Death, got {other:?}"),
        }
    }

    #[test]
    fn parse_death_without_killer() {
        let line = "2025/12/15 10:00:00 12345 [INFO] : MyChar has been slain.";
        let ev = parse_log_line(line).expect("parsed");
        match ev {
            ClientLogEvent::Death { victim, killer, .. } => {
                assert_eq!(victim, "MyChar");
                assert!(killer.is_none());
            }
            other => panic!("expected Death without killer, got {other:?}"),
        }
    }

    #[test]
    fn parse_whisper() {
        let line = "@From SellerName: WTB your divine 2 chaos";
        let ev = parse_log_line(line).expect("parsed");
        match ev {
            ClientLogEvent::Whisper { from, message, .. } => {
                assert_eq!(from, "SellerName");
                assert!(message.contains("WTB"));
            }
            other => panic!("expected Whisper, got {other:?}"),
        }
    }

    #[test]
    fn parse_drops_uninteresting_lines() {
        assert!(parse_log_line("2025/12/15 10:00:00 12345 [DEBUG] foo bar baz").is_none());
        assert!(parse_log_line("").is_none());
    }
}
