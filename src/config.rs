use std::{collections::HashMap, path::PathBuf, time::Duration};

use anyhow::Context;
use typenv::{FromEnv, parser::ParseResult};

#[derive(Debug, FromEnv)]
pub struct Config {
    #[env(default = "0.0.0.0:8080")]
    pub bind: String,

    #[env(default_value = 3_000_000)]
    pub max_size: usize,

    #[env(default_value = 8)]
    pub max_concurrency: usize,

    #[env(parser = duration, default_value = Duration::from_secs(10))]
    pub timeout: Duration,

    #[env(default = "vali3/vali-igc.conf")]
    pub vali_config: PathBuf,

    #[env(default = "vali3/cgi-bin")]
    pub vali_modules: PathBuf,

    #[env(default = "wine")]
    pub wine: PathBuf,

    #[env(default = "vali3/cgi-bin/ivshell.exe")]
    pub ivshell: PathBuf,
}

fn duration(s: &str) -> ParseResult<Duration> {
    let secs: u64 = s.parse()?;
    Ok(Duration::from_secs(secs))
}

impl Config {
    pub fn load_modules(&self) -> anyhow::Result<HashMap<String, (String, String)>> {
        let config =
            std::fs::read_to_string(&self.vali_config).context("Failed to read vali config")?;

        let mut modules = HashMap::new();

        for line in config.lines() {
            if let Some(module_line) = line.trim().strip_prefix("Module ") {
                let (ccc, rest) = module_line
                    .trim()
                    .split_once([' ', '\t'])
                    .context("Unexpected format")?;
                let (vali, pattern) = rest
                    .trim()
                    .split_once([' ', '\t'])
                    .context("Unexpected format")?;
                modules.insert(
                    ccc.to_string(),
                    (vali.to_string(), pattern.trim().to_string()),
                );
            }
        }

        Ok(modules)
    }
}
