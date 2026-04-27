//! Architecture Support
//!
//! Platform-specific implementations.

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

#[cfg(target_arch = "aarch64")]
pub mod aarch64;

#[cfg(target_arch = "riscv64")]
pub mod riscv64;

/// Architecture trait
pub trait Architecture {
    /// Initialize architecture-specific features
    fn init() -> crate::error::Result<()>;
    /// Halt the CPU (infinite loop)
    fn halt() -> !;
    /// Enable hardware interrupts
    fn interrupts_enable();
    /// Disable hardware interrupts
    fn interrupts_disable();
}

#[cfg(target_arch = "aarch64")]
pub use aarch64::AArch64 as CurrentArch;
#[cfg(target_arch = "riscv64")]
pub use riscv64::Riscv64 as CurrentArch;
#[cfg(target_arch = "x86_64")]
pub use x86_64::X86_64 as CurrentArch;
