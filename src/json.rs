/* src/json.rs */

//!
//! This module is available with the `json` feature and supports `no_std` environments.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::pin::Pin;

use serde_json::{Map, Value};

use crate::{Config, Error, Resolved, Resolver};

/// Resolve all strings in a JSON [`Value`].
///
/// Recursively traverses the value and passes each string to the resolver.
/// The resolver decides whether to transform or skip each string.
///
/// # Errors
///
/// Returns an error if:
/// - The resolver returns an error
/// - The depth limit is exceeded
///
/// # Example
///
/// ```rust
/// use serde_resolve::{json, Config, Resolved};
///
/// # async fn example() -> Result<(), serde_resolve::Error<std::convert::Infallible>> {
/// let input = serde_json::json!({
///     "message": "hello",
///     "nested": { "value": "world" }
/// });
///
/// let output = json::resolve(
///     input,
///     &|s: &str| {
///         let s = s.to_string();
///         async move {
///             Ok::<_, std::convert::Infallible>(Resolved::changed(s.to_uppercase()))
///         }
///     },
///     &Config::default(),
/// ).await?;
///
/// assert_eq!(output["message"], "HELLO");
/// assert_eq!(output["nested"]["value"], "WORLD");
/// # Ok(())
/// # }
/// ```
pub async fn resolve<R>(
	value: Value,
	resolver: &R,
	config: &Config,
) -> Result<Value, Error<R::Error>>
where
	R: Resolver,
{
	resolve_recursive(value, resolver, config, 0).await
}

/// Internal recursive implementation.
fn resolve_recursive<'a, R>(
	value: Value,
	resolver: &'a R,
	config: &'a Config,
	depth: usize,
) -> Pin<Box<dyn core::future::Future<Output = Result<Value, Error<R::Error>>> + Send + 'a>>
where
	R: Resolver,
{
	Box::pin(async move {
		// Check depth limit
		if depth > config.max_depth {
			return Err(Error::depth_exceeded(config.max_depth));
		}

		#[cfg(feature = "tracing")]
		tracing::trace!(depth, value_type = ?value_type_name(&value), "resolving");

		match value {
			Value::String(s) => match resolver.resolve(&s).await.map_err(Error::resolver)? {
				Resolved::Changed(new_s) => {
					#[cfg(feature = "tracing")]
					tracing::trace!(original = %s, resolved = %new_s, "string changed");
					Ok(Value::String(new_s))
				}
				Resolved::Unchanged => {
					#[cfg(feature = "tracing")]
					tracing::trace!(value = %s, "string unchanged");
					Ok(Value::String(s))
				}
			},

			Value::Array(arr) => {
				let mut result = Vec::with_capacity(arr.len());
				for item in arr {
					result.push(resolve_recursive(item, resolver, config, depth + 1).await?);
				}
				Ok(Value::Array(result))
			}

			Value::Object(map) => {
				let mut result = Map::with_capacity(map.len());
				for (key, val) in map {
					// Optionally resolve keys
					let resolved_key = if config.resolve_keys {
						match resolver.resolve(&key).await.map_err(Error::resolver)? {
							Resolved::Changed(new_key) => new_key,
							Resolved::Unchanged => key,
						}
					} else {
						key
					};

					let resolved_val = resolve_recursive(val, resolver, config, depth + 1).await?;
					result.insert(resolved_key, resolved_val);
				}
				Ok(Value::Object(result))
			}

			// Pass through non-string primitives unchanged
			other @ (Value::Null | Value::Bool(_) | Value::Number(_)) => Ok(other),
		}
	})
}

