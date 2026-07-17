use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

#[derive(Parser)]
#[command(
    name = "openhug",
    version,
    about = "Upload and download models and datasets from OpenHug"
)]
struct Cli {
    #[arg(long, env = "OPENHUG_URL", default_value = "http://localhost:3000")]
    server: String,
    #[arg(long, env = "OPENHUG_TOKEN")]
    token: Option<String>,
    #[arg(long, help = "Allow sending bearer tokens to non-loopback HTTP URLs")]
    allow_insecure_http: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Login {
        #[arg(long)]
        token: String,
    },
    Logout,
    Whoami,
    Repo {
        #[command(subcommand)]
        command: RepoCommand,
    },
    Upload {
        repository: String,
        source: PathBuf,
        #[arg(long)]
        path: Option<String>,
        #[arg(long, default_value = "Upload with OpenHug CLI")]
        message: String,
        #[arg(long, value_enum, default_value_t=Kind::Model)]
        kind: Kind,
        #[arg(long)]
        exclude: Vec<String>,
    },
    Download {
        repository: String,
        path: String,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long, default_value = "main")]
        revision: String,
        #[arg(long,value_enum,default_value_t=Kind::Model)]
        kind: Kind,
    },
}

#[derive(Subcommand)]
enum RepoCommand {
    List {
        #[arg(long, value_enum)]
        kind: Option<Kind>,
        #[arg(long)]
        search: Option<String>,
    },
    Create {
        repository: String,
        #[arg(long,value_enum,default_value_t=Kind::Model)]
        kind: Kind,
        #[arg(long)]
        visibility: Option<String>,
        #[arg(long, default_value = "")]
        description: String,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Kind {
    Model,
    Dataset,
}
impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Dataset => "dataset",
        }
    }
}

#[derive(Deserialize)]
struct BlobReceipt {
    sha256: String,
    size: i64,
}
#[derive(Deserialize)]
struct UserInfo {
    username: String,
}
#[derive(Serialize)]
struct CommitFile {
    path: String,
    sha256: String,
    size: i64,
}
#[derive(Serialize)]
struct CommitRequest {
    message: String,
    files: Vec<CommitFile>,
    deletions: Vec<String>,
}

