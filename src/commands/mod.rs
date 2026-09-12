/// The "next step" line `init` and `migrate` both print. Shared because the two
/// had drifted into describing the same command in two different ways.
pub const NEXT_ENV_UPDATE: &str =
    "  acrust env update   # match the judge environment (crates, Cargo.lock, rustc)";

pub mod auth;
pub mod contest;
pub mod copy;
pub mod env;
pub mod init;
pub mod migrate;
pub mod open;
pub mod run;
pub mod submit;
pub mod test;
