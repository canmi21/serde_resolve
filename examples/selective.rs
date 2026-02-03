/* examples/selective.rs */

//! Selective resolution: only transform strings matching a pattern.

use serde_resolve::{json, Config, Resolved};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = serde_json::json!({
        "template": "Hello, {{name}}!",
        "static": "This stays unchanged"
    });

    let output = json::resolve(
        input,
        &|s: &str| {
            let s = s.to_string();
            async move {
                if s.contains("{{") {
                    // Only transform strings with placeholders
                    Ok::<_, std::convert::Infallible>(Resolved::changed(
                        s.replace("{{name}}", "Alice"),
                    ))
                } else {
                    // Skip strings without placeholders
                    Ok(Resolved::unchanged())
                }
            }
        },
        &Config::default(),
    )
    .await?;

    println!("{}", serde_json::to_string_pretty(&output)?);
    // Output: { "template": "Hello, Alice!", "static": "This stays unchanged" }

    Ok(())
}