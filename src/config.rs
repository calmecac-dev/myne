// SPDX-License-Identifier: GPL-3.0-or-later
//! User preferences, stored as TOML in `~/.config/myne/config.toml`.

use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::compiler::CompileOptions;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub compile_on_save: bool,
    pub command: String,
    pub tbl: bool,
    pub eqn: bool,
    pub pic: bool,
    pub refer: bool,
    pub show_log_on_error: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            compile_on_save: true,
            command: "pdfmom".into(),
            tbl: false,
            eqn: false,
            pic: false,
            refer: false,
            show_log_on_error: true,
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        glib::user_config_dir().join("myne").join("config.toml")
    }

    /// Loads the configuration. A missing file gives the defaults; an
    /// unreadable one gives the defaults plus the error, to be shown.
    pub fn load() -> (Self, Option<anyhow::Error>) {
        let path = Self::path();
        match fs::read_to_string(&path) {
            Ok(text) => match toml::from_str(&text) {
                Ok(config) => (config, None),
                Err(err) => (
                    Self::default(),
                    Some(
                        anyhow::Error::new(err).context(format!("{} no es válido", path.display())),
                    ),
                ),
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => (Self::default(), None),
            Err(err) => (
                Self::default(),
                Some(
                    anyhow::Error::new(err).context(format!("no se pudo leer {}", path.display())),
                ),
            ),
        }
    }

    /// Writes the configuration atomically (temporary file + rename).
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)
                .with_context(|| format!("no se pudo crear {}", dir.display()))?;
        }
        let text =
            toml::to_string_pretty(self).context("no se pudo serializar la configuración")?;
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, text).with_context(|| format!("no se pudo escribir {}", tmp.display()))?;
        fs::rename(&tmp, &path)
            .with_context(|| format!("no se pudo escribir {}", path.display()))?;
        Ok(())
    }

    pub fn compile_options(&self) -> CompileOptions {
        CompileOptions {
            command: self.command.clone(),
            tbl: self.tbl,
            eqn: self.eqn,
            pic: self.pic,
            refer: self.refer,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_file_uses_defaults() {
        let config: Config = toml::from_str("tbl = true\n").expect("parse");
        assert!(config.tbl);
        assert!(config.compile_on_save);
        assert_eq!(config.command, "pdfmom");
    }

    #[test]
    fn round_trip() {
        let config = Config {
            eqn: true,
            command: "pdfmom -wall".into(),
            ..Config::default()
        };
        let text = toml::to_string_pretty(&config).expect("serialize");
        assert_eq!(toml::from_str::<Config>(&text).expect("parse"), config);
    }
}
