/* src/toml.rs */

//! TOML support via `toml` crate.
//!
//! This module requires the `std` feature.

#[cfg(feature = "tracing")]
use alloc::vec::Vec;
use toml::Value;

use crate::{Config, Error, Resolver};

/// Resolve all strings in a TOML [`Value`].
///
/// See [`crate::json::resolve`] for detailed documentation.
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
		Value::Table,
		toml::map::Map::with_capacity,
		|k: &alloc::string::String| k.clone(),
		resolver, config, depth, path, key,
		{
				match resolver.resolve(&key).await.map_err(crate::Error::resolver)? {
						crate::Resolved::Changed(new_key) => new_key,
						crate::Resolved::Unchanged => key,
				}
		},
		{
				// TOML-specific types
				Value::Datetime(dt) => Ok(Value::Datetime(dt)),

				// Pass through unchanged
				other @ (Value::Integer(_) | Value::Float(_) | Value::Boolean(_)) => Ok(other),
		}
);

#[cfg(feature = "tracing")]
fn value_type_name(value: &Value) -> &'static str {
	match value {
		Value::String(_) => "string",
		Value::Integer(_) => "integer",
		Value::Float(_) => "float",
		Value::Boolean(_) => "boolean",
		Value::Datetime(_) => "datetime",
		Value::Array(_) => "array",
		Value::Table(_) => "table",
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Resolved;
	use alloc::string::ToString;
	use core::convert::Infallible;
	use toml::map::Map;

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
	async fn test_resolve_table() {
		let mut map = Map::new();
		map.insert("key".into(), Value::String("value".into()));
		let input = Value::Table(map);

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

		let mut expected = Map::new();
		expected.insert("key".into(), Value::String("VALUE".into()));
		assert_eq!(output, Value::Table(expected));
	}

	#[tokio::test]
	async fn test_resolve_table_keys() {
		let mut map = Map::new();
		map.insert("key".into(), Value::String("value".into()));
		let input = Value::Table(map);

		let output = resolve(
			input,
			&|s: &str| {
				let s = s.to_string();
				async move { Ok::<_, Infallible>(Resolved::changed(s.to_uppercase())) }
			},
			&Config::default().resolve_keys(true),
		)
		.await
		.unwrap();

		let mut expected = Map::new();
		expected.insert("KEY".into(), Value::String("VALUE".into()));
		assert_eq!(output, Value::Table(expected));
	}

	#[tokio::test]
	async fn test_resolve_array() {
		let input = Value::Array(vec![Value::String("a".into()), Value::String("b".into())]);
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

		assert_eq!(
			output,
			Value::Array(vec![Value::String("A".into()), Value::String("B".into())])
		);
	}

	#[tokio::test]
	async fn test_depth_limit() {
		let mut value = Value::String("deep".into());
		for _ in 0..32 {
			let mut map = Map::new();
			map.insert("nested".into(), value);
			value = Value::Table(map);
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

		let empty_table = resolve(
			Value::Table(Map::new()),
			&|s: &str| {
				let s = s.to_string();
				async move { Ok::<_, Infallible>(Resolved::changed(s)) }
			},
			&Config::default(),
		)
		.await
		.unwrap();
		assert_eq!(empty_table, Value::Table(Map::new()));
	}

	#[tokio::test]
	async fn test_non_string_unchanged() {
		let input = Value::Integer(42);
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
}