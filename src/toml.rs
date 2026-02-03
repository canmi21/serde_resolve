/* src/toml.rs */

//!
//! This module requires the `std` feature.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::pin::Pin;
use toml::Value;

use crate::{Config, Error, Resolved, Resolver};

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
	resolve_recursive(value, resolver, config, 0).await
}

fn resolve_recursive<'a, R>(
	value: Value,
	resolver: &'a R,
	config: &'a Config,
	depth: usize,
) -> Pin<Box<dyn std::future::Future<Output = Result<Value, Error<R::Error>>> + Send + 'a>>
where
	R: Resolver,
{
	Box::pin(async move {
		if depth > config.max_depth {
			return Err(Error::depth_exceeded(config.max_depth));
		}

		match value {
			Value::String(s) => match resolver.resolve(&s).await.map_err(Error::resolver)? {
				Resolved::Changed(new_s) => Ok(Value::String(new_s)),
				Resolved::Unchanged => Ok(Value::String(s)),
			},

			Value::Array(arr) => {
				let mut result = Vec::with_capacity(arr.len());
				for item in arr {
					result.push(resolve_recursive(item, resolver, config, depth + 1).await?);
				}
				Ok(Value::Array(result))
			}

			Value::Table(table) => {
				let mut result = toml::map::Map::with_capacity(table.len());
				for (key, val) in table {
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
				Ok(Value::Table(result))
			}

			// TOML-specific types
			Value::Datetime(dt) => Ok(Value::Datetime(dt)),

			// Pass through unchanged
			other @ (Value::Integer(_) | Value::Float(_) | Value::Boolean(_)) => Ok(other),
		}
	})
}

#[cfg(test)]
mod tests {
	use super::*;
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
