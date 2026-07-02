//! JIT compilation contract.
//!
//! Defines the trait for jitson template compilation.
//! n8n-rs implements CompiledStyleRegistry using this contract.
//! ndarray provides the jitson engine.

use crate::thinking::{ScanParams, ThinkingStyle};

/// JIT template — a compiled scan kernel configuration.
///
/// Produced by lance-graph (jitson_kernel.rs).
/// Compiled by ndarray (jitson/Cranelift).
/// Cached by n8n-rs (CompiledStyleRegistry).
#[derive(Debug, Clone)]
pub struct JitTemplate {
    /// Template JSON (JITSON format).
    pub json: String,
    /// τ address for cache key.
    pub tau_address: u8,
    /// Scan parameters baked as immediates.
    pub scan_params: ScanParams,
}

/// Compiled kernel handle — opaque pointer to native code.
///
/// Produced by ndarray jitson Cranelift compilation.
/// Stored in n8n-rs CompiledStyleRegistry kernel cache.
#[derive(Debug, Clone, Copy)]
pub struct KernelHandle {
    /// Function pointer to compiled scan kernel.
    /// Safety: only valid for the lifetime of the JIT engine.
    pub fn_ptr: *const u8,
    /// Parameter hash (for cache invalidation).
    pub param_hash: u64,
    /// Whether this kernel uses AVX-512.
    pub avx512: bool,
}

// SAFETY: KernelHandle is Send+Sync because the function pointer
// points to immutable compiled code in the JIT engine's code space.
unsafe impl Send for KernelHandle {}
unsafe impl Sync for KernelHandle {}

/// JIT compilation contract.
///
/// ndarray's jitson engine implements this.
/// n8n-rs calls it during workflow activation.
pub trait JitCompiler: Send + Sync {
    /// Compile a JITSON template into a native kernel.
    fn compile(&self, template: &JitTemplate) -> Result<KernelHandle, JitError>;

    /// Check if a kernel is cached for the given parameter hash.
    fn cached(&self, param_hash: u64) -> Option<KernelHandle>;

    /// Evict a kernel from the cache.
    fn evict(&self, param_hash: u64);
}

/// JIT compilation registry — caches compiled kernels by thinking style.
///
/// n8n-rs implements this. crewai-rust queries it to get compiled
/// kernels for agent thinking styles.
pub trait StyleRegistry: Send + Sync {
    /// Get or compile the kernel for a thinking style.
    fn get_kernel(&self, style: ThinkingStyle) -> Result<KernelHandle, JitError>;

    /// Compile all 36 styles at startup (warm cache).
    fn warm_cache(&self) -> Result<(), JitError>;

    /// Get the JITSON template for a style (without compiling).
    fn template_for(&self, style: ThinkingStyle) -> JitTemplate;
}

/// JIT compilation error.
#[derive(Debug, Clone)]
pub enum JitError {
    /// Template parsing failed.
    TemplateParse(String),
    /// Cranelift compilation failed.
    CompileFailed(String),
    /// Feature not available (e.g., AVX-512 not supported).
    FeatureUnavailable(String),
}

impl core::fmt::Display for JitError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TemplateParse(s) => write!(f, "JIT template parse: {s}"),
            Self::CompileFailed(s) => write!(f, "JIT compile failed: {s}"),
            Self::FeatureUnavailable(s) => write!(f, "JIT feature unavailable: {s}"),
        }
    }
}
