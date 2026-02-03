/* examples/simple.rs */

use serde_resolve::{Config, Resolved, json};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	// 1. Create a JSON value with some templates
	let input = serde_json::json!({
			"app_name": "{{app_name}}",
			"version": "1.0.0",
			"config": {
					"db_url": "postgres://{{user}}:{{password}}@localhost/db",
					"timeout": 30
			},
			"tags": ["{{env}}", "v1"]
	});

	println!("Input:\n{:#?}", input);

	// 2. Define a context for resolution
	let context = std::collections::HashMap::from([
		("app_name", "MySuperApp"),
		("user", "admin"),
		("password", "secret123"),
		("env", "production"),
	]);

	// 3. Resolve the value
	let resolved = json::resolve(
		input,
		&|s: &str| {
			let context = &context;
			let s = s.to_string();
			async move {
				if !s.contains("{{ ") {
					return Ok::<_, std::convert::Infallible>(Resolved::Unchanged);
				}

				let mut new_s = s.clone();
				for (key, val) in context {
					let placeholder = format!("{{{{{}}}}}", key);
					new_s = new_s.replace(&placeholder, val);
				}

				if new_s != s {
					Ok(Resolved::changed(new_s))
				} else {
					Ok(Resolved::Unchanged)
				}
			}
		},
		&Config::default(),
	)
	.await?;

	println!("\nResolved:\n{:#?}", resolved);

	Ok(())
}
