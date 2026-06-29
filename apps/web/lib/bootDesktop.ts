"use client";

/// Wires the optional desktop capture bridge into the store: item text pushed
/// from the shell flows through the normal import path and lands on the Item
/// pane. No-op in a plain browser.

import { getDesktopBridge } from "./desktop";
import { useCraft } from "./store";

let wired = false;

/** Idempotent. Returns a teardown (a no-op when no bridge is present). */
export function bootDesktop(): () => void {
  const bridge = getDesktopBridge();
  if (!bridge || wired) return () => {};
  wired = true;
  const off = bridge.onItemText((text) => {
    void useCraft.getState().ingestExternalItemText(text, "capture");
  });
  return () => {
    wired = false;
    off();
  };
}
