/* src/yaml.rs */

//!
//! This module requires the `std` feature.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::pin::Pin;
use serde_yaml::Value;

use crate::{Config, Error, Resolved, Resolver};

/// Resolve all strings in a YAML [`Value`].
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

			Value::Sequence(seq) => {
				let mut result = Vec::with_capacity(seq.len());
				for item in seq {
					result.push(resolve_recursive(item, resolver, config, depth + 1).await?);
				}
				Ok(Value::Sequence(result))
			}

			Value::Mapping(map) => {
				let mut result = serde_yaml::Mapping::with_capacity(map.len());
				for (key, val) in map {
					let resolved_key = if config.resolve_keys {
						resolve_recursive(key, resolver, config, depth + 1).await?
					} else {
						key
					};
					let resolved_val = resolve_recursive(val, resolver, config, depth + 1).await?;
					result.insert(resolved_key, resolved_val);
				}
				Ok(Value::Mapping(result))
			}

			// Tagged values - resolve inner
			Value::Tagged(tagged) => {
				let resolved_inner = resolve_recursive(tagged.value, resolver, config, depth + 1).await?;
				Ok(Value::Tagged(Box::new(serde_yaml::value::TaggedValue {
					tag: tagged.tag,
					value: resolved_inner,
				})))
			}

			// Pass through unchanged
			other @ (Value::Null | Value::Bool(_) | Value::Number(_)) => Ok(other),
		}
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloc::string::ToString;
	use core::convert::Infallible;
	use serde_yaml::Mapping;

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
	async fn test_resolve_sequence() {
		let input = Value::Sequence(vec![Value::String("a".into()), Value::String("b".into())]);
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
			Value::Sequence(vec![Value::String("A".into()), Value::String("B".into())])
		);
	}

	#[tokio::test]
	async fn test_resolve_mapping() {
		let mut map = Mapping::new();
		map.insert(Value::String("key".into()), Value::String("value".into()));
		let input = Value::Mapping(map);

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

		// Key unchanged (default), value changed
		let mut expected = Mapping::new();
		expected.insert(Value::String("key".into()), Value::String("VALUE".into()));
		assert_eq!(output, Value::Mapping(expected));
	}

	#[tokio::test]
	async fn test_resolve_mapping_keys() {
		let mut map = Mapping::new();
		map.insert(Value::String("key".into()), Value::String("value".into()));
		let input = Value::Mapping(map);

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

		// Key changed, value changed
		let mut expected = Mapping::new();
		expected.insert(Value::String("KEY".into()), Value::String("VALUE".into()));
		assert_eq!(output, Value::Mapping(expected));
	}

	#[tokio::test]
	async fn test_tagged() {
		// Tagged value: !mytag "hello"
		let tagged = serde_yaml::value::TaggedValue {
			tag: serde_yaml::value::Tag::new("!mytag"),
			value: Value::String("hello".into()),
		};
		let input = Value::Tagged(Box::new(tagged));

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

		match output {
			Value::Tagged(t) => {
				assert_eq!(t.tag.to_string(), "!mytag");
				assert_eq!(t.value, Value::String("HELLO".into()));
			}
			_ => panic!("Expected Tagged value"),
		}
	}
}
