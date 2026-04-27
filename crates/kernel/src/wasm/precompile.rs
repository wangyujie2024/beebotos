//! WASM Precompilation
//!
//! Precompiles WASM modules to native code for faster startup.
//! Supports caching and parallel compilation.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::RwLock;
use tracing::{debug, info, warn};

use crate::error::{KernelError, Result};
use crate::wasm::engine::WasmEngine;

/// Precompiled module cache
pub struct PrecompileCache {
    /// Cache directory
    cache_dir: PathBuf,
    /// In-memory cache of serialized modules
    memory_cache: RwLock<HashMap<String, Vec<u8>>>,
    /// Maximum cache size in bytes
    max_size: usize,
    /// Current cache size
    current_size: RwLock<usize>,
}

impl PrecompileCache {
    /// Create new precompile cache
    pub fn new(cache_dir: impl AsRef<Path>, max_size: usize) -> Result<Self> {
        let cache_dir = cache_dir.as_ref().to_path_buf();

        // Create cache directory if needed
        if !cache_dir.exists() {
            std::fs::create_dir_all(&cache_dir)
                .map_err(|e| KernelError::io(format!("Failed to create cache dir: {}", e)))?;
        }

        Ok(Self {
            cache_dir,
            memory_cache: RwLock::new(HashMap::new()),
            max_size,
            current_size: RwLock::new(0),
        })
    }

    /// Create cache in default location
    pub fn default_cache() -> Result<Self> {
        let cache_dir = dirs::cache_dir()
            .map(|d| d.join("beebotos/wasm"))
            .unwrap_or_else(|| PathBuf::from("data/wasm_cache"));

        Self::new(cache_dir, 1024 * 1024 * 1024) // 1GB default
    }

    /// Get cache key for WASM module
    fn cache_key(wasm_bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(wasm_bytes);
        format!("{:x}", hasher.finalize())
    }

    /// Check if precompiled module exists in cache
    pub fn contains(&self, wasm_bytes: &[u8]) -> bool {
        let key = Self::cache_key(wasm_bytes);

        // Check memory cache
        if self.memory_cache.read().contains_key(&key) {
            return true;
        }

        // Check disk cache
        let path = self.cache_dir.join(format!("{}.cwasm", key));
        path.exists()
    }

    /// Get precompiled module from cache
    pub fn get(&self, wasm_bytes: &[u8]) -> Result<Option<Vec<u8>>> {
        let key = Self::cache_key(wasm_bytes);

        // Check memory cache first
        {
            let cache = self.memory_cache.read();
            if let Some(data) = cache.get(&key) {
                debug!("Precompile cache hit (memory): {}", &key[..16]);
                return Ok(Some(data.clone()));
            }
        }

        // Check disk cache
        let path = self.cache_dir.join(format!("{}.cwasm", key));
        if path.exists() {
            match std::fs::read(&path) {
                Ok(data) => {
                    debug!("Precompile cache hit (disk): {}", &key[..16]);

                    // Add to memory cache
                    self.add_to_memory_cache(key, data.clone())?;

                    return Ok(Some(data));
                }
                Err(e) => {
                    warn!("Failed to read cache file: {}", e);
                    // Remove corrupted cache file
                    let _ = std::fs::remove_file(&path);
                }
            }
        }

        Ok(None)
    }

    /// Store precompiled module in cache
    pub fn put(&self, wasm_bytes: &[u8], precompiled: &[u8]) -> Result<()> {
        let key = Self::cache_key(wasm_bytes);

        // Write to disk
        let path = self.cache_dir.join(format!("{}.cwasm", key));
        std::fs::write(&path, precompiled)
            .map_err(|e| KernelError::io(format!("Failed to write cache: {}", e)))?;

        debug!(
            "Stored precompiled module: {} ({} bytes)",
            &key[..16],
            precompiled.len()
        );

        // Add to memory cache
        self.add_to_memory_cache(key, precompiled.to_vec())?;

        Ok(())
    }

