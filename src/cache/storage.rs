//! On-disk cache entry storage and total deserialisation.
//!
//! # Specification (docs/cache-design.md §6, §7)
//!
//! Content-addressed by `region_key` (32 bytes).
//! Format: Binary bincode.
//! Total deserialisation discipline: Under `panic = "abort"`, corrupted entries
//! must NEVER panic. Every read returns `Result` and errors gracefully fall
//! through to a cache miss.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::McResult;

/// Current format version. Invalidate on change; never silently reinterpret.
pub const FORMAT_VERSION: u32 = 1;

/// The pinned upstream commit from `docs/UPSTREAM.md`.
pub const PINNED_UPSTREAM_COMMIT: &str = "7149d568785b039134f0b2baa58358c8af63e70d";

/// One serialized cache entry corresponding to a single design region.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CacheEntry {
    pub format_version: u32,
    pub upstream_commit: String,
    pub identity_scheme: String,
    pub region_key: [u8; 32],
    pub whole_digest: [u8; 32],
    pub verdict: McResult,
    /// Side table: atom identity string -> local variable ID
    pub side_table: Vec<(String, u32)>,
    /// Clauses stored as signed variable literals (e.g. +1, -2)
    pub clauses: Vec<Vec<i32>>,
    pub collision: bool,
}

/// Default directory for cache storage.
pub fn default_cache_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("RIC3_CACHE_DIR") {
        PathBuf::from(dir)
    } else {
        PathBuf::from(".ric3cache")
    }
}

/// Path to the cache entry for a given `region_key`.
pub fn entry_path(cache_dir: &Path, region_key: &[u8; 32]) -> PathBuf {
    let mut hex = String::with_capacity(64);
    for b in region_key {
        use std::fmt::Write;
        let _ = write!(hex, "{:02x}", b);
    }
    cache_dir.join(format!("{}.bin", hex))
}

/// Store a cache entry to disk. Atomic write via tempfile to prevent partial reads.
pub fn store_entry(cache_dir: &Path, entry: &CacheEntry) -> Result<(), String> {
    if let Err(e) = fs::create_dir_all(cache_dir) {
        return Err(format!("failed to create cache dir: {e}"));
    }

    let encoded = bincode::serialize(entry).map_err(|e| format!("bincode serialization error: {e}"))?;

    let final_path = entry_path(cache_dir, &entry.region_key);
    let tmp_path = cache_dir.join(format!(".tmp_{}.bin", std::process::id()));

    if let Err(e) = fs::write(&tmp_path, encoded) {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!("failed to write temp cache entry: {e}"));
    }

    if let Err(e) = fs::rename(&tmp_path, &final_path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!("failed to commit cache entry: {e}"));
    }

    Ok(())
}

/// Load a cache entry from disk, strictly enforcing total deserialisation.
///
/// Returns `None` on any corruption, version mismatch, or commit divergence,
/// falling through to a full run without panicking (AGENTS.md §1.5).
pub fn load_entry(cache_dir: &Path, region_key: &[u8; 32]) -> Option<CacheEntry> {
    let path = entry_path(cache_dir, region_key);
    if !path.is_file() {
        return None;
    }

    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            log::warn!("Cache entry read error on {}: {e}", path.display());
            return None;
        }
    };

    // Bincode deserialisation is total — it returns Err rather than panicking on malformed bytes.
    let entry: CacheEntry = match bincode::deserialize(&bytes) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("Cache entry corrupt in {}: {e}", path.display());
            return None;
        }
    };

    // Format and commit integrity checks
    if entry.format_version != FORMAT_VERSION {
        log::info!(
            "Cache entry version mismatch (stored: {}, current: {}), ignoring.",
            entry.format_version,
            FORMAT_VERSION
        );
        return None;
    }

    if entry.upstream_commit != PINNED_UPSTREAM_COMMIT {
        log::info!("Cache entry upstream commit mismatch, ignoring.");
        return None;
    }

    if entry.region_key != *region_key {
        log::warn!("Cache entry region_key mismatch, ignoring.");
        return None;
    }

    if entry.collision {
        log::warn!("Cache entry marked with collision flag, ignoring.");
        return None;
    }

    Some(entry)
}