#[cfg(feature = "tracing")]
fn value_type_name(value: &Value) -> &'static str {
	match value {
		Value::Null => "null",
		Value::Bool(_) => "bool",
		Value::Number(_) => "number",
		Value::String(_) => "string",
		Value::Array(_) => "array",
		Value::Object(_) => "object",
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloc::string::ToString;
	use core::convert::Infallible;

	#[tokio::test]
	async fn test_resolve_string() {
		let input = Value::String("hello".into());
		let output = resolve(
			input,
			&|s: &str| {
				let s = s.to_string();
				async move { Ok::<_, Infallible>(Resolved::changed(s.to_uppercase())) }
			},
			&Config::default(),
		)
		.await
		.unwrap();

		assert_eq!(output, Value::String("HELLO".into()));
	}

	#[tokio::test]
	async fn test_resolve_unchanged() {
		let input = Value::String("hello".into());
		let output = resolve(
			input,
			&|_: &str| async move { Ok::<_, Infallible>(Resolved::unchanged()) },
			&Config::default(),
		)
		.await
		.unwrap();

		assert_eq!(output, Value::String("hello".into()));
	}

	#[tokio::test]
	async fn test_resolve_nested() {
		let input = serde_json::json!({
				"a": "one",
				"b": {
						"c": "two",
						"d": ["three", "four"]
				}
		});

		let output = resolve(
			input,
			&|s: &str| {
				let s = s.to_string();
				async move { Ok::<_, Infallible>(Resolved::changed(s.to_uppercase())) }
			},
			&Config::default(),
		)
		.await
		.unwrap();

		assert_eq!(output["a"], "ONE");
		assert_eq!(output["b"]["c"], "TWO");
		assert_eq!(output["b"]["d"][0], "THREE");
		assert_eq!(output["b"]["d"][1], "FOUR");
	}

	#[tokio::test]
	async fn test_selective_resolve() {
		let input = serde_json::json!({
				"template": "{{name}}",
				"static": "no template"
		});

		let output = resolve(
			input,
			&|s: &str| {
				let s = s.to_string();
				async move {
					if s.contains("{{") {
						Ok::<_, Infallible>(Resolved::changed(s.replace("{{name}}", "Alice")))
					} else {
						Ok(Resolved::unchanged())
					}
				}
			},
			&Config::default(),
		)
		.await
		.unwrap();

		assert_eq!(output["template"], "Alice");
		assert_eq!(output["static"], "no template");
	}

	#[tokio::test]
	async fn test_depth_limit() {
		// Create deeply nested structure
		let mut value = Value::String("deep".into());
		for _ in 0..50 {
			value = serde_json::json!({ "nested": value });
		}

		let result = resolve(
			value,
			&|s: &str| {
				let s = s.to_string();
				async move { Ok::<_, Infallible>(Resolved::changed(s)) }
			},
			&Config::default().max_depth(10),
		)
		.await;

		assert!(matches!(result, Err(Error::DepthExceeded { limit: 10 })));
	}

	#[tokio::test]
	async fn test_resolve_keys() {
		let input = serde_json::json!({
				"{{key}}": "value"
		});

		let output = resolve(
			input,
			&|s: &str| {
				let s = s.to_string();
				async move { Ok::<_, Infallible>(Resolved::changed(s.replace("{{key}}", "resolved_key"))) }
			},
			&Config::default().resolve_keys(true),
		)
		.await
		.unwrap();

		assert!(output.get("resolved_key").is_some());
		assert_eq!(output["resolved_key"], "value");
	}

	#[tokio::test]
	async fn test_non_string_unchanged() {
		let input = serde_json::json!({
				"number": 42,
				"bool": true,
				"null": null
		});

		let output = resolve(
			input.clone(),
			&|s: &str| {
				let s = s.to_string();
				async move { Ok::<_, Infallible>(Resolved::changed(s.to_uppercase())) }
			},
			&Config::default(),
		)
		.await
		.unwrap();

		assert_eq!(output, input);
	}

	#[tokio::test]
	async fn test_resolver_error() {
		#[derive(Debug)]
		struct MyError;

		let input = Value::String("hello".into());
		let result = resolve(
			input,
			&|_: &str| async move { Err::<Resolved, _>(MyError) },
			&Config::default(),
		)
		.await;

		assert!(matches!(result, Err(Error::Resolver(MyError))));
	}

	#[tokio::test]
	async fn test_empty_structures() {
		let empty_array = resolve(
			Value::Array(vec![]),
			&|s: &str| {
				let s = s.to_string();
				async move { Ok::<_, Infallible>(Resolved::changed(s)) }
			},
			&Config::default(),
		)
		.await
		.unwrap();
		assert_eq!(empty_array, Value::Array(vec![]));

		let empty_object = resolve(
			Value::Object(Map::new()),
			&|s: &str| {
				let s = s.to_string();
				async move { Ok::<_, Infallible>(Resolved::changed(s)) }
			},
			&Config::default(),
		)
		.await
		.unwrap();
		assert_eq!(empty_object, Value::Object(Map::new()));
	}
}
