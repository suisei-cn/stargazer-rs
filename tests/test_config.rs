use std::path::{Path, PathBuf};
use std::str::FromStr;

use stargazer_lib::Config;

#[test]
fn must_load_config() {
    let _ = Config::new(Some(
        &*PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/config.toml"),
    ))
    .unwrap();
}
