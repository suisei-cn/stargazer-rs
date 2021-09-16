use super::{Config, HTTP};
use figment::Jail;

#[test]
fn must_load_specified() {
    Jail::expect_with(|jail| {
        jail.create_file("config.toml", include_str!("../../../tests/config.toml"))?;
        Config::new(Some("config.toml".as_ref()))?;
        Config::new(Some("config".as_ref()))?;
        Ok(())
    });
}

#[test]
fn must_env_overwrite() {
    Jail::expect_with(|jail| {
        jail.create_file("config.toml", include_str!("../../../tests/config.toml"))?;
        jail.set_env("STARGAZER_HTTP_ENABLED", "false");
        assert!(matches!(
            Config::new(Some("config.toml".as_ref()))?.http,
            HTTP::Disabled
        ));
        Ok(())
    });
}
