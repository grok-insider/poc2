//! # poc2-plugin-sdk
//!
//! Guest-side helpers + macros for authoring Wasm plugins.
//!
//! ## Authoring a custom-predicate plugin
//!
//! ```toml
//! # Cargo.toml
//! [package]
//! name = "my-plugin"
//! version = "0.1.0"
//! edition = "2021"
//!
//! [lib]
//! crate-type = ["cdylib"]
//!
//! [dependencies]
//! poc2-plugin-sdk = { path = "../../crates/plugin-sdk" }
//! serde_json = "1"
//! ```
//!
//! ```rust,ignore
//! use poc2_plugin_sdk::*;
//!
//! declare_predicate!(my_special_predicate, |item, args| {
//!     // item: serde_json::Value of an Item
//!     // args: serde_json::Value of the predicate's args object
//!     let ilvl = item["ilvl"].as_u64().unwrap_or(0);
//!     let threshold = args["min_ilvl"].as_u64().unwrap_or(82);
//!     ilvl >= threshold
//! });
//! ```
//!
//! Build with `cargo build --release --target wasm32-unknown-unknown`
//! and copy the resulting `.wasm` to
//! `~/.config/poc2/plugins/<plugin-id>/`.

// SDK is wasm-target intended; we emit `#[no_mangle]` exports + use
// raw pointer arithmetic for the host ABI. Both are necessary; the
// alternative (component model) lands in v1.x.
#![allow(unsafe_code)]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(clippy::no_mangle_with_rust_abi)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::doc_markdown)]

use std::cell::RefCell;
use std::os::raw::c_void;

thread_local! {
    /// Allocation arena. The host calls `alloc(len)` to grow this
    /// arena; subsequent host writes go into the returned offset.
    /// Plugins write outputs directly to the arena's tail and return
    /// `(ptr, len)` for the host to read.
    static ARENA: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Allocate `len` bytes in the plugin's arena, returning the pointer
/// the host should write to. Called by the host before invoking
/// any export that takes input bytes.
///
/// # Safety
///
/// The returned pointer is valid until the next `reset_arena` call
/// (typically once per export invocation).
#[no_mangle]
pub extern "C" fn alloc(len: i32) -> *mut c_void {
    if len <= 0 {
        return std::ptr::null_mut();
    }
    let len_usize = len as usize;
    ARENA.with(|a| {
        let mut a = a.borrow_mut();
        let offset = a.len();
        a.resize(offset + len_usize, 0);
        // SAFETY: `as_mut_ptr` is valid for the duration of the borrow.
        // The host writes directly into this region; no aliasing
        // concern within the single-threaded wasm sandbox.
        unsafe { a.as_mut_ptr().add(offset).cast::<c_void>() }
    })
}

/// Reset the arena (typically called between exports). Optional —
/// the host can also choose to keep the arena and accumulate.
#[no_mangle]
pub extern "C" fn reset_arena() {
    ARENA.with(|a| a.borrow_mut().clear());
}

/// Write `bytes` to the arena and return `(ptr, len)` as a packed i64
/// (`(len as i64) << 32 | ptr as i64`). Plugin export wrappers use
/// this to return JSON outputs back to the host.
///
/// `ptr` is an ABSOLUTE linear-memory address (what [`alloc`] returns
/// and what hosts read via the module's exported memory), so the copy
/// goes through a raw pointer — NOT a `Vec` index. The original v1
/// implementation indexed the arena `Vec` with the absolute address,
/// which panicked (→ `unreachable` trap) on every real emission call;
/// it went unnoticed because the host tests exercised hand-written WAT
/// fixtures, never an SDK-built plugin.
#[must_use]
pub fn write_output(bytes: &[u8]) -> (i32, i32) {
    let len = bytes.len() as i32;
    let ptr = alloc(len) as i32;
    if ptr == 0 {
        return (0, 0);
    }
    // SAFETY: `alloc` just reserved exactly `len` bytes at this address
    // inside the arena, and nothing reallocates the arena between the
    // reservation and this copy (single-threaded guest).
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as usize as *mut u8, bytes.len());
    }
    (ptr, len)
}