struct Api {
    client: Client,
    server: String,
    token: Option<String>,
}
impl Api {
    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let request = self.client.request(
            method,
            format!("{}{}", self.server.trim_end_matches('/'), path),
        );
        if let Some(token) = &self.token {
            request.bearer_auth(token)
        } else {
            request
        }
    }
    async fn checked(&self, request: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        let response = request.send().await.context("request failed")?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("server returned {status}: {body}")
        }
        Ok(response)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    enforce_secure_token_transport(&cli.server, cli.token.as_deref(), cli.allow_insecure_http)?;
    if let Command::Login { token } = &cli.command {
        enforce_secure_token_transport(&cli.server, Some(token), cli.allow_insecure_http)?;
        let api = Api {
            client: Client::new(),
            server: cli.server.clone(),
            token: Some(token.clone()),
        };
        api.checked(api.request(reqwest::Method::GET, "/api/v1/auth/me"))
            .await?;
        save_token(token)?;
        println!("Token verified and saved.");
        return Ok(());
    }
    if matches!(&cli.command, Command::Logout) {
        clear_token()?;
        println!("Local token removed.");
        return Ok(());
    }
    let token = cli.token.or_else(load_token);
    enforce_secure_token_transport(&cli.server, token.as_deref(), cli.allow_insecure_http)?;
    let api = Api {
        client: Client::new(),
        server: cli.server,
        token,
    };
    match cli.command {
        Command::Login { .. } | Command::Logout => unreachable!(),
        Command::Whoami => {
            let value: serde_json::Value = api
                .checked(api.request(reqwest::Method::GET, "/api/v1/auth/me"))
                .await?
                .json()
                .await?;
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        Command::Repo {
            command: RepoCommand::List { kind, search },
        } => {
            let mut url = "/api/v1/repositories?limit=100".to_string();
            if let Some(kind) = kind {
                url.push_str(&format!("&kind={}", kind.as_str()));
            }
            if let Some(search) = search {
                url.push_str(&format!("&search={}", urlencoding(&search)));
            }
            let value: serde_json::Value = api
                .checked(api.request(reqwest::Method::GET, &url))
                .await?
                .json()
                .await?;
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        Command::Repo {
            command:
                RepoCommand::Create {
                    repository,
                    kind,
                    visibility,
                    description,
                },
        } => {
            let (owner, name) = parse_repo(&repository)?;
            let me: UserInfo = api
                .checked(api.request(reqwest::Method::GET, "/api/v1/auth/me"))
                .await?
                .json()
                .await?;
            if owner != me.username {
                bail!(
                    "repository owner must match the authenticated user ({})",
                    me.username
                );
            }
            let mut payload =
                serde_json::json!({"kind":kind.as_str(),"name":name,"description":description});
            if let Some(visibility) = visibility {
                payload["visibility"] = serde_json::Value::String(visibility);
            }
            let value: serde_json::Value = api
                .checked(
                    api.request(reqwest::Method::POST, "/api/v1/repositories")
                        .json(&payload),
                )
                .await?
                .json()
                .await?;
            println!(
                "Created {}/{} ({})",
                repository,
                value["id"].as_str().unwrap_or("unknown"),
                kind.as_str()
            );
        }
        Command::Upload {
            repository,
            source,
            path,
            message,
            kind,
            exclude,
        } => upload(&api, &repository, &source, path, message, kind, &exclude).await?,
        Command::Download {
            repository,
            path,
            output,
            revision,
            kind,
        } => download(&api, &repository, &path, output, &revision, kind).await?,
    }
    Ok(())
}

async fn upload(
    api: &Api,
    repository: &str,
    source: &Path,
    remote_path: Option<String>,
    message: String,
    kind: Kind,
    excludes: &[String],
) -> Result<()> {
    let (owner, name) = parse_repo(repository)?;
    let paths: Vec<(PathBuf, String)> = if source.is_file() {
        vec![(
            source.to_path_buf(),
            remote_path
                .unwrap_or_else(|| source.file_name().unwrap().to_string_lossy().into_owned()),
        )]
    } else if source.is_dir() {
        let mut collected = Vec::new();
        for entry in WalkDir::new(source) {
            let entry = entry.with_context(|| format!("walk {}", source.display()))?;
            if !entry.file_type().is_file() {
                continue;
            }
            let relative = entry
                .path()
                .strip_prefix(source)
                .with_context(|| format!("strip prefix for {}", entry.path().display()))?
                .to_string_lossy()
                .replace('\\', "/");
            if !excludes.iter().any(|x| pattern_matches(x, &relative)) {
                collected.push((entry.into_path(), relative));
            }
        }
        collected
    } else {
        bail!("source does not exist: {}", source.display())
    };
    if paths.is_empty() {
        bail!("no files matched the upload")
    }
    let mut files = Vec::with_capacity(paths.len());
    for (index, (local, remote)) in paths.iter().enumerate() {
        let bytes = fs::read(local).with_context(|| format!("read {}", local.display()))?;
        let expected = hex::encode(Sha256::digest(&bytes));
        eprintln!("[{}/{}] {}", index + 1, paths.len(), remote);
        let receipt: BlobReceipt = api
            .checked(
                api.request(reqwest::Method::POST, "/api/v1/blobs")
                    .body(bytes),
            )
            .await?
            .json()
            .await?;
        if receipt.sha256 != expected {
            bail!("checksum mismatch for {remote}")
        }
        files.push(CommitFile {
            path: remote.clone(),
            sha256: receipt.sha256,
            size: receipt.size,
        });
    }
    let payload = CommitRequest {
        message,
        files,
        deletions: vec![],
    };
    let url = format!(
        "/api/v1/repositories/{}/{}/{}/commits",
        kind.as_str(),
        owner,
        name
    );
    let value: serde_json::Value = api
        .checked(api.request(reqwest::Method::POST, &url).json(&payload))
        .await?
        .json()
        .await?;
    println!(
        "Committed {} files as {}",
        paths.len(),
        value["commit"].as_str().unwrap_or("unknown")
    );
    Ok(())
}

async fn download(
    api: &Api,
    repository: &str,
    path: &str,
    output: Option<PathBuf>,
    revision: &str,
    kind: Kind,
) -> Result<()> {
    let (owner, name) = parse_repo(repository)?;
    let url = format!(
        "/api/v1/repositories/{}/{}/{}/resolve/{}/{}",
        kind.as_str(),
        owner,
        name,
        urlencoding(revision),
        urlencode_path(path)
    );
    let response = api.checked(api.request(reqwest::Method::GET, &url)).await?;
    let bytes = response.bytes().await?;
    let output = output.unwrap_or_else(|| PathBuf::from(path.rsplit('/').next().unwrap_or(path)));
    fs::write(&output, &bytes).with_context(|| format!("write {}", output.display()))?;
    println!("Downloaded {} bytes to {}", bytes.len(), output.display());
    Ok(())
}

fn parse_repo(value: &str) -> Result<(&str, &str)> {
    value
        .split_once('/')
        .filter(|(a, b)| !a.is_empty() && !b.is_empty() && !b.contains('/'))
        .ok_or_else(|| anyhow::anyhow!("repository must be OWNER/NAME"))
}
fn pattern_matches(pattern: &str, path: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix("*.") {
        path.ends_with(&format!(".{suffix}"))
    } else {
        path == pattern || path.starts_with(&format!("{}/", pattern.trim_end_matches('/')))
    }
}
fn enforce_secure_token_transport(
    server: &str,
    token: Option<&str>,
    allow_insecure_http: bool,
) -> Result<()> {
    if token.is_none() || allow_insecure_http {
        return Ok(());
    }
    let url = reqwest::Url::parse(server).context("server must be a valid URL")?;
    if url.scheme() != "http" {
        return Ok(());
    }
    let host = url.host_str().unwrap_or("");
    if matches!(host, "localhost" | "127.0.0.1" | "::1") {
        return Ok(());
    }
    bail!(
        "refusing to send a bearer token to non-loopback HTTP; use HTTPS or --allow-insecure-http"
    )
}

fn urlencode_path(value: &str) -> String {
    value
        .split('/')
        .map(urlencoding)
        .collect::<Vec<_>>()
        .join("/")
}

fn urlencoding(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn token_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".config/openhug/token"))
}

fn load_token() -> Option<String> {
    fs::read_to_string(token_path()?)
        .ok()
        .map(|v| v.trim().to_string())
}

fn save_token(token: &str) -> Result<()> {
    let path = token_path().context("HOME is not set; use OPENHUG_TOKEN instead")?;
    let parent = path.parent().expect("token path has a parent");
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(".token.tmp.{}", std::process::id()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&tmp)?;
        file.write_all(token.as_bytes())?;
        file.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&tmp)?;
        file.write_all(token.as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&tmp, &path)?;
    Ok(())
}

fn clear_token() -> Result<()> {
    if let Some(path) = token_path()
        && path.exists()
    {
        fs::remove_file(path)?;
    }
    Ok(())
}
