//! JSON support via `serde_json`.
//!
//! This module is available with the `json` feature and supports `no_std` environments.

#[cfg(feature = "tracing")]
use alloc::vec::Vec;
use serde_json::{Map, Value};

use crate::{Config, Error, Resolver};

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
	#[cfg(feature = "tracing")]
	let mut path = Vec::new();

	resolve_recursive(
		value,
		resolver,
		config,
		0,
		#[cfg(feature = "tracing")]
		&mut path,
	)
	.await
}

impl_resolve_recursive!(
		Value,
		Value::String,
		Value::Array,
		Value::Object,
		Map::with_capacity,
		|k: &alloc::string::String| k.clone(),
		resolver, config, depth, path, key,
		{
				match resolver.resolve(&key).await.map_err(crate::Error::resolver)? {
						crate::Resolved::Changed(new_key) => new_key,
						crate::Resolved::Unchanged => key,
				}
		},
		{
				other @ (Value::Null | Value::Bool(_) | Value::Number(_)) => Ok(other),
		}
);

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
	use crate::Resolved;
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
		for _ in 0..32 {
			value = serde_json::json!({ "nested": value });
		}

		let result = resolve(
			value,
			&|s: &str| {
				let s = s.to_string();
				async move { Ok::<_, Infallible>(Resolved::changed(s)) }
			},
			&Config::default().max_depth(32),
		)
		.await;

		assert!(matches!(result, Err(Error::DepthExceeded { limit: 32 })));
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

	#[tokio::test]
	async fn test_depth_zero() {
		let input = Value::String("hello".into());
		let result = resolve(
			input,
			&|s: &str| {
				let s = s.to_string();
				async move { Ok::<_, Infallible>(Resolved::changed(s)) }
			},
			&Config::default().max_depth(0),
		)
		.await;

		assert!(matches!(result, Err(Error::DepthExceeded { limit: 0 })));
	}

	#[tokio::test]
	async fn test_nested_arrays() {
		let input = serde_json::json!([["a", "b"], ["c", "d"]]);
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

		assert_eq!(output, serde_json::json!([["A", "B"], ["C", "D"]]));
	}
}
