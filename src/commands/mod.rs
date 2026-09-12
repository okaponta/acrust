/// `init` と `migrate` が「次にやること」で出す案内。
///
/// 同じコマンドの説明を 2 か所で別の言い方にしていたので、1 つにまとめた。
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
