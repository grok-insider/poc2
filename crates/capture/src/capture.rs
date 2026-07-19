//! The capture sequence: inject Ctrl+C into the focused game window, poll
//! the Wayland clipboard for the item text PoE2 writes, restore the user's
//! clipboard, and hand the result back for broadcasting.
//!
//! Mechanism notes (from the Awakened PoE Trade / Exiled Exchange 2 study —
//! see ADR-0011): overlays do NOT OCR; they synthesize Ctrl+C and read the
//! clipboard, because the game itself copies the hovered item. On Wayland
//! the injection paths are:
//!
//! 1. `hyprctl dispatch sendshortcut "CTRL, C, activewindow"` — compositor-
//!    level, zero permissions, works wayland→XWayland (the Proton game path).
//! 2. `ydotool key …` — kernel uinput, indistinguishable from hardware;
//!    needs `ydotoold` (`programs.ydotool.enable = true` on NixOS). Used as
//!    the fallback when hyprctl fails.
//!
//! Clipboard I/O is `wl-paste` / `wl-copy` (wlr-data-control — works while
//! the game keeps focus). Timing mirrors APT: ~50 ms poll up to ~600 ms,
//! restore after ~120 ms.

use std::time::Duration;

use anyhow::{bail, Context, Result};
use tokio::process::Command;
use tracing::{debug, warn};

/// Linux input-event key codes (uinput): KEY_LEFTCTRL=29, KEY_LEFTALT=56,
/// KEY_C=46.
const YDOTOOL_CTRL_C: &[&str] = &["key", "29:1", "46:1", "46:0", "29:0"];
const YDOTOOL_CTRL_ALT_C: &[&str] = &["key", "29:1", "56:1", "46:1", "46:0", "56:0", "29:0"];

/// Does the clipboard text look like a PoE2 item copy?
/// (Locale-tolerant: matches the known first-line prefixes APT checks.)
pub fn looks_like_item_text(text: &str) -> bool {
    let head = text.trim_start();
    // English + the common localized variants of PoE2's first line.
    const PREFIXES: &[&str] = &[
        "Item Class:",
        "Objektklasse:",
        "Classe d'objet:",
        "Clase de objeto:",
        "Classe do Item:",
        "Класс предмета:",
        "아이템 종류:",
        "物品種類:",
        "物品类别:",
    ];
    PREFIXES.iter().any(|p| head.starts_with(p))
}

/// Compute the screenshot region around the cursor for OCR mode
/// (`grim -g "X,Y WxH"`). Clamps to non-negative origin.
pub fn ocr_region(cursor_x: i64, cursor_y: i64) -> (i64, i64, u32, u32) {
    const W: i64 = 560;
    const H: i64 = 360;
    let x = (cursor_x - W / 2).max(0);
    // Tooltips render above/right of the cursor more often than below.
    let y = (cursor_y - H + 40).max(0);
    (x, y, W as u32, H as u32)
}

async fn run(cmd: &str, args: &[&str]) -> Result<Vec<u8>> {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .await
        .with_context(|| format!("spawn `{cmd}`"))?;
    if !out.status.success() {
        bail!(
            "`{cmd} {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(out.stdout)
}

async fn wl_paste() -> Option<String> {
    match run("wl-paste", &["--no-newline", "--type", "text"]).await {
        Ok(bytes) => Some(String::from_utf8_lossy(&bytes).into_owned()),
        Err(e) => {
            debug!(error = %e, "wl-paste returned nothing (empty clipboard?)");
            None
        }
    }
}

async fn wl_copy(text: &str) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut child = Command::new("wl-copy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .context("spawn wl-copy")?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes()).await?;
    }
    child.wait().await?;
    Ok(())
}

/// Inject Ctrl+C (or Ctrl+Alt+C for advanced mod descriptions) into the
/// focused window. `hyprctl sendshortcut` first; `ydotool` fallback.
async fn inject_copy(advanced: bool) -> Result<&'static str> {
    let shortcut = if advanced {
        "CTRL ALT, C, activewindow"
    } else {
        "CTRL, C, activewindow"
    };
    match run("hyprctl", &["dispatch", "sendshortcut", shortcut]).await {
        Ok(out) => {
            // hyprctl reports dispatcher errors on stdout with exit code 0.
            let s = String::from_utf8_lossy(&out);
            if s.to_lowercase().contains("invalid") || s.to_lowercase().contains("error") {
                warn!(reply = %s.trim(), "hyprctl sendshortcut rejected; falling back to ydotool");
            } else {
                return Ok("hyprctl");
            }
        }
        Err(e) => warn!(error = %e, "hyprctl unavailable; falling back to ydotool"),
    }
    let keys = if advanced {
        YDOTOOL_CTRL_ALT_C
    } else {
        YDOTOOL_CTRL_C
    };
    run("ydotool", keys).await.context(
        "ydotool injection failed (is ydotoold running? programs.ydotool.enable on NixOS)",
    )?;
    Ok("ydotool")
}

