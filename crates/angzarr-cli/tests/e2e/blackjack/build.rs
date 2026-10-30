//! The wire types, the markup domain layer, the adapters and `register()`,
//! generated into OUT_DIR from the protos; the player, served by the Python
//! child, gets a contract, not an adapter.

const PROTOS: &[&str] = &[
    "io/angzarr/examples/v1/table.proto",
    "io/angzarr/examples/v1/player.proto",
    "io/angzarr/examples/v1/policies.proto",
];
const INCLUDES: &[&str] = &["proto"];
const REMOTE: &[&str] = &["player"];
/// The package's prost types, for its domain layer.
const WIRE: &[(&str, &str)] = &[("io.angzarr.examples.v1", "crate::wire")];

fn main() -> Result<(), angzarr_build::BuildError> {
    angzarr_build::wire(PROTOS, INCLUDES)?;
    angzarr_build::markup_domain(PROTOS, INCLUDES, WIRE)?;
    angzarr_build::adapters(PROTOS, INCLUDES, REMOTE)?;
    angzarr_build::register(PROTOS, INCLUDES, REMOTE)
}
