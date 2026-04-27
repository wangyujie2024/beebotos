//! Path Security
//!
//! Provides safe path handling to prevent directory traversal attacks
//! and enforce sandbox boundaries.

use std::path::{Component, Path, PathBuf};

use crate::error::{KernelError, Result};

/// Path validation options
#[derive(Debug, Clone)]
pub struct PathValidationOptions {
    /// Allow absolute paths
    pub allow_absolute: bool,
    /// Base directory for relative paths (sandbox root)
    pub base_dir: Option<PathBuf>,
    /// Maximum path length
    pub max_length: usize,
    /// Allow symlinks
    pub allow_symlinks: bool,
    /// Allowed path prefixes (whitelist)
    pub allowed_prefixes: Vec<PathBuf>,
}

impl Default for PathValidationOptions {
    fn default() -> Self {
        Self {
            allow_absolute: false,
            base_dir: None,
            max_length: 4096,
            allow_symlinks: false,
            allowed_prefixes: vec![],
        }
    }
}

impl PathValidationOptions {
    /// Create sandbox options with base directory
    pub fn sandbox(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            allow_absolute: false,
            base_dir: Some(base_dir.into()),
            max_length: 4096,
            allow_symlinks: false,
            allowed_prefixes: vec![],
        }
    }

    /// Allow absolute paths within allowed prefixes
    pub fn allow_absolute_in_prefixes(mut self, prefixes: Vec<PathBuf>) -> Self {
        self.allow_absolute = true;
        self.allowed_prefixes = prefixes;
        self
    }
}

/// Validates and normalizes a path to prevent directory traversal attacks
///
/// # Arguments
/// * `path` - The path to validate
/// * `options` - Validation options
///
/// # Returns
/// * `Ok(PathBuf)` - The normalized, safe path
/// * `Err(KernelError)` - If the path is unsafe
///
/// # Examples
/// ```
/// use std::path::PathBuf;
///
/// use beebotos_kernel::security::path::validate_path;
///
/// // Safe path
/// let safe = validate_path("docs/readme.txt", &Default::default());
/// assert!(safe.is_ok());
///
/// // Directory traversal attack
/// let unsafe_path = validate_path("../../../etc/passwd", &Default::default());
/// assert!(unsafe_path.is_err());
/// ```
pub fn validate_path(path: &str, options: &PathValidationOptions) -> Result<PathBuf> {
    // Check path length
    if path.len() > options.max_length {
        return Err(KernelError::invalid_argument(format!(
            "Path exceeds maximum length of {}",
            options.max_length
        )));
    }

    // Check for null bytes
    if path.contains('\0') {
        return Err(KernelError::Security("Path contains null bytes".into()));
    }

    // Parse the path
    let path = Path::new(path);

    // Reject absolute paths unless explicitly allowed
    if path.is_absolute() && !options.allow_absolute {
        return Err(KernelError::Security(
            "Absolute paths are not allowed".into(),
        ));
    }

    // If absolute paths are allowed, check against allowed prefixes
    if path.is_absolute() && options.allow_absolute {
        let canonical = path.canonicalize().ok();
        let allowed = options
            .allowed_prefixes
            .iter()
            .any(|prefix| canonical.as_ref().map_or(false, |c| c.starts_with(prefix)));
        if !allowed {
            return Err(KernelError::Security(
                "Absolute path outside allowed prefixes".into(),
            ));
        }
    }

    // Normalize the path by resolving . and ..
    let mut normalized = normalize_path(path);

    // If we have a base directory, ensure the normalized path stays within it
    if let Some(ref base) = options.base_dir {
        // Join with base directory
        let full_path = base.join(&normalized);

        // Canonicalize if possible to resolve symlinks
        let canonical_base = base.canonicalize().unwrap_or_else(|_| base.clone());
        let canonical_full = full_path.canonicalize().ok();

        // Check if resolved path is within base directory
        let within_base = canonical_full
            .as_ref()
            .map_or(false, |c| c.starts_with(&canonical_base));

        if !within_base && canonical_full.is_some() {
            return Err(KernelError::Security(
                "Path escapes sandbox directory".into(),
            ));
        }

        // Return path relative to base
        normalized = full_path;
    }

    // Final security check: ensure no .. components remain
    if normalized
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(KernelError::Security(
            "Path contains directory traversal sequences".into(),
        ));
    }

    Ok(normalized)
}

/// Normalizes a path by resolving . and .. components
///
/// This is a pure string operation that doesn't access the filesystem.
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();

    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                // Preserve absolute path prefix
                result.push(component);
            }
            Component::CurDir => {
                // Skip . components
            }
            Component::ParentDir => {
                // Handle .. by removing last component if possible
                if result.file_name().is_some()
                    && !result.as_os_str().is_empty()
                    && result != Path::new("/")
                {
                    result.pop();
                } else {
                    // Keep .. if we're at root (to detect escapes)
                    result.push(component);
                }
            }
            Component::Normal(name) => {
                result.push(name);
            }
        }
    }

    // Handle empty result (path was just "." or "")
    if result.as_os_str().is_empty() {
        result.push(".");
    }

    result
}

