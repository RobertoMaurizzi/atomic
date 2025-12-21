//! Settings module - re-exports from atomic-core
//!
//! This module re-exports settings functionality from atomic-core with
//! String error types for Tauri command compatibility.

use rusqlite::Connection;
use std::collections::HashMap;

/// Get all settings as a HashMap (wraps atomic-core with String error)
pub fn get_all_settings(conn: &Connection) -> Result<HashMap<String, String>, String> {
    atomic_core::settings::get_all_settings(conn)
        .map_err(|e| e.to_string())
}

/// Set a setting (wraps atomic-core with String error)
pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    atomic_core::settings::set_setting(conn, key, value)
        .map_err(|e| e.to_string())
}
