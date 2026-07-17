use std::{env, net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result, bail};

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub bind: SocketAddr,
    pub public_url: String,
    pub setup_token: Option<String>,
    pub storage: StorageConfig,
}

#[derive(Clone, Debug)]
pub enum StorageConfig {
    Local {
        path: PathBuf,
    },
    S3 {
        provider: String,
        bucket: String,
        region: String,
        endpoint: Option<String>,
        access_key: String,
        secret_key: String,
        virtual_hosted_style: bool,
    },
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let database_url = required("OPENHUG_DATABASE_URL")?;
        let bind = env::var("OPENHUG_BIND")
            .unwrap_or_else(|_| "127.0.0.1:3000".into())
            .parse()
            .context("OPENHUG_BIND must be a socket address such as 127.0.0.1:3000")?;
        let setup_token = env::var("OPENHUG_SETUP_TOKEN")
            .ok()
            .filter(|v| !v.is_empty());
        if let Some(token) = &setup_token
            && token.len() < 32
        {
            bail!("OPENHUG_SETUP_TOKEN must be at least 32 characters");
        }
        let public_url = env::var("OPENHUG_PUBLIC_URL")
            .unwrap_or_else(|_| "http://localhost:3000".into())
            .trim_end_matches('/')
            .to_string();

        let driver = env::var("OPENHUG_STORAGE_DRIVER")
            .unwrap_or_else(|_| "local".into())
            .to_lowercase();
        let storage = match driver.as_str() {
            "local" => StorageConfig::Local {
                path: env::var("OPENHUG_STORAGE_LOCAL_PATH")
                    .unwrap_or_else(|_| "./data".into())
                    .into(),
            },
            "s3" | "minio" | "digitalocean" | "hetzner" => StorageConfig::S3 {
                provider: driver,
                bucket: required("OPENHUG_STORAGE_BUCKET")?,
                region: required("OPENHUG_STORAGE_REGION")?,
                endpoint: env::var("OPENHUG_STORAGE_ENDPOINT").ok(),
                access_key: required("OPENHUG_STORAGE_ACCESS_KEY")?,
                secret_key: required("OPENHUG_STORAGE_SECRET_KEY")?,
                virtual_hosted_style: env::var("OPENHUG_STORAGE_VIRTUAL_HOSTED_STYLE")
                    .map(|v| v == "true" || v == "1")
                    .unwrap_or(false),
            },
            other => bail!("unsupported OPENHUG_STORAGE_DRIVER: {other}"),
        };

        Ok(Self {
            database_url,
            bind,
            public_url,
            setup_token,
            storage,
        })
    }

    pub fn storage_label(&self) -> &str {
        match &self.storage {
            StorageConfig::Local { .. } => "local",
            StorageConfig::S3 { provider, .. } => provider,
        }
    }
}

fn required(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("{name} is required"))
}