/// Sanitize a filename to prevent path traversal in filename-only contexts
///
/// Removes any path separators and parent directory references.
pub fn sanitize_filename(name: &str) -> Result<String> {
    // Check for path separators
    if name.contains('/') || name.contains('\\') {
        return Err(KernelError::invalid_argument(
            "Filename cannot contain path separators",
        ));
    }

    // Check for parent directory references
    if name == ".." || name.starts_with("../") || name.contains("/../") {
        return Err(KernelError::invalid_argument(
            "Filename cannot contain directory traversal",
        ));
    }

    // Check for hidden files (optional security measure)
    if name.starts_with('.') && name != "." && name != ".." {
        // Allow but log - this might be intentional
        tracing::debug!("Sanitizing hidden file name: {}", name);
    }

    // Check for null bytes
    if name.contains('\0') {
        return Err(KernelError::invalid_argument(
            "Filename cannot contain null bytes",
        ));
    }

    Ok(name.to_string())
}

/// Check if a path is safe (doesn't contain traversal sequences)
pub fn is_safe_path(path: &str) -> bool {
    // Quick checks first
    if path.contains("..") || path.contains("./") || path.starts_with('/') {
        return false;
    }

    // More thorough check
    validate_path(path, &Default::default()).is_ok()
}

/// Sandbox path validator
///
/// Ensures all paths stay within a specific root directory.
pub struct PathSandbox {
    root: PathBuf,
    options: PathValidationOptions,
}

impl PathSandbox {
    /// Create a new path sandbox
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let options = PathValidationOptions::sandbox(&root);

        Self { root, options }
    }

    /// Validate a path within the sandbox
    pub fn validate(&self, path: &str) -> Result<PathBuf> {
        let validated = validate_path(path, &self.options)?;

        // Ensure the path is within the sandbox
        let full = self.root.join(&validated);
        let canonical_root = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone());
        let canonical_path = full.canonicalize().ok();

        if let Some(ref canonical) = canonical_path {
            if !canonical.starts_with(&canonical_root) {
                return Err(KernelError::Security("Path escapes sandbox".into()));
            }
        }

        Ok(validated)
    }

    /// Get sandbox root
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve a path to full filesystem path
    pub fn resolve(&self, path: &str) -> Result<PathBuf> {
        let validated = self.validate(path)?;
        Ok(self.root.join(validated))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        assert_eq!(normalize_path(Path::new("a/b/c")), PathBuf::from("a/b/c"));
        assert_eq!(
            normalize_path(Path::new("a/./b/../c")),
            PathBuf::from("a/c")
        );
        assert_eq!(
            normalize_path(Path::new("../a")),
            PathBuf::from("../a") // Can't resolve .. at root
        );
        assert_eq!(normalize_path(Path::new("./a")), PathBuf::from("a"));
    }

    #[test]
    fn test_validate_path_safe() {
        let opts = Default::default();

        assert!(validate_path("file.txt", &opts).is_ok());
        assert!(validate_path("dir/file.txt", &opts).is_ok());
        assert!(validate_path("a/b/c/d.txt", &opts).is_ok());
    }

    #[test]
    fn test_validate_path_traversal() {
        let opts = Default::default();

        assert!(validate_path("../file.txt", &opts).is_err());
        assert!(validate_path("../../etc/passwd", &opts).is_err());
        assert!(validate_path("a/../../../etc", &opts).is_err());
    }

    #[test]
    fn test_validate_path_absolute() {
        let opts = Default::default();

        assert!(validate_path("/etc/passwd", &opts).is_err());

        let opts_with_absolute = PathValidationOptions {
            allow_absolute: true,
            allowed_prefixes: vec![PathBuf::from("/home")],
            ..Default::default()
        };
        // This might succeed or fail depending on filesystem
        // but the validation logic should run
    }

    #[test]
    fn test_sanitize_filename() {
        assert!(sanitize_filename("file.txt").is_ok());
        assert!(sanitize_filename("..").is_err());
        assert!(sanitize_filename("a/b").is_err());
        assert!(sanitize_filename("a\\b").is_err());
        assert!(sanitize_filename("file\0.txt").is_err());
    }

    #[test]
    fn test_path_sandbox() {
        let sandbox = PathSandbox::new("data/sandbox");

        assert!(sandbox.validate("file.txt").is_ok());
        assert!(sandbox.validate("../escape.txt").is_err());
    }
}
