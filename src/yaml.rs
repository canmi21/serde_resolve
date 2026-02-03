//! YAML support via `serde_yaml`.
//!
//! This module requires the `std` feature.

use alloc::boxed::Box;
#[cfg(feature = "tracing")]
use alloc::vec::Vec;
use serde_yaml::Value;

use crate::{Config, Error, Resolver};

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
		Value::Sequence,
		Value::Mapping,
		serde_yaml::Mapping::with_capacity,
		|k: &Value| format!("{k:?}"),
		resolver, config, depth, path, key,
		{
				resolve_recursive(
						key,
						resolver,
						config,
						depth + 1,
						#[cfg(feature = "tracing")]
						path,
				)
				.await?
		},
		{
				// Tagged values - resolve inner
				Value::Tagged(tagged) => {
						let resolved_inner = resolve_recursive(
								tagged.value,
								resolver,
								config,
								depth + 1,
								#[cfg(feature = "tracing")]
								path,
						)
						.await?;
						Ok(Value::Tagged(Box::new(serde_yaml::value::TaggedValue {
								tag: tagged.tag,
								value: resolved_inner,
						})))
				}

				// Pass through unchanged
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
		Value::Sequence(_) => "sequence",
		Value::Mapping(_) => "mapping",
		Value::Tagged(_) => "tagged",
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Resolved;
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

	#[tokio::test]
	async fn test_depth_limit() {
		let mut value = Value::String("deep".into());
		for _ in 0..32 {
			let mut map = Mapping::new();
			map.insert(Value::String("nested".into()), value);
			value = Value::Mapping(map);
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
		let empty_seq = resolve(
			Value::Sequence(vec![]),
			&|s: &str| {
				let s = s.to_string();
				async move { Ok::<_, Infallible>(Resolved::changed(s)) }
			},
			&Config::default(),
		)
		.await
		.unwrap();
		assert_eq!(empty_seq, Value::Sequence(vec![]));

		let empty_map = resolve(
			Value::Mapping(Mapping::new()),
			&|s: &str| {
				let s = s.to_string();
				async move { Ok::<_, Infallible>(Resolved::changed(s)) }
			},
			&Config::default(),
		)
		.await
		.unwrap();
		assert_eq!(empty_map, Value::Mapping(Mapping::new()));
	}

	#[tokio::test]
	async fn test_non_string_unchanged() {
		let input = Value::Number(42.into());
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