    /// Add to memory cache with size management
    ///
    /// Lock acquisition order: memory_cache, then current_size
    fn add_to_memory_cache(&self, key: String, data: Vec<u8>) -> Result<()> {
        let size = data.len();

        // First, acquire memory_cache lock and evict if needed
        // Then update current_size separately to avoid nested locks
        let mut cache = self.memory_cache.write();

        // Evict entries if needed while holding only memory_cache lock
        while self.current_size() + size > self.max_size {
            if let Some(first_key) = cache.keys().next().cloned() {
                if let Some(evicted_data) = cache.remove(&first_key) {
                    // Update current_size separately (no nested lock)
                    let mut current = self.current_size.write();
                    *current = current.saturating_sub(evicted_data.len());
                } else {
                    break; // Nothing to evict
                }
            } else {
                break; // Cache is empty
            }
        }

        // Insert new data
        cache.insert(key.clone(), data);
        drop(cache); // Release memory_cache lock

        // Update size after releasing memory_cache lock
        let mut current = self.current_size.write();
        *current += size;
        drop(current);

        debug!(
            "Added to memory cache: {} ({} bytes)",
            &key[..key.len().min(16)],
            size
        );
        Ok(())
    }

    /// Get current cache size (helper to avoid lock issues)
    fn current_size(&self) -> usize {
        *self.current_size.read()
    }