/// Read `len` bytes of linear memory starting at absolute address
/// `ptr`. Used by export wrappers to deserialize host inputs (the host
/// writes them at addresses previously returned by [`alloc`]).
#[must_use]
pub fn read_input(ptr: i32, len: i32) -> Vec<u8> {
    if ptr <= 0 || len <= 0 {
        return Vec::new();
    }
    // SAFETY: the host contract is that `(ptr, len)` came from `alloc`
    // reservations it wrote into; reading them as a shared slice while
    // no `&mut` borrow of the arena is live is sound in the
    // single-threaded guest.
    unsafe { std::slice::from_raw_parts(ptr as usize as *const u8, len as usize).to_vec() }
}

/// Re-export serde_json so plugins don't need to depend on it
/// directly.
pub use serde_json;

/// Declare a custom predicate plugin. Generates the standard ABI
/// surface (`alloc`, `eval_predicate`) so the plugin author writes
/// only the closure body.
#[macro_export]
macro_rules! declare_predicate {
    ($name:ident, $body:expr) => {
        const _: fn(&$crate::serde_json::Value, &$crate::serde_json::Value) -> bool = $body;

        #[no_mangle]
        pub extern "C" fn eval_predicate(
            name_ptr: i32,
            name_len: i32,
            item_ptr: i32,
            item_len: i32,
            args_ptr: i32,
            args_len: i32,
        ) -> i32 {
            let name_bytes = $crate::read_input(name_ptr, name_len);
            let name = std::str::from_utf8(&name_bytes).unwrap_or("");
            if name != stringify!($name) {
                // The plugin only knows one predicate name; refuse
                // others gracefully.
                return 0;
            }
            let item_bytes = $crate::read_input(item_ptr, item_len);
            let args_bytes = $crate::read_input(args_ptr, args_len);
            let item: $crate::serde_json::Value = $crate::serde_json::from_slice(&item_bytes)
                .unwrap_or($crate::serde_json::Value::Null);
            let args: $crate::serde_json::Value = $crate::serde_json::from_slice(&args_bytes)
                .unwrap_or($crate::serde_json::Value::Null);
            let result: bool = ($body)(&item, &args);
            i32::from(result)
        }
    };
}

/// Declare a strategy emitter. Generates `list_strategies()` returning
/// a JSON-encoded `Vec<String>` of strategy TOMLs.
#[macro_export]
macro_rules! declare_strategies {
    ($($toml:expr),* $(,)?) => {
        #[no_mangle]
        pub extern "C" fn list_strategies() -> i64 {
            let strategies: Vec<String> = vec![ $( $toml.to_string() ),* ];
            let json = $crate::serde_json::to_vec(&strategies).unwrap_or_default();
            let (ptr, len) = $crate::write_output(&json);
            ((len as i64) << 32) | (ptr as i64 & 0xffff_ffff)
        }
    };
}

/// Declare a rule emitter. Same shape as [`declare_strategies`].
#[macro_export]
macro_rules! declare_rules {
    ($($toml:expr),* $(,)?) => {
        #[no_mangle]
        pub extern "C" fn list_rules() -> i64 {
            let rules: Vec<String> = vec![ $( $toml.to_string() ),* ];
            let json = $crate::serde_json::to_vec(&rules).unwrap_or_default();
            let (ptr, len) = $crate::write_output(&json);
            ((len as i64) << 32) | (ptr as i64 & 0xffff_ffff)
        }
    };
}

/// Declare a recommendation emitter. The closure takes the host-
/// provided state JSON and returns a `Vec<PluginCandidate>` JSON.
#[macro_export]
macro_rules! declare_recommendation_emitter {
    ($body:expr) => {
        const _: fn(&$crate::serde_json::Value) -> $crate::serde_json::Value = $body;

        #[no_mangle]
        pub extern "C" fn emit_recommendations(state_ptr: i32, state_len: i32) -> i64 {
            let state_bytes = $crate::read_input(state_ptr, state_len);
            let state: $crate::serde_json::Value = $crate::serde_json::from_slice(&state_bytes)
                .unwrap_or($crate::serde_json::Value::Null);
            let candidates: $crate::serde_json::Value = ($body)(&state);
            let json = $crate::serde_json::to_vec(&candidates).unwrap_or_default();
            let (ptr, len) = $crate::write_output(&json);
            ((len as i64) << 32) | (ptr as i64 & 0xffff_ffff)
        }
    };
}
