# Serde Resolve

Recursively traverse serde data structures and transform string values.

`serde_resolve` provides async string transformation for nested JSON, YAML, and TOML structures. Pass a resolver function to selectively transform strings based on patterns, templates, or any custom logic.

## Features

- **Async Traversal**: Recursively walk any serde-compatible structure with async resolvers.
- **Selective Transformation**: Return `Resolved::Changed` to transform or `Resolved::Unchanged` to skip.
- **Multi-Format Support**: Works with JSON (`no_std`), YAML, and TOML value types.
- **Typed Structs**: `resolve_struct()` transforms any `Serialize + DeserializeOwned` type via JSON round-trip.
- **Key Resolution**: Optionally resolve object/map keys in addition to values.
- **Depth Limiting**: Configurable max depth to prevent stack overflow on malicious input.

## Usage Examples

Check the `examples` directory for runnable code:

- **Basic Usage**: [`examples/basic.rs`](examples/basic.rs) - Transform all strings to uppercase.
- **Selective Resolution**: [`examples/selective.rs`](examples/selective.rs) - Only transform strings matching a pattern.
- **Custom Resolver**: [`examples/custom_resolver.rs`](examples/custom_resolver.rs) - Implement the `Resolver` trait for reusable logic.
- **Typed Structs**: [`examples/resolve_struct.rs`](examples/resolve_struct.rs) - Resolve strings in a typed struct.
- **Key Resolution**: [`examples/resolve_keys.rs`](examples/resolve_keys.rs) - Resolve object keys in addition to values.

## Installation

```toml
[dependencies]
serde_resolve = { version = "0.1", features = ["full"] }
```

## Feature Flags

| Feature | Description |
|---------|-------------|
| `std` | Standard library support (enabled by default). |
| `json` | JSON support via `serde_json` (`no_std` compatible). |
| `yaml` | YAML support via `serde_yaml` (requires `std`). |
| `toml` | TOML support via `toml` crate (requires `std`). |
| `tracing` | Debug logging via `tracing` crate. |
| `full` | Enables all features above. |

## License

Released under the MIT License © 2026 [Canmi](https://github.com/canmi21)
