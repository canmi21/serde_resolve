//! Resolve strings in a typed struct via JSON round-trip.

use serde::{Deserialize, Serialize};
use serde_resolve::{resolve_struct, Config, Resolved};

#[derive(Debug, Serialize, Deserialize)]
struct AppConfig {
    name: String,
    database_url: String,
    port: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig {
        name: "{{APP_NAME}}".to_string(),
        database_url: "postgres://{{DB_USER}}:{{DB_PASS}}@localhost/db".to_string(),
        port: 8080,
    };

    let env = std::collections::HashMap::from([
        ("APP_NAME", "MyApp"),
        ("DB_USER", "admin"),
        ("DB_PASS", "secret"),
    ]);

    let resolved: AppConfig = resolve_struct(
        config,
        &|s: &str| {
            let env = &env;
            let s = s.to_string();
            async move {
                let mut result = s.clone();
                for (key, val) in env {
                    result = result.replace(&format!("{{{{{}}}}}", key), val);
                }
                if result != s {
                    Ok::<_, std::convert::Infallible>(Resolved::changed(result))
                } else {
                    Ok(Resolved::unchanged())
                }
            }
        },
        &Config::default(),
    )
    .await?;

    println!("{:#?}", resolved);
    // AppConfig { name: "MyApp", database_url: "postgres://admin:secret@localhost/db", port: 8080 }

    Ok(())
}
