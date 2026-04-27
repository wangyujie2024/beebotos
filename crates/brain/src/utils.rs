//! Utility Functions
//!
//! Common utility functions used throughout the brain module.

use std::cmp::Ordering;

/// Compare two f32 values, handling NaN gracefully.
///
/// NaN is treated as less than any other value, and equal to itself.
/// This provides a total ordering that's safe to use with sorting.
///
/// # Examples
/// ```
/// use std::cmp::Ordering;
///
/// use beebotos_brain::utils::compare_f32;
///
/// assert_eq!(compare_f32(&1.0, &2.0), Ordering::Less);
/// assert_eq!(compare_f32(&f32::NAN, &1.0), Ordering::Less);
/// assert_eq!(compare_f32(&1.0, &f32::NAN), Ordering::Greater);
/// assert_eq!(compare_f32(&f32::NAN, &f32::NAN), Ordering::Equal);
/// ```
pub fn compare_f32(a: &f32, b: &f32) -> Ordering {
    match (a.is_nan(), b.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
    }
}

/// Compare two f64 values, handling NaN gracefully.
///
/// NaN is treated as less than any other value, and equal to itself.
pub fn compare_f64(a: &f64, b: &f64) -> Ordering {
    match (a.is_nan(), b.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
    }
}

/// Get the maximum of two f32 values, handling NaN.
///
/// Returns the non-NaN value if one is NaN. Returns the first if both are NaN.
pub fn max_f32(a: f32, b: f32) -> f32 {
    match (a.is_nan(), b.is_nan()) {
        (true, _) => b,
        (_, true) => a,
        (false, false) => a.max(b),
    }
}

/// Get the minimum of two f32 values, handling NaN.
///
/// Returns the non-NaN value if one is NaN. Returns the first if both are NaN.
pub fn min_f32(a: f32, b: f32) -> f32 {
    match (a.is_nan(), b.is_nan()) {
        (true, _) => b,
        (_, true) => a,
        (false, false) => a.min(b),
    }
}

/// Get the maximum of two f64 values, handling NaN.
pub fn max_f64(a: f64, b: f64) -> f64 {
    match (a.is_nan(), b.is_nan()) {
        (true, _) => b,
        (_, true) => a,
        (false, false) => a.max(b),
    }
}

/// Get the minimum of two f64 values, handling NaN.
pub fn min_f64(a: f64, b: f64) -> f64 {
    match (a.is_nan(), b.is_nan()) {
        (true, _) => b,
        (_, true) => a,
        (false, false) => a.min(b),
    }
}

/// Check if an f32 is effectively zero (within epsilon).
pub fn is_effectively_zero_f32(value: f32, epsilon: f32) -> bool {
    value.abs() < epsilon
}

/// Check if an f64 is effectively zero (within epsilon).
pub fn is_effectively_zero_f64(value: f64, epsilon: f64) -> bool {
    value.abs() < epsilon
}

/// Clamp a value to the range [min, max], handling NaN by returning min.
pub fn clamp_f32(value: f32, min: f32, max: f32) -> f32 {
    if value.is_nan() {
        min
    } else {
        value.clamp(min, max)
    }
}

/// Clamp a value to the range [min, max], handling NaN by returning min.
pub fn clamp_f64(value: f64, min: f64, max: f64) -> f64 {
    if value.is_nan() {
        min
    } else {
        value.clamp(min, max)
    }
}

/// Get current timestamp in seconds since UNIX epoch.
pub fn current_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Get current timestamp in milliseconds since UNIX epoch.
pub fn current_timestamp_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Validate that a string input is within acceptable length limits.
///
/// Returns Err if the input is too long or empty (when required).
pub fn validate_input_length(
    input: &str,
    max_length: usize,
    allow_empty: bool,
) -> Result<(), String> {
    if !allow_empty && input.is_empty() {
        return Err("Input cannot be empty".to_string());
    }
    if input.len() > max_length {
        return Err(format!(
            "Input exceeds maximum length of {} characters",
            max_length
        ));
    }
    Ok(())
}

/// Validate that a priority value is within valid range [0.0, 1.0].
pub fn validate_priority(priority: f32) -> Result<f32, String> {
    if priority.is_nan() || priority < 0.0 || priority > 1.0 {
        Err(format!(
            "Priority must be in range [0.0, 1.0], got {}",
            priority
        ))
    } else {
        Ok(priority)
    }
}

