/* examples/resolve_keys.rs */

//! Resolve object keys in addition to values.

use serde_resolve::{json, Config, Resolved};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = serde_json::json!({
        "{{key}}": "value",
        "normal_key": "{{value}}"
    });

    let output = json::resolve(
        input,
        &|s: &str| {
            let s = s.to_string();
            async move {
                let resolved = s
                    .replace("{{key}}", "resolved_key")
                    .replace("{{value}}", "resolved_value");
                if resolved != s {
                    Ok::<_, std::convert::Infallible>(Resolved::changed(resolved))
                } else {
                    Ok(Resolved::unchanged())
                }
            }
        },
        &Config::default().resolve_keys(true), // Enable key resolution
    )
    .await?;

    println!("{}", serde_json::to_string_pretty(&output)?);
    // Output: { "resolved_key": "value", "normal_key": "resolved_value" }

    Ok(())
}