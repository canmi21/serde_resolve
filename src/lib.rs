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
//!                 Ok::<_, std::convert::Infallible>(Resolved::Changed(
//!                     s.replace("{{name}}", "World")
//!                 ))
//!             } else {
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

/// A segment in a value path, used for tracing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSegment {
	/// Object/Map key
	Key(String),
	/// Array index
	Index(usize),
}

#[cfg(any(feature = "json", feature = "yaml", feature = "toml"))]
macro_rules! impl_resolve_recursive {
    (
        $value_type:ty,
        $variant_string:path,
        $variant_array:path,
        $variant_object:path,
        $map_constructor:expr,
        $key_to_string:expr,
        $resolver:ident, $config:ident, $depth:ident, $path:ident, $key:ident,
        $resolve_key_logic:block,
        { $($other_arms:tt)* }
    ) => {
        fn resolve_recursive<'a, R>(
            value: $value_type,
            $resolver: &'a R,
            $config: &'a Config,
            $depth: usize,
            #[cfg(feature = "tracing")] $path: &'a mut alloc::vec::Vec<crate::PathSegment>,
        ) -> core::pin::Pin<alloc::boxed::Box<dyn core::future::Future<Output = Result<$value_type, crate::Error<R::Error>>> + Send + 'a>>
        where
            R: crate::Resolver,
        {
            alloc::boxed::Box::pin(async move {
                if $depth >= $config.max_depth {
                    return Err(crate::Error::depth_exceeded($config.max_depth));
                }

                #[cfg(feature = "tracing")]
                tracing::trace!(depth = $depth, path = ?$path, value_type = ?value_type_name(&value), "resolving");

                match value {
                    $variant_string(s) => {
                        match $resolver.resolve(&s).await.map_err(crate::Error::resolver)? {
                            crate::Resolved::Changed(new_s) => {
                                #[cfg(feature = "tracing")]
                                tracing::trace!(original = %s, resolved = %new_s, "string changed");
                                Ok($variant_string(new_s))
                            }
                            crate::Resolved::Unchanged => {
                                #[cfg(feature = "tracing")]
                                tracing::trace!(value = %s, "string unchanged");
                                Ok($variant_string(s))
                            }
                        }
                    }

                    $variant_array(arr) => {
                        let mut result = alloc::vec::Vec::with_capacity(arr.len());
                        for (_i, item) in arr.into_iter().enumerate() {
                            #[cfg(feature = "tracing")]
                            $path.push(crate::PathSegment::Index(_i));

                            let res = resolve_recursive(
                                item,
                                $resolver,
                                $config,
                                $depth + 1,
                                #[cfg(feature = "tracing")] $path
                            ).await?;
                            result.push(res);

                            #[cfg(feature = "tracing")]
                            $path.pop();
                        }
                        Ok($variant_array(result))
                    }

                    $variant_object(map) => {
                        let mut result = $map_constructor(map.len());
                        for ($key, val) in map {
                            // Helper to get key string for tracing
                            #[cfg(feature = "tracing")]
                            let key_str = ($key_to_string)(&$key);

                            // Optionally resolve keys
                            let resolved_key = if $config.resolve_keys {
                                $resolve_key_logic
                            } else {
                                $key
                            };

                            #[cfg(feature = "tracing")]
                            $path.push(crate::PathSegment::Key(key_str));

                            let resolved_val = resolve_recursive(
                                val,
                                $resolver,
                                $config,
                                $depth + 1,
                                #[cfg(feature = "tracing")] $path
                            ).await?;
                            result.insert(resolved_key, resolved_val);

                            #[cfg(feature = "tracing")]
                            $path.pop();
                        }
                        Ok($variant_object(result))
                    }

                    $($other_arms)*
                }
            })
        }
    }
}

#[cfg(feature = "json")]
pub mod json;

#[cfg(feature = "yaml")]
pub mod yaml;

#[cfg(feature = "toml")]
pub mod toml;

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

impl From<String> for Resolved {
	#[inline]
	fn from(s: String) -> Self {
		Self::Changed(s)
	}
}

impl<'a> From<&'a str> for Resolved {
	#[inline]
	fn from(s: &'a str) -> Self {
		Self::Changed(s.into())
	}
}

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

	/// Disable depth limiting.
	///
	/// # Warning
	///
	/// This may cause stack overflow on deeply nested or malicious input.
	/// Prefer using [`Config::max_depth`] with a reasonable limit in most cases.
	#[inline]
	#[must_use]
	pub fn unlimited_depth(mut self) -> Self {
		self.max_depth = usize::MAX;
		self
	}
}

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

/// Trait for async string resolvers.
///
/// Implementors decide:
/// - Which strings to transform ([`Resolved::Changed`])
/// - Which strings to skip ([`Resolved::Unchanged`])
/// - When to abort with an error ([`Err`])
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

/// Error type for generic struct resolution.
///
/// This error type wraps errors that can occur during the serialize-resolve-deserialize
/// round-trip when using [`resolve_struct`].
#[cfg(feature = "json")]
#[derive(Debug)]
pub enum StructResolveError<E> {
	/// Serialization error.
	Serialize(serde_json::Error),
	/// Resolution error.
	Resolve(Error<E>),
	/// Deserialization error.
	Deserialize(serde_json::Error),
}

