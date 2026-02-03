//! Custom Resolver: implement the Resolver trait for reusable logic.

use serde_resolve::{json, Config, Resolved, Resolver};

/// A resolver that prefixes all strings with a given prefix.
struct PrefixResolver {
    prefix: String,
}

impl Resolver for PrefixResolver {
    type Error = std::convert::Infallible;

    async fn resolve(&self, input: &str) -> Result<Resolved, Self::Error> {
        Ok(Resolved::changed(format!("{}{}", self.prefix, input)))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = serde_json::json!({
        "key": "value",
        "nested": { "inner": "data" }
    });

    let resolver = PrefixResolver {
        prefix: "[resolved] ".to_string(),
    };

    let output = json::resolve(input, &resolver, &Config::default()).await?;

    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
}
