/* src/lib.rs */

//!
//! Recursively traverse serde data structures and transform string values.
//!
//! ## Features
//!
//! - `std` (default): Standard library support
//! - `json`: JSON support via serde_json (no_std compatible)
//! - `yaml`: YAML support via serde_yaml (requires std)
//! - `toml`: TOML support via toml crate (requires std)
//! - `tracing`: Debug logging
//!
//! ## Example
//!
//! ```rust
//! use serde_resolve::{json, Config, Resolved};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let value = serde_json::json!({
//!     "greeting": "Hello {{name}}",
//!     "static": "no template here"
//! });
//!
//! let resolved = json::resolve(
//!     value,
//!     &|s: &str| {
//!         let s = s.to_string();
//!         async move {
//!             if s.contains("{{") {
//!                 // Transform strings containing templates
//!                 Ok::<_, std::convert::Infallible>(Resolved::Changed(
//!                     s.replace("{{name}}", "World")
//!                 ))
//!             } else {
//!                 // Skip strings without templates
//!                 Ok(Resolved::Unchanged)
//!             }
//!         }
//!     },
//!     &Config::default(),
//! ).await?;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(missing_docs)]

extern crate alloc;

use alloc::string::String;
use core::future::Future;

#[cfg(feature = "json")]
pub mod json;

#[cfg(feature = "yaml")]
pub mod yaml;

#[cfg(feature = "toml")]
pub mod toml;

// ============================================================================
// Resolved
// ============================================================================

/// Result of resolving a single string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
	/// The string was transformed to a new value.
	Changed(String),
	/// The string should remain unchanged (skip transformation).
	Unchanged,
}

impl Resolved {
	/// Create a `Changed` variant.
	#[inline]
	pub fn changed(s: impl Into<String>) -> Self {
		Self::Changed(s.into())
	}

	/// Create an `Unchanged` variant.
	#[inline]
	#[must_use]
	pub const fn unchanged() -> Self {
		Self::Unchanged
	}

	/// Returns `true` if this is `Changed`.
	#[inline]
	#[must_use]
	pub const fn is_changed(&self) -> bool {
		matches!(self, Self::Changed(_))
	}

	/// Returns `true` if this is `Unchanged`.
	#[inline]
	#[must_use]
	pub const fn is_unchanged(&self) -> bool {
		matches!(self, Self::Unchanged)
	}
}

// ============================================================================
// Config
// ============================================================================

/// Configuration for resolve operations.
#[derive(Debug, Clone)]
pub struct Config {
	/// Maximum nesting depth. Default: 32.
	///
	/// Prevents stack overflow on deeply nested or malicious input.
	pub max_depth: usize,

	/// Whether to resolve object keys. Default: false.
	///
	/// When `true`, object keys are also passed to the resolver.
	pub resolve_keys: bool,
}

impl Default for Config {
	fn default() -> Self {
		Self {
			max_depth: 32,
			resolve_keys: false,
		}
	}
}

impl Config {
	/// Create a new config with default values.
	#[inline]
	#[must_use]
	pub fn new() -> Self {
		Self::default()
	}

	/// Set maximum depth.
	#[inline]
	#[must_use]
	pub fn max_depth(mut self, depth: usize) -> Self {
		self.max_depth = depth;
		self
	}

	/// Set whether to resolve object keys.
	#[inline]
	#[must_use]
	pub fn resolve_keys(mut self, resolve: bool) -> Self {
		self.resolve_keys = resolve;
		self
	}
}

// ============================================================================
// Error
// ============================================================================

/// Error type for resolve operations.
#[derive(Debug)]
pub enum Error<E> {
	/// The resolver returned an error.
	Resolver(E),
	/// Depth limit exceeded.
	DepthExceeded {
		/// The configured limit that was exceeded.
		limit: usize,
	},
}

impl<E: core::fmt::Display> core::fmt::Display for Error<E> {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			Self::Resolver(e) => write!(f, "resolver error: {e}"),
			Self::DepthExceeded { limit } => write!(f, "depth limit ({limit}) exceeded"),
		}
	}
}

#[cfg(feature = "std")]
impl<E: std::error::Error + 'static> std::error::Error for Error<E> {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			Self::Resolver(e) => Some(e),
			Self::DepthExceeded { .. } => None,
		}
	}
}

impl<E> Error<E> {
	/// Create a resolver error.
	#[inline]
	pub fn resolver(e: E) -> Self {
		Self::Resolver(e)
	}

	/// Create a depth exceeded error.
	#[inline]
	#[must_use]
	pub fn depth_exceeded(limit: usize) -> Self {
		Self::DepthExceeded { limit }
	}
}
// ============================================================================
// Resolver Trait
// ============================================================================

/// Trait for async string resolvers.
///
/// Implementors decide:
/// - Which strings to transform (`Resolved::Changed`)
/// - Which strings to skip (`Resolved::Unchanged`)
/// - When to abort with an error (`Err`)
///
/// # Example
///
/// ```rust
/// use serde_resolve::{Resolved, Resolver};
///
/// struct MyResolver;
///
/// impl Resolver for MyResolver {
///     type Error = std::convert::Infallible;
///
///     async fn resolve(&self, input: &str) -> Result<Resolved, Self::Error> {
///         if input.starts_with("UPPER:") {
///             Ok(Resolved::changed(input[6..].to_uppercase()))
///         } else {
///             Ok(Resolved::unchanged())
///         }
///     }
/// }
/// ```
pub trait Resolver: Send + Sync {
	/// Error type returned by this resolver.
	type Error: Send;

	/// Resolve a string value.
	///
	/// # Returns
	///
	/// - `Ok(Resolved::Changed(new_value))` - Use the transformed value
	/// - `Ok(Resolved::Unchanged)` - Keep the original value
	/// - `Err(e)` - Abort the entire resolve operation
	fn resolve(&self, input: &str) -> impl Future<Output = Result<Resolved, Self::Error>> + Send;
}

// Blanket impl for Fn(&str) -> Future<Output = Result<Resolved, E>>
impl<F, Fut, E> Resolver for F
where
	F: Fn(&str) -> Fut + Send + Sync,
	Fut: Future<Output = Result<Resolved, E>> + Send,
	E: Send,
{
	type Error = E;

	#[inline]
	fn resolve(&self, input: &str) -> impl Future<Output = Result<Resolved, Self::Error>> + Send {
		self(input)
	}
}
