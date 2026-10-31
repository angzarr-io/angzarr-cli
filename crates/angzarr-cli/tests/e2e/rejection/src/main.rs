//! The rejection app on angzarr-embedded: the player launched as a Python
//! child, registered by the generated `register()` with the host config
//! beside it, then the driver.

pub mod wire {
    include!(concat!(env!("OUT_DIR"), "/io.angzarr.examples.v1.rs"));
}
pub mod domain {
    include!(concat!(
        env!("OUT_DIR"),
        "/io.angzarr.examples.v1.domain.rs"
    ));
}
pub mod adapters {
    include!(concat!(env!("OUT_DIR"), "/angzarr-adapters.rs"));
}
include!(concat!(env!("OUT_DIR"), "/angzarr-register.rs"));

pub mod driver;

/// Runs the application, then its driver, on tokio's runtime (the App is
/// async, on the caller's). Dropping the application stops the child.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = angzarr_embedded_host::run("angzarr.toml", register).await?;
    driver::drive(&app).await
}
