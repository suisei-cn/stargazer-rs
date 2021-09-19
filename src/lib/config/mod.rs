use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;
use std::time::Duration;

use figment::providers::{Env, Serialized};
use figment::{Error, Figment};
use serde::de;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use provider::ConfigFile;

mod provider;
#[cfg(test)]
mod tests;

pub type ScheduleConfig = Schedule;
pub type ScheduleHTTP = HTTP;

/// Contains all configuration to run the application.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Hash, Default)]
pub struct Config {
    http: HTTP,
    schedule: Schedule,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize, Hash)]
pub struct Schedule {
    /// Interval between schedule attempts.
    #[serde(with = "humantime_serde")]
    schedule_interval: Duration,
    /// Max allowed duration for an entry to be an orphan.
    #[serde(with = "humantime_serde")]
    max_interval: Duration,
}

impl Schedule {
    pub const fn schedule_interval(&self) -> Duration {
        self.schedule_interval
    }
    pub const fn max_interval(&self) -> Duration {
        self.max_interval
    }
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            schedule_interval: Duration::from_secs(3),
            max_interval: Duration::from_secs(10),
        }
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

impl Default for HTTP {
    fn default() -> Self {
        Self::Enabled {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 8080,
        }
    }
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
    pub fn new(config: Option<&Path>) -> Result<Self, Error> {
        let config = config
            .map_or_else(
                || {
                    Figment::from(Serialized::defaults(Self::default()))
                        .merge(ConfigFile::file("/etc/stargazer/config"))
                        .merge(ConfigFile::file(
                            dirs::config_dir()
                                .unwrap_or_default()
                                .join("stargazer/config"),
                        ))
                        .merge(ConfigFile::file("config"))
                },
                |config| {
                    Figment::from(Serialized::defaults(Self::default()))
                        .merge(ConfigFile::file(config))
                },
            )
            .merge(Env::prefixed("STARGAZER_").split("_"));

        config.extract()
    }
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