/// Outcome of one capture attempt.
#[derive(Debug)]
pub enum Captured {
    ItemText(String),
    /// PNG bytes of the cursor-region screenshot (OCR mode).
    Image(Vec<u8>),
}

/// Clipboard capture: inject, poll, restore.
pub async fn capture_item_text(advanced: bool) -> Result<Captured> {
    let previous = wl_paste().await;

    let injector = inject_copy(advanced).await?;
    debug!(injector, advanced, "copy keystroke injected");

    // Poll for the game's clipboard write (APT: 48 ms × ~10).
    let mut captured: Option<String> = None;
    for _ in 0..12 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Some(text) = wl_paste().await {
            let changed = previous.as_deref() != Some(text.as_str());
            if changed && looks_like_item_text(&text) {
                captured = Some(text);
                break;
            }
            // The game may rewrite identical text; accept same-text item copies
            // when the previous clipboard already was an item.
            if !changed && looks_like_item_text(&text) {
                captured = Some(text);
                break;
            }
        }
    }

    let Some(text) = captured else {
        bail!("no item text appeared on the clipboard — is the game focused with an item hovered?");
    };

    // Restore the user's clipboard (APT waits 120 ms so the game is done).
    if let Some(prev) = previous {
        if prev != text {
            tokio::time::sleep(Duration::from_millis(120)).await;
            if let Err(e) = wl_copy(&prev).await {
                warn!(error = %e, "failed to restore previous clipboard");
            }
        }
    }

    Ok(Captured::ItemText(text))
}

/// OCR capture: screenshot the region around the cursor with `grim`.
pub async fn capture_cursor_region() -> Result<Captured> {
    // `hyprctl cursorpos` → "1234, 567"
    let pos = run("hyprctl", &["cursorpos"]).await?;
    let pos = String::from_utf8_lossy(&pos);
    let mut it = pos.trim().split(',').map(|s| s.trim().parse::<i64>());
    let (x, y) = match (it.next(), it.next()) {
        (Some(Ok(x)), Some(Ok(y))) => (x, y),
        _ => bail!("could not parse `hyprctl cursorpos` output: {pos:?}"),
    };
    let (rx, ry, rw, rh) = ocr_region(x, y);
    let geometry = format!("{rx},{ry} {rw}x{rh}");
    let png = run("grim", &["-g", &geometry, "-"]).await?;
    if png.len() < 1000 {
        bail!(
            "grim produced a suspiciously small screenshot ({} bytes)",
            png.len()
        );
    }
    Ok(Captured::Image(png))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_text_detection() {
        assert!(looks_like_item_text(
            "Item Class: Shields\nRarity: Normal\nEffigial Tower Shield"
        ));
        assert!(looks_like_item_text("\n  Item Class: Boots\n"));
        assert!(looks_like_item_text("Класс предмета: Кольца\n"));
        assert!(!looks_like_item_text("hello world"));
        assert!(!looks_like_item_text(""));
        assert!(!looks_like_item_text("Rarity: Rare\nno header"));
    }

    #[test]
    fn ocr_region_clamps_to_origin() {
        let (x, y, w, h) = ocr_region(10, 10);
        assert_eq!((x, y), (0, 0));
        assert_eq!((w, h), (560, 360));
        let (x2, y2, _, _) = ocr_region(1000, 800);
        assert_eq!(x2, 1000 - 280);
        assert_eq!(y2, 800 - 360 + 40);
    }
}
