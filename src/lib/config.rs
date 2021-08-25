use std::net::IpAddr;
use std::path::Path;

use config::Source;
use serde::de;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::ConfigError;

/// Contains all configuration to run the application.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Hash)]
pub struct Config {
    #[serde(rename = "HTTP")]
    http: HTTP,
}

impl Config {
    /// Construct a new [`Config`](Config).
    ///
    /// `/etc/stargazer/config.toml`, `~/.config/stargazer/config.toml` and `./config.toml` will be merged in order if exists.
    /// If a path is specified, the files described earlier will be ignored.
    ///
    /// `toml` and `json` are both supported.
    ///
    /// Environmental values started with `stargazer_` may override parameters.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`](ConfigError) if there's no config source available or there's any parsing error.
    #[allow(clippy::missing_panics_doc)]
    pub fn new(config: Option<&Path>) -> Result<Self, ConfigError> {
        let config = if let Some(config) = config {
            config::Config::new().with_merged(config::File::from(config).required(false))?
        } else {
            config::Config::new()
                .with_merged(config::File::with_name("/etc/stargazer/config").required(false))?
                .with_merged(
                    config::File::from(
                        dirs::config_dir()
                            .unwrap_or_default()
                            .join("stargazer/config"),
                    )
                    .required(false),
                )?
                .with_merged(config::File::with_name("config").required(false))?
        }
        .with_merged(config::Environment::with_prefix("stargazer"))?;

        if config.collect().unwrap().is_empty() {
            return Err(ConfigError::Missing);
        }

        Ok(config.try_into()?)
    }
}

// TODO workaround before https://github.com/serde-rs/serde/pull/2056 is merged
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
// #[serde(tag = "enabled")]
pub enum HTTP {
    // #[serde(rename = false)]
    Disabled,
    // #[serde(rename = true)]
    Enabled { host: IpAddr, port: u16 },
}

impl Serialize for HTTP {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        #[serde(untagged)]
        enum Body {
            Disabled,
            Enabled { host: IpAddr, port: u16 },
        }
        #[derive(Serialize)]
        struct Tagged {
            enabled: bool,
            #[serde(flatten)]
            body: Body,
        }
        match self {
            HTTP::Disabled => Tagged {
                enabled: false,
                body: Body::Disabled,
            },
            HTTP::Enabled { host, port } => Tagged {
                enabled: true,
                body: Body::Enabled {
                    host: *host,
                    port: *port,
                },
            },
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for HTTP {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Map::deserialize(deserializer)?;

        let enabled = value
            .get("enabled")
            .ok_or_else(|| de::Error::missing_field("enabled"))
            .map(Deserialize::deserialize)?
            .map_err(de::Error::custom)?;

        Ok(if enabled {
            Self::Enabled {
                host: value
                    .get("host")
                    .ok_or_else(|| de::Error::missing_field("host"))
                    .map(Deserialize::deserialize)?
                    .map_err(de::Error::custom)?,
                port: value
                    .get("port")
                    .ok_or_else(|| de::Error::missing_field("port"))
                    .map(Deserialize::deserialize)?
                    .map_err(de::Error::custom)?,
            }
        } else {
            Self::Disabled
        })
    }
}
