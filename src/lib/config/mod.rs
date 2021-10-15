use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;
use std::time::Duration;

use figment::providers::{Env, Serialized};
use figment::{Error, Figment};
use getset::CopyGetters;
use getset::Getters;
use serde::de;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use provider::ConfigFile;

mod provider;
#[cfg(test)]
mod tests;

pub type ScheduleConfig = Schedule;
pub type HTTPConfig = HTTP;
pub type MongoDBConfig = MongoDB;
pub type AMQPConfig = AMQP;
pub type TwitterConfig = Twitter;

/// Contains all configuration to run the application.
#[derive(
    Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Hash, Default, CopyGetters, Getters,
)]
pub struct Config {
    #[getset(get_copy = "pub")]
    basic: Basic,
    #[getset(get_copy = "pub")]
    http: HTTP,
    #[getset(get_copy = "pub")]
    schedule: Schedule,
    #[getset(get = "pub")]
    mongodb: MongoDB,
    #[getset(get = "pub")]
    collector: Collector,
    #[getset(get = "pub")]
    source: Source,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize, Hash, Default, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct Basic {
    /// Workers on instance. Non-set or zero value set the value to count of available logical cpu cores.
    #[serde(deserialize_with = "to_maybe_non_zero")]
    workers: Option<usize>,
}

fn to_maybe_non_zero<'de, D>(deserializer: D) -> Result<Option<usize>, D::Error>
where
    D: Deserializer<'de>,
{
    let num: Option<usize> = Deserialize::deserialize(deserializer)?;

    num.map_or(Ok(None), |num| Ok(if num == 0 { None } else { Some(num) }))
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize, Hash, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct Schedule {
    /// Interval between schedule attempts.
    #[serde(with = "humantime_serde")]
    schedule_interval: Duration,
    /// Interval between balance schedule attempts.
    #[serde(with = "humantime_serde")]
    balance_interval: Duration,
    /// Max allowed duration for an entry to be an orphan.
    #[serde(with = "humantime_serde")]
    max_interval: Duration,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            schedule_interval: Duration::from_secs(5),
            balance_interval: Duration::from_secs(30),
            max_interval: Duration::from_secs(60),
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Hash)]
pub struct MongoDB {
    uri: String,
    database: String,
}

impl MongoDB {
    pub fn uri(&self) -> &str {
        &self.uri
    }
    pub fn database(&self) -> &str {
        &self.database
    }
}

impl Default for MongoDB {
    fn default() -> Self {
        Self {
            uri: String::from("mongodb://localhost"),
            database: String::from("stargazer"),
        }
    }
}

#[derive(
    Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Hash, Default, Getters, CopyGetters,
)]
pub struct Collector {
    #[getset(get = "pub")]
    amqp: AMQP,
    #[getset(get_copy = "pub")]
    debug: DebugCollector,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum Twitter {
    Enabled { token: String },
    Disabled,
}

impl Default for Twitter {
    fn default() -> Self {
        Self::Disabled
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize, Hash, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct Bililive {
    enabled: bool,
}

impl Default for Bililive {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize, Hash, Default, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct DebugSource {
    enabled: bool,
}

#[derive(
    Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Hash, Default, CopyGetters, Getters,
)]
pub struct Source {
    #[getset(get = "pub")]
    twitter: Twitter,
    #[getset(get_copy = "pub")]
    bililive: Bililive,
    #[getset(get_copy = "pub")]
    debug: DebugSource,
}

// TODO workaround before https://github.com/serde-rs/serde/pull/2056 is merged
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
// #[serde(tag = "enabled")]
pub enum AMQP {
    // #[serde(rename = false)]
    Disabled,
    // #[serde(rename = true)]
    Enabled { uri: String, exchange: String },
}

impl Default for AMQP {
    fn default() -> Self {
        Self::Enabled {
            uri: String::from("amqp://127.0.0.1"),
            exchange: String::from("stargazer"),
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, Default, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct DebugCollector {
    enabled: bool,
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

impl Serialize for AMQP {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        #[serde(untagged)]
        enum Body {
            Disabled,
            Enabled { uri: String, exchange: String },
        }
        #[derive(Serialize)]
        struct Tagged {
            enabled: bool,
            #[serde(flatten)]
            body: Body,
        }
        match self {
            AMQP::Disabled => Tagged {
                enabled: false,
                body: Body::Disabled,
            },
            AMQP::Enabled { uri, exchange } => Tagged {
                enabled: true,
                body: Body::Enabled {
                    uri: uri.clone(),
                    exchange: exchange.clone(),
                },
            },
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AMQP {
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
                uri: value
                    .get("uri")
                    .ok_or_else(|| de::Error::missing_field("uri"))
                    .map(Deserialize::deserialize)?
                    .map_err(de::Error::custom)?,
                exchange: value
                    .get("exchange")
                    .ok_or_else(|| de::Error::missing_field("exchange"))
                    .map(Deserialize::deserialize)?
                    .map_err(de::Error::custom)?,
            }
        } else {
            Self::Disabled
        })
    }
}

impl Serialize for Twitter {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        #[serde(untagged)]
        enum Body {
            Disabled,
            Enabled { token: String },
        }
        #[derive(Serialize)]
        struct Tagged {
            enabled: bool,
            #[serde(flatten)]
            body: Body,
        }
        match self {
            Twitter::Disabled => Tagged {
                enabled: false,
                body: Body::Disabled,
            },
            Twitter::Enabled { token } => Tagged {
                enabled: true,
                body: Body::Enabled {
                    token: token.clone(),
                },
            },
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Twitter {
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
                token: value
                    .get("token")
                    .ok_or_else(|| de::Error::missing_field("token"))
                    .map(Deserialize::deserialize)?
                    .map_err(de::Error::custom)?,
            }
        } else {
            Self::Disabled
        })
    }
}
