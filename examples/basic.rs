/* examples/basic.rs */

//! Basic example: transform all strings to uppercase.

use serde_resolve::{json, Config, Resolved};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = serde_json::json!({
        "greeting": "hello",
        "name": "world"
    });

    let output = json::resolve(
        input,
        &|s: &str| {
            let upper = s.to_uppercase();
            async move { Ok::<_, std::convert::Infallible>(Resolved::changed(upper)) }
        },
        &Config::default(),
    )
    .await?;

    println!("{}", serde_json::to_string_pretty(&output)?);
    // Output: { "greeting": "HELLO", "name": "WORLD" }

    Ok(())
}