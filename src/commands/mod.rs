/// `init` と `migrate` が「次にやること」で出す案内。
///
/// 同じコマンドの説明を 2 か所で別の言い方にしていたので、1 つにまとめた。
pub const NEXT_ENV_UPDATE: &str =
    "  acrust env update   # ジャッジ環境（依存・Cargo.lock・rustc）に合わせる";

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