    /// Clear all caches
    pub fn clear(&self) -> Result<()> {
        // Clear memory cache
        {
            let mut cache = self.memory_cache.write();
            cache.clear();
            *self.current_size.write() = 0;
        }

        // Clear disk cache
        if self.cache_dir.exists() {
            for entry in std::fs::read_dir(&self.cache_dir)
                .map_err(|e| KernelError::io(format!("Failed to read cache dir: {}", e)))?
            {
                if let Ok(entry) = entry {
                    if entry
                        .path()
                        .extension()
                        .map(|e| e == "cwasm")
                        .unwrap_or(false)
                    {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }

        info!("Precompile cache cleared");
        Ok(())
    }

    /// Get cache statistics
    pub fn stats(&self) -> PrecompileStats {
        let memory_entries = self.memory_cache.read().len();
        let memory_size = *self.current_size.read();

        // Count disk entries
        let disk_entries = if self.cache_dir.exists() {
            std::fs::read_dir(&self.cache_dir)
                .map(|entries| entries.filter(|e| e.is_ok()).count())
                .unwrap_or(0)
        } else {
            0
        };

        PrecompileStats {
            memory_entries,
            memory_size,
            disk_entries,
            max_size: self.max_size,
        }
    }
}

/// Precompile statistics
#[derive(Debug, Clone, Copy)]
pub struct PrecompileStats {
    /// Memory cache entries
    pub memory_entries: usize,
    /// Memory cache size
    pub memory_size: usize,
    /// Disk cache entries
    pub disk_entries: usize,
    /// Maximum cache size
    pub max_size: usize,
}

impl PrecompileStats {
    /// Format as human-readable string
    pub fn format(&self) -> String {
        format!(
            "Precompile Cache: {} entries in memory ({} MB), {} entries on disk, max {} MB",
            self.memory_entries,
            self.memory_size / (1024 * 1024),
            self.disk_entries,
            self.max_size / (1024 * 1024)
        )
    }

    /// Get memory utilization
    pub fn memory_utilization(&self) -> f64 {
        if self.max_size == 0 {
            return 0.0;
        }
        self.memory_size as f64 / self.max_size as f64
    }
}

/// Precompile manager for batch operations
pub struct PrecompileManager {
    /// WASM engine
    engine: Arc<WasmEngine>,
    /// Precompile cache
    cache: Arc<PrecompileCache>,
}

impl PrecompileManager {
    /// Create new precompile manager
    pub fn new(engine: Arc<WasmEngine>, cache: Arc<PrecompileCache>) -> Self {
        Self { engine, cache }
    }

    /// Precompile a single module
    pub fn precompile(&self, name: &str, wasm_bytes: &[u8]) -> Result<Vec<u8>> {
        // Check cache first
        if let Some(cached) = self.cache.get(wasm_bytes)? {
            debug!("Using cached precompiled module: {}", name);
            return Ok(cached);
        }

        // Precompile
        let start = std::time::Instant::now();
        let precompiled = self.engine.precompile(wasm_bytes)?;

        info!(
            "Precompiled '{}': {} bytes -> {} bytes in {:?}",
            name,
            wasm_bytes.len(),
            precompiled.len(),
            start.elapsed()
        );

        // Cache it
        self.cache.put(wasm_bytes, &precompiled)?;

        Ok(precompiled)
    }

    /// Precompile multiple modules in parallel
    pub fn precompile_batch(&self, modules: &[(&str, &[u8])]) -> Vec<Result<Vec<u8>>> {
        use rayon::prelude::*;

        modules
            .par_iter()
            .map(|(name, wasm)| self.precompile(name, wasm))
            .collect()
    }

    /// Load a precompiled module
    pub fn load_precompiled(&self, name: &str, wasm_bytes: &[u8]) -> Result<wasmtime::Module> {
        // Try cache first
        let cached = self.cache.get(wasm_bytes)?;
        if let Some(serialized) = cached {
            return self.engine.load_precompiled(name, &serialized);
        }

        // Precompile and load
        let serialized = self.precompile(name, wasm_bytes)?;
        self.engine.load_precompiled(name, &serialized)
    }

    /// Get cache reference
    pub fn cache(&self) -> &PrecompileCache {
        &self.cache
    }
}

/// Precompile utility functions

/// Precompile WASM bytes to a file
pub fn precompile_to_file(
    engine: &WasmEngine,
    wasm_bytes: &[u8],
    output_path: &Path,
) -> Result<()> {
    let precompiled = engine.precompile(wasm_bytes)?;

    std::fs::write(output_path, precompiled)
        .map_err(|e| KernelError::io(format!("Failed to write precompiled module: {}", e)))?;

    info!("Precompiled module written to: {:?}", output_path);
    Ok(())
}

/// Load precompiled module from file
pub fn load_precompiled_from_file(
    engine: &WasmEngine,
    name: &str,
    path: &Path,
) -> Result<wasmtime::Module> {
    let serialized = std::fs::read(path)
        .map_err(|e| KernelError::io(format!("Failed to read precompiled module: {}", e)))?;

    engine.load_precompiled(name, &serialized)
}

/// Validate WASM module without compiling
pub fn validate_wasm(wasm_bytes: &[u8]) -> Result<()> {
    // Basic magic number and version check
    if wasm_bytes.len() < 8 {
        return Err(KernelError::invalid_argument("WASM module too short"));
    }

    if &wasm_bytes[0..4] != b"\0asm" {
        return Err(KernelError::invalid_argument("Invalid WASM magic number"));
    }

    let version = u32::from_le_bytes([wasm_bytes[4], wasm_bytes[5], wasm_bytes[6], wasm_bytes[7]]);
    if version != 1 {
        return Err(KernelError::invalid_argument(format!(
            "Unsupported WASM version: {}",
            version
        )));
    }

    // Full validation would use wasmparser
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_cache_key() {
        let bytes1 = b"test wasm module";
        let bytes2 = b"test wasm module";
        let bytes3 = b"different module";

        assert_eq!(
            PrecompileCache::cache_key(bytes1),
            PrecompileCache::cache_key(bytes2)
        );
        assert_ne!(
            PrecompileCache::cache_key(bytes1),
            PrecompileCache::cache_key(bytes3)
        );
    }

    #[test]
    fn test_cache_operations() {
        let temp_dir = TempDir::new().unwrap();
        let cache = PrecompileCache::new(temp_dir.path(), 1024 * 1024).unwrap();

        let wasm = b"\0asm\x01\x00\x00\x00"; // Minimal WASM header
        let compiled = b"compiled data";

        // Put in cache
        cache.put(wasm, compiled).unwrap();
        assert!(cache.contains(wasm));

        // Get from cache
        let retrieved = cache.get(wasm).unwrap();
        assert_eq!(retrieved, Some(compiled.to_vec()));

        // Stats
        let stats = cache.stats();
        assert_eq!(stats.memory_entries, 1);
    }

    #[test]
    fn test_validate_wasm() {
        // Valid WASM header
        let valid = b"\0asm\x01\x00\x00\x00";
        assert!(validate_wasm(valid).is_ok());

        // Too short
        let short = b"\0asm";
        assert!(validate_wasm(short).is_err());

        // Wrong magic
        let wrong_magic = b"WASM\x01\x00\x00\x00";
        assert!(validate_wasm(wrong_magic).is_err());
    }
}