#[cfg(feature = "json")]
impl<E: core::fmt::Display> core::fmt::Display for StructResolveError<E> {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			Self::Serialize(e) => write!(f, "serialization error: {e}"),
			Self::Resolve(e) => write!(f, "resolution error: {e}"),
			Self::Deserialize(e) => write!(f, "deserialization error: {e}"),
		}
	}
}

#[cfg(all(feature = "json", feature = "std"))]
impl<E: std::error::Error + 'static> std::error::Error for StructResolveError<E> {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			Self::Serialize(e) | Self::Deserialize(e) => Some(e),
			Self::Resolve(e) => Some(e),
		}
	}
}

/// Resolve strings in any serializable struct via JSON round-trip.
///
/// This function serializes the value to JSON, resolves all strings,
/// and deserializes back to the original type.
///
/// # Errors
///
/// Returns an error if:
/// - Serialization fails
/// - The resolver returns an error
/// - The depth limit is exceeded
/// - Deserialization fails
#[cfg(feature = "json")]
pub async fn resolve_struct<T, R>(
	value: T,
	resolver: &R,
	config: &Config,
) -> Result<T, StructResolveError<R::Error>>
where
	T: serde::Serialize + serde::de::DeserializeOwned,
	R: Resolver,
{
	let json = serde_json::to_value(value).map_err(StructResolveError::Serialize)?;
	let resolved = json::resolve(json, resolver, config)
		.await
		.map_err(StructResolveError::Resolve)?;
	serde_json::from_value(resolved).map_err(StructResolveError::Deserialize)
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloc::string::ToString;

	#[test]
	fn test_resolved_changed() {
		let r = Resolved::changed("hello");
		assert!(r.is_changed());
		assert!(!r.is_unchanged());
		assert_eq!(r, Resolved::Changed("hello".to_string()));
	}

	#[test]
	fn test_resolved_unchanged() {
		let r = Resolved::unchanged();
		assert!(r.is_unchanged());
		assert!(!r.is_changed());
		assert_eq!(r, Resolved::Unchanged);
	}

	#[test]
	fn test_resolved_from_string() {
		let r: Resolved = String::from("test").into();
		assert_eq!(r, Resolved::Changed("test".to_string()));
	}

	#[test]
	fn test_resolved_from_str() {
		let r: Resolved = "test".into();
		assert_eq!(r, Resolved::Changed("test".to_string()));
	}

	#[test]
	fn test_config_default() {
		let config = Config::default();
		assert_eq!(config.max_depth, 32);
		assert!(!config.resolve_keys);
	}

	#[test]
	fn test_config_builder() {
		let config = Config::new().max_depth(10).resolve_keys(true);
		assert_eq!(config.max_depth, 10);
		assert!(config.resolve_keys);
	}

	#[test]
	fn test_config_unlimited_depth() {
		let config = Config::new().unlimited_depth();
		assert_eq!(config.max_depth, usize::MAX);
	}

	#[test]
	fn test_error_display() {
		let err: Error<&str> = Error::resolver("custom error");
		assert_eq!(err.to_string(), "resolver error: custom error");

		let err: Error<&str> = Error::depth_exceeded(10);
		assert_eq!(err.to_string(), "depth limit (10) exceeded");
	}

	#[test]
	fn test_path_segment() {
		let key = PathSegment::Key("foo".to_string());
		let index = PathSegment::Index(42);

		assert_eq!(key, PathSegment::Key("foo".to_string()));
		assert_eq!(index, PathSegment::Index(42));
		assert_ne!(key, index);
	}
}

#[cfg(all(test, feature = "json"))]
mod json_tests {
	use super::*;
	use alloc::string::ToString;
	use core::convert::Infallible;
	use serde::{Deserialize, Serialize};

	#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
	struct TestStruct {
		name: String,
		value: i32,
	}

	#[tokio::test]
	async fn test_resolve_struct_basic() {
		let input = TestStruct {
			name: "hello".to_string(),
			value: 42,
		};

		let output = resolve_struct(
			input,
			&|s: &str| {
				let s = s.to_string();
				async move { Ok::<_, Infallible>(Resolved::changed(s.to_uppercase())) }
			},
			&Config::default(),
		)
		.await
		.unwrap();

		assert_eq!(output.name, "HELLO");
		assert_eq!(output.value, 42);
	}

	#[tokio::test]
	async fn test_resolve_struct_unchanged() {
		let input = TestStruct {
			name: "hello".to_string(),
			value: 42,
		};

		let output = resolve_struct(
			input.clone(),
			&|_: &str| async move { Ok::<_, Infallible>(Resolved::unchanged()) },
			&Config::default(),
		)
		.await
		.unwrap();

		assert_eq!(output, input);
	}

	#[tokio::test]
	async fn test_resolve_struct_error() {
		#[derive(Debug)]
		struct MyError;

		let input = TestStruct {
			name: "hello".to_string(),
			value: 42,
		};

		let result = resolve_struct(
			input,
			&|_: &str| async move { Err::<Resolved, _>(MyError) },
			&Config::default(),
		)
		.await;

		assert!(matches!(result, Err(StructResolveError::Resolve(_))));
	}
}
