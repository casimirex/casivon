//! Prints the OpenAPI document to stdout.
//!
//! The frontend's types are generated from this, so it has to be producible
//! without a database, a server or a port:
//!
//! ```sh
//! cargo run --bin openapi > ../frontend/openapi.json
//! ```

use casivon_backend::openapi::ApiDoc;
use utoipa::OpenApi;

fn main() -> anyhow::Result<()> {
    println!("{}", ApiDoc::openapi().to_pretty_json()?);
    Ok(())
}
