use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use figment::providers::{Format, Json, Toml};
use figment::value::{Dict, Map};
use figment::{Error, Metadata, Profile, Provider};

pub struct ConfigFile {
    inner: Option<Box<dyn Provider>>,
}

impl ConfigFile {
    pub fn file(path: impl AsRef<Path>) -> Self {
        fn try_parse<T: 'static + Format>(path: &Path) -> Option<Box<dyn Provider>> {
            let provider = T::file(path);
            if provider.metadata().source.is_some() {
                Some(Box::new(provider))
            } else {
                None
            }
        }
        fn append_ext(path: &Path, ext: impl AsRef<OsStr>) -> PathBuf {
            let mut path = path.to_path_buf().into_os_string();
            path.push(".");
            path.push(ext);
            path.into()
        }

        let path = path.as_ref();
        let ext = path.extension().map(OsStr::to_ascii_lowercase);

        Self {
            inner: ext
                .and_then(|ext| match ext.to_str().unwrap() {
                    "toml" => try_parse::<Toml>(path),
                    "json" => try_parse::<Json>(path),
                    _ => None,
                })
                .or_else(|| {
                    if path.is_dir() {
                        None
                    } else {
                        try_parse::<Toml>(&*append_ext(path, "toml"))
                            .or_else(|| try_parse::<Json>(&*append_ext(path, "json")))
                    }
                }),
        }
    }
}

impl Provider for ConfigFile {
    fn metadata(&self) -> Metadata {
        self.inner
            .as_ref()
            .map_or_else(|| Metadata::named("unsupported"), |inner| inner.metadata())
    }

    fn data(&self) -> Result<Map<Profile, Dict>, Error> {
        self.inner.as_ref().map_or_else(
            || Err(Error::from(String::from("unsupported format"))),
            |inner| inner.data(),
        )
    }
}
