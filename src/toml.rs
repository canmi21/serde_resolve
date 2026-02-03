/* src/toml.rs */

//!
//! This module requires the `std` feature.

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
}