/// Validate that an importance value is within valid range [0.0, 1.0].
pub fn validate_importance(importance: f32) -> Result<f32, String> {
    if importance.is_nan() || importance < 0.0 || importance > 1.0 {
        Err(format!(
            "Importance must be in range [0.0, 1.0], got {}",
            importance
        ))
    } else {
        Ok(importance)
    }
}

/// Truncate a string to a maximum length, adding ellipsis if truncated.
pub fn truncate_with_ellipsis(input: &str, max_len: usize) -> String {
    if input.len() <= max_len {
        input.to_string()
    } else {
        format!("{}...", &input[..max_len.saturating_sub(3)])
    }
}

// =============================================================================
// Random number generation utilities
// =============================================================================

use rand::Rng;

/// Generate a random f32 in range [0, 1)
pub fn random_f32() -> f32 {
    rand::thread_rng().gen::<f32>()
}

/// Generate a random f64 in range [0, 1)
pub fn random_f64() -> f64 {
    rand::thread_rng().gen::<f64>()
}

/// Generate a random f32 in range [min, max)
pub fn random_f32_range(min: f32, max: f32) -> f32 {
    rand::thread_rng().gen_range(min..max)
}

/// Generate a random f64 in range [min, max)
pub fn random_f64_range(min: f64, max: f64) -> f64 {
    rand::thread_rng().gen_range(min..max)
}

/// Generate a random bool with given probability of being true
pub fn random_bool(probability: f32) -> bool {
    rand::thread_rng().gen::<f32>() < probability
}

/// Generate a random usize in range [0, max)
pub fn random_usize(max: usize) -> usize {
    if max == 0 {
        0
    } else {
        rand::thread_rng().gen_range(0..max)
    }
}

/// Generate a random i32 in range [min, max)
pub fn random_i32_range(min: i32, max: i32) -> i32 {
    rand::thread_rng().gen_range(min..max)
}

/// Generate a random u64
pub fn random_u64() -> u64 {
    rand::thread_rng().gen::<u64>()
}

/// Shuffle a slice in place
pub fn shuffle<T>(slice: &mut [T]) {
    use rand::seq::SliceRandom;
    slice.shuffle(&mut rand::thread_rng());
}

/// Choose a random element from a slice
pub fn choose<T>(slice: &[T]) -> Option<&T> {
    if slice.is_empty() {
        None
    } else {
        let idx = random_usize(slice.len());
        Some(&slice[idx])
    }
}

/// Choose a random element from a slice with a mutable reference
pub fn choose_mut<T>(slice: &mut [T]) -> Option<&mut T> {
    if slice.is_empty() {
        None
    } else {
        let idx = random_usize(slice.len());
        Some(&mut slice[idx])
    }
}

// =============================================================================
// Seeded random number generation (for reproducibility)
// =============================================================================

use std::cell::RefCell;

use rand::rngs::StdRng;
use rand::SeedableRng;

thread_local! {
    static SEEDED_RNG: RefCell<Option<StdRng>> = RefCell::new(None);
}

/// Set a seed for reproducible random numbers (thread-local)
pub fn set_seed(seed: u64) {
    SEEDED_RNG.with(|rng| {
        *rng.borrow_mut() = Some(StdRng::seed_from_u64(seed));
    });
}

/// Clear the seed and return to non-deterministic randomness
pub fn clear_seed() {
    SEEDED_RNG.with(|rng| {
        *rng.borrow_mut() = None;
    });
}

/// Generate a random f32 using seeded RNG if available
pub fn random_f32_seeded() -> f32 {
    SEEDED_RNG.with(|rng| {
        if let Some(ref mut r) = *rng.borrow_mut() {
            r.gen::<f32>()
        } else {
            random_f32()
        }
    })
}

/// Generate a random bool using seeded RNG if available
pub fn random_bool_seeded(probability: f32) -> bool {
    random_f32_seeded() < probability
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare_f32() {
        assert_eq!(compare_f32(&1.0, &2.0), Ordering::Less);
        assert_eq!(compare_f32(&2.0, &1.0), Ordering::Greater);
        assert_eq!(compare_f32(&1.0, &1.0), Ordering::Equal);
        assert_eq!(compare_f32(&f32::NAN, &1.0), Ordering::Less);
        assert_eq!(compare_f32(&1.0, &f32::NAN), Ordering::Greater);
        assert_eq!(compare_f32(&f32::NAN, &f32::NAN), Ordering::Equal);
    }

    #[test]
    fn test_compare_f64() {
        assert_eq!(compare_f64(&1.0, &2.0), Ordering::Less);
        assert_eq!(compare_f64(&f64::NAN, &1.0), Ordering::Less);
        assert_eq!(compare_f64(&f64::NAN, &f64::NAN), Ordering::Equal);
    }

    #[test]
    fn test_max_min_f32() {
        assert_eq!(max_f32(1.0, 2.0), 2.0);
        assert_eq!(max_f32(2.0, 1.0), 2.0);
        assert_eq!(max_f32(f32::NAN, 1.0), 1.0);
        assert_eq!(max_f32(1.0, f32::NAN), 1.0);

        assert_eq!(min_f32(1.0, 2.0), 1.0);
        assert_eq!(min_f32(f32::NAN, 1.0), 1.0);
    }

    #[test]
    fn test_clamp_f32() {
        assert_eq!(clamp_f32(5.0, 0.0, 10.0), 5.0);
        assert_eq!(clamp_f32(-5.0, 0.0, 10.0), 0.0);
        assert_eq!(clamp_f32(15.0, 0.0, 10.0), 10.0);
        assert_eq!(clamp_f32(f32::NAN, 0.0, 10.0), 0.0);
    }

    #[test]
    fn test_validate_input_length() {
        assert!(validate_input_length("hello", 10, false).is_ok());
        assert!(validate_input_length("", 10, false).is_err());
        assert!(validate_input_length("", 10, true).is_ok());
        assert!(validate_input_length("hello world", 5, false).is_err());
    }

    #[test]
    fn test_validate_priority() {
        assert!(validate_priority(0.5).is_ok());
        assert!(validate_priority(0.0).is_ok());
        assert!(validate_priority(1.0).is_ok());
        assert!(validate_priority(-0.1).is_err());
        assert!(validate_priority(1.1).is_err());
        assert!(validate_priority(f32::NAN).is_err());
    }

    #[test]
    fn test_truncate_with_ellipsis() {
        assert_eq!(truncate_with_ellipsis("hello", 10), "hello");
        assert_eq!(truncate_with_ellipsis("hello world", 8), "hello...");
        assert_eq!(truncate_with_ellipsis("hello", 3), "...");
    }

    // =============================================================================
    // Random number generation tests
    // =============================================================================

    #[test]
    fn test_random_f32() {
        let r1 = random_f32();
        let r2 = random_f32();
        // Should be in range [0, 1)
        assert!(r1 >= 0.0 && r1 < 1.0);
        assert!(r2 >= 0.0 && r2 < 1.0);
        // Very unlikely to be equal (but possible)
        // We just verify it returns values
    }

    #[test]
    fn test_random_f64() {
        let r = random_f64();
        assert!(r >= 0.0 && r < 1.0);
    }

    #[test]
    fn test_random_f32_range() {
        let r = random_f32_range(5.0, 10.0);
        assert!(r >= 5.0 && r < 10.0);
    }

    #[test]
    fn test_random_bool() {
        let mut true_count = 0;
        for _ in 0..100 {
            if random_bool(0.5) {
                true_count += 1;
            }
        }
        // With 50% probability, should get roughly 50 true
        assert!(true_count >= 20 && true_count <= 80);
    }

    #[test]
    fn test_random_usize() {
        let r = random_usize(100);
        assert!(r < 100);

        // Zero max returns 0
        assert_eq!(random_usize(0), 0);
    }

    #[test]
    fn test_choose() {
        let slice = [1, 2, 3, 4, 5];
        let choice = choose(&slice);
        assert!(choice.is_some());
        assert!(slice.contains(choice.unwrap()));

        // Empty slice returns None
        let empty: &[i32] = &[];
        assert!(choose(empty).is_none());
    }

    #[test]
    fn test_seeded_random() {
        set_seed(12345);
        let r1 = random_f32_seeded();

        // Reset seed should give same value
        set_seed(12345);
        let r2 = random_f32_seeded();

        assert_eq!(r1, r2);

        // Clear seed
        clear_seed();
        let r3 = random_f32_seeded();
        // Should not panic and return a valid value
        assert!(r3 >= 0.0 && r3 < 1.0);
    }
}
