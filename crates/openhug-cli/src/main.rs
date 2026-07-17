use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

const CONFIG_VERSION: u32 = 1;
const DEFAULT_SERVER_NAME: &str = "default";
const LOCK_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Parser)]
#[command(
    name = "openhug",
    version,
    about = "Upload and download models and datasets from OpenHug"
)]
struct Cli {
    #[command(flatten)]
    connection: ConnectionOptions,
    #[arg(long, help = "Allow sending bearer tokens to non-loopback HTTP URLs")]
    allow_insecure_http: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Args, Clone, Default)]
struct ConnectionOptions {
    #[arg(
        long,
        value_name = "name",
        conflicts_with = "server_url",
        help = "Select a saved server by name"
    )]
    server: Option<String>,
    #[arg(
        long,
        value_name = "url",
        help = "Use an unsaved server URL for this invocation"
    )]
    server_url: Option<String>,
    #[arg(
        long,
        value_name = "token",
        help = "Use an unsaved bearer token for this invocation"
    )]
    token: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Deprecated alias for `openhug server login <name>`")]
    Login {
        #[arg(long)]
        token: String,
    },
    #[command(about = "Deprecated alias for `openhug server logout <name>`")]
    Logout,
    Whoami,
    Server {
        #[command(subcommand)]
        command: ServerCommand,
    },
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
enum ServerCommand {
    Add {
        name: String,
        url: String,
        #[arg(long)]
        token: String,
        #[arg(long)]
        default: bool,
    },
    List,
    Default {
        name: String,
    },
    Remove {
        name: String,
    },
    Rename {
        old_name: String,
        new_name: String,
    },
    Login {
        name: String,
        #[arg(long)]
        token: String,
    },
    Logout {
        name: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ServerProfile {
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CliConfig {
    version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_server: Option<String>,
    servers: BTreeMap<String, ServerProfile>,
}
impl Default for CliConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            default_server: None,
            servers: BTreeMap::new(),
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

#[derive(Clone)]
struct ResolvedConnection {
    url: String,
    token: Option<String>,
    server_name: Option<String>,
    source: ConnectionSource,
}

#[derive(Clone)]
enum ConnectionSource {
    Temporary,
    Named,
    Environment,
    Default,
}
impl ConnectionSource {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Temporary => "temporary",
            Self::Named => "named",
            Self::Environment => "environment",
            Self::Default => "default",
        }
    }
}

struct Api {
    client: Client,
    connection: ResolvedConnection,
}
impl Api {
    fn new(connection: ResolvedConnection) -> Self {
        Self {
            client: Client::new(),
            connection,
        }
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        debug_assert!(!self.connection.source.as_str().is_empty());
        let request = self.client.request(
            method,
            format!("{}{}", self.connection.url.trim_end_matches('/'), path),
        );
        if let Some(token) = &self.connection.token {
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
            if status == reqwest::StatusCode::UNAUTHORIZED
                && self.connection.token.is_none()
                && let Some(name) = &self.connection.server_name
            {
                bail!(
                    "server returned {status}: {body}\nNo token is saved for server `{name}`. Run `openhug server login {name} --token <token>`."
                );
            }
            bail!("server returned {status}: {body}")
        }
        Ok(response)
    }
}

struct ConfigStore {
    config_path: PathBuf,
    legacy_token_path: PathBuf,
}

impl ConfigStore {
    fn discover() -> Result<Self> {
        let config_root = if let Some(value) = std::env::var_os("XDG_CONFIG_HOME") {
            PathBuf::from(value)
        } else {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".config"))
                .context(
                    "HOME is not set; set XDG_CONFIG_HOME or pass temporary connection flags",
                )?
        };
        let dir = config_root.join("openhug");
        Ok(Self {
            config_path: dir.join("config.json"),
            legacy_token_path: dir.join("token"),
        })
    }

    fn load(&self) -> Result<CliConfig> {
        self.load_inner()
    }

    fn load_inner(&self) -> Result<CliConfig> {
        if path_exists_rejecting_symlink(&self.config_path)? {
            if path_exists_rejecting_symlink(&self.legacy_token_path)? {
                eprintln!(
                    "Warning: found legacy token file at {}; using {} instead.",
                    self.legacy_token_path.display(),
                    self.config_path.display()
                );
            }
            return self.read_existing();
        }
        if path_exists_rejecting_symlink(&self.legacy_token_path)? {
            return self.migrate_legacy_token();
        }
        Ok(CliConfig::default())
    }

    fn read_existing(&self) -> Result<CliConfig> {
        reject_symlink(&self.config_path)?;
        let data = fs::read_to_string(&self.config_path)
            .with_context(|| format!("read {}", self.config_path.display()))?;
        let config: CliConfig = serde_json::from_str(&data)
            .with_context(|| format!("parse {}", self.config_path.display()))?;
        validate_config(&config)?;
        Ok(config)
    }

    fn mutate<F>(&self, f: F) -> Result<CliConfig>
    where
        F: FnOnce(&mut CliConfig) -> Result<()>,
    {
        let _lock = self.lock()?;
        let mut config = self.load_for_mutation()?;
        f(&mut config)?;
        validate_config(&config)?;
        self.write(&config)?;
        Ok(config)
    }

    fn load_for_mutation(&self) -> Result<CliConfig> {
        if path_exists_rejecting_symlink(&self.config_path)? {
            return self.read_existing();
        }
        if path_exists_rejecting_symlink(&self.legacy_token_path)? {
            return self.migrate_legacy_token_locked();
        }
        Ok(CliConfig::default())
    }

    fn migrate_legacy_token(&self) -> Result<CliConfig> {
        let _lock = self.lock()?;
        if path_exists_rejecting_symlink(&self.config_path)? {
            return self.read_existing();
        }
        self.migrate_legacy_token_locked()
    }

    fn migrate_legacy_token_locked(&self) -> Result<CliConfig> {
        reject_symlink(&self.legacy_token_path)?;
        let token = fs::read_to_string(&self.legacy_token_path)
            .with_context(|| format!("read {}", self.legacy_token_path.display()))?
            .trim()
            .to_string();
        if token.is_empty() {
            bail!("legacy token file is empty; remove it or run `openhug server add`");
        }
        let url = match std::env::var("OPENHUG_URL") {
            Ok(value) if !value.trim().is_empty() => normalize_url(&value)?,
            _ => "http://localhost:3000".to_string(),
        };
        let mut config = CliConfig {
            default_server: Some(DEFAULT_SERVER_NAME.to_string()),
            ..CliConfig::default()
        };
        config.servers.insert(
            DEFAULT_SERVER_NAME.to_string(),
            ServerProfile {
                url,
                token: Some(token),
                username: None,
            },
        );
        self.write(&config)?;
        fs::remove_file(&self.legacy_token_path)
            .with_context(|| format!("remove {}", self.legacy_token_path.display()))?;
        eprintln!(
            "Migrated legacy CLI token to {} as server `{}`.",
            self.config_path.display(),
            DEFAULT_SERVER_NAME
        );
        Ok(config)
    }

    fn write(&self, config: &CliConfig) -> Result<()> {
        let parent = self
            .config_path
            .parent()
            .context("config path has no parent directory")?;
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        let _ = path_exists_rejecting_symlink(&self.config_path)?;
        let tmp = parent.join(format!(".config.json.tmp.{}", std::process::id()));
        let data = serde_json::to_vec_pretty(config).context("serialize CLI config")?;
        let write_result = (|| -> Result<()> {
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                let mut file = fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .mode(0o600)
                    .open(&tmp)
                    .with_context(|| format!("create {}", tmp.display()))?;
                file.write_all(&data)?;
                file.write_all(b"\n")?;
                file.sync_all()?;
            }
            #[cfg(not(unix))]
            {
                let mut file = fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&tmp)
                    .with_context(|| format!("create {}", tmp.display()))?;
                file.write_all(&data)?;
                file.write_all(b"\n")?;
                file.sync_all()?;
            }
            fs::rename(&tmp, &self.config_path).with_context(|| {
                format!("rename {} to {}", tmp.display(), self.config_path.display())
            })?;
            #[cfg(unix)]
            fs::set_permissions(
                &self.config_path,
                std::os::unix::fs::PermissionsExt::from_mode(0o600),
            )?;
            sync_dir(parent);
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        write_result
    }

    fn lock(&self) -> Result<LockGuard> {
        let parent = self
            .config_path
            .parent()
            .context("config path has no parent directory")?;
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        let path = parent.join("config.lock");
        let started = Instant::now();
        loop {
            let result = {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    fs::OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .mode(0o600)
                        .open(&path)
                }
                #[cfg(not(unix))]
                {
                    fs::OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(&path)
                }
            };
            match result {
                Ok(mut file) => {
                    let _ = writeln!(file, "{}", std::process::id());
                    let _ = file.sync_all();
                    return Ok(LockGuard { path });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if started.elapsed() >= LOCK_TIMEOUT {
                        bail!(
                            "timed out waiting for CLI config lock at {}; retry the command",
                            path.display()
                        );
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(error) => {
                    return Err(error).with_context(|| format!("lock {}", path.display()));
                }
            }
        }
    }
}

struct LockGuard {
    path: PathBuf,
}
impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let store = ConfigStore::discover()?;

    match cli.command {
        Command::Server { command } => {
            handle_server_command(&store, command, cli.allow_insecure_http).await?;
            return Ok(());
        }
        Command::Login { token } => {
            handle_legacy_login(&store, &cli.connection, &token, cli.allow_insecure_http).await?;
            return Ok(());
        }
        Command::Logout => {
            handle_legacy_logout(&store, &cli.connection)?;
            return Ok(());
        }
        command => {
            let config = store.load()?;
            let connection = resolve_connection(&config, &cli.connection)?;
            if let Some(token) = connection.token.as_deref() {
                ensure_nonempty_token(token)?;
            }
            enforce_secure_token_transport(
                &connection.url,
                connection.token.as_deref(),
                cli.allow_insecure_http,
            )?;
            let api = Api::new(connection);
            run_api_command(api, command).await?;
        }
    }
    Ok(())
}

async fn run_api_command(api: Api, command: Command) -> Result<()> {
    match command {
        Command::Login { .. } | Command::Logout | Command::Server { .. } => unreachable!(),
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

async fn handle_server_command(
    store: &ConfigStore,
    command: ServerCommand,
    allow_insecure_http: bool,
) -> Result<()> {
    match command {
        ServerCommand::Add {
            name,
            url,
            token,
            default,
        } => {
            let name = normalize_name(&name)?;
            let url = normalize_url(&url)?;
            ensure_nonempty_token(&token)?;
            enforce_secure_token_transport(&url, Some(&token), allow_insecure_http)?;
            let username = verify_token(&url, &token).await?;
            store.mutate(|config| {
                if config.servers.contains_key(&name) {
                    bail!("server `{name}` already exists");
                }
                let make_default = default || config.servers.is_empty();
                config.servers.insert(
                    name.clone(),
                    ServerProfile {
                        url: url.clone(),
                        token: Some(token.clone()),
                        username: Some(username.clone()),
                    },
                );
                if make_default {
                    config.default_server = Some(name.clone());
                }
                Ok(())
            })?;
            println!("Server `{name}` added.");
        }
        ServerCommand::List => {
            let config = store.load()?;
            if config.servers.is_empty() {
                println!("No OpenHug servers are configured.");
                println!("Example: openhug server add home https://hub.example.com --token oh_...");
                return Ok(());
            }
            println!("DEFAULT  NAME  URL  USER");
            for (name, profile) in &config.servers {
                let marker = if config.default_server.as_deref() == Some(name.as_str()) {
                    "*"
                } else {
                    " "
                };
                let user = profile.username.as_deref().unwrap_or("-");
                println!("{marker:7}  {name}  {}  {user}", profile.url);
            }
        }
        ServerCommand::Default { name } => {
            let name = normalize_name(&name)?;
            store.mutate(|config| {
                if !config.servers.contains_key(&name) {
                    bail!("unknown server `{name}`");
                }
                config.default_server = Some(name.clone());
                Ok(())
            })?;
            println!("Default server set to `{name}`.");
        }
        ServerCommand::Remove { name } => {
            let name = normalize_name(&name)?;
            store.mutate(|config| {
                if config.servers.remove(&name).is_none() {
                    bail!("unknown server `{name}`");
                }
                if config.default_server.as_deref() == Some(name.as_str()) {
                    config.default_server = config.servers.keys().next().cloned();
                }
                Ok(())
            })?;
            println!("Server `{name}` removed.");
        }
        ServerCommand::Rename { old_name, new_name } => {
            let old_name = normalize_name(&old_name)?;
            let new_name = normalize_name(&new_name)?;
            store.mutate(|config| {
                if config.servers.contains_key(&new_name) {
                    bail!("server `{new_name}` already exists");
                }
                let profile = config
                    .servers
                    .remove(&old_name)
                    .ok_or_else(|| anyhow::anyhow!("unknown server `{old_name}`"))?;
                config.servers.insert(new_name.clone(), profile);
                if config.default_server.as_deref() == Some(old_name.as_str()) {
                    config.default_server = Some(new_name.clone());
                }
                Ok(())
            })?;
            println!("Server `{old_name}` renamed to `{new_name}`.");
        }
        ServerCommand::Login { name, token } => {
            let name = normalize_name(&name)?;
            ensure_nonempty_token(&token)?;
            let config = store.load()?;
            let url = config
                .servers
                .get(&name)
                .ok_or_else(|| anyhow::anyhow!("unknown server `{name}`"))?
                .url
                .clone();
            enforce_secure_token_transport(&url, Some(&token), allow_insecure_http)?;
            let username = verify_token(&url, &token).await?;
            store.mutate(|config| {
                let profile = config
                    .servers
                    .get_mut(&name)
                    .ok_or_else(|| anyhow::anyhow!("unknown server `{name}`"))?;
                profile.token = Some(token.clone());
                profile.username = Some(username.clone());
                Ok(())
            })?;
            println!("Token verified and saved for `{name}`.");
        }
        ServerCommand::Logout { name } => {
            let name = normalize_name(&name)?;
            store.mutate(|config| {
                let profile = config
                    .servers
                    .get_mut(&name)
                    .ok_or_else(|| anyhow::anyhow!("unknown server `{name}`"))?;
                profile.token = None;
                Ok(())
            })?;
            println!("Local token removed for `{name}`.");
        }
    }
    Ok(())
}

async fn handle_legacy_login(
    store: &ConfigStore,
    options: &ConnectionOptions,
    token: &str,
    allow_insecure_http: bool,
) -> Result<()> {
    ensure_nonempty_token(token)?;
    let config = store.load()?;
    let (name, url, create) = if let Some(server) = &options.server {
        let name = normalize_name(server)?;
        let profile = config
            .servers
            .get(&name)
            .ok_or_else(|| anyhow::anyhow!("unknown server `{name}`"))?;
        (name, profile.url.clone(), false)
    } else if let Some(default) = &config.default_server {
        let profile = config
            .servers
            .get(default)
            .ok_or_else(|| anyhow::anyhow!("default server `{default}` is missing"))?;
        (default.clone(), profile.url.clone(), false)
    } else if let Some(url) = &options.server_url {
        (DEFAULT_SERVER_NAME.to_string(), normalize_url(url)?, true)
    } else {
        bail!(
            "no default server is configured; run `openhug server add <name> <url> --token <token>` or `openhug --server-url <url> login --token <token>`"
        );
    };
    enforce_secure_token_transport(&url, Some(token), allow_insecure_http)?;
    let username = verify_token(&url, token).await?;
    store.mutate(|config| {
        if create {
            if config.servers.contains_key(&name) {
                bail!("server `{name}` already exists");
            }
            config.servers.insert(
                name.clone(),
                ServerProfile {
                    url: url.clone(),
                    token: Some(token.to_string()),
                    username: Some(username.clone()),
                },
            );
            config.default_server = Some(name.clone());
        } else {
            let profile = config
                .servers
                .get_mut(&name)
                .ok_or_else(|| anyhow::anyhow!("unknown server `{name}`"))?;
            profile.token = Some(token.to_string());
            profile.username = Some(username.clone());
        }
        Ok(())
    })?;
    eprintln!(
        "Warning: `openhug login` is deprecated; use `openhug server login {name} --token <token>`."
    );
    println!("Token verified and saved for `{name}`.");
    Ok(())
}

fn handle_legacy_logout(store: &ConfigStore, options: &ConnectionOptions) -> Result<()> {
    let config = store.load()?;
    let name = if let Some(server) = &options.server {
        normalize_name(server)?
    } else {
        config
            .default_server
            .clone()
            .context("no default server is configured; run `openhug server list`")?
    };
    store.mutate(|config| {
        let profile = config
            .servers
            .get_mut(&name)
            .ok_or_else(|| anyhow::anyhow!("unknown server `{name}`"))?;
        profile.token = None;
        Ok(())
    })?;
    eprintln!("Warning: `openhug logout` is deprecated; use `openhug server logout {name}`.");
    println!("Local token removed for `{name}`.");
    Ok(())
}

async fn verify_token(url: &str, token: &str) -> Result<String> {
    let connection = ResolvedConnection {
        url: url.to_string(),
        token: Some(token.to_string()),
        server_name: None,
        source: ConnectionSource::Temporary,
    };
    let api = Api::new(connection);
    let me: UserInfo = api
        .checked(api.request(reqwest::Method::GET, "/api/v1/auth/me"))
        .await
        .map_err(|error| anyhow::anyhow!("token verification failed: {error}"))?
        .json()
        .await
        .context("parse token verification response")?;
    Ok(me.username)
}

fn resolve_connection(
    config: &CliConfig,
    options: &ConnectionOptions,
) -> Result<ResolvedConnection> {
    if let Some(token) = options.token.as_deref() {
        ensure_nonempty_token(token)?;
    }

    if let Some(url) = &options.server_url {
        let token = options
            .token
            .clone()
            .or_else(|| nonempty_env("OPENHUG_TOKEN"));
        return Ok(ResolvedConnection {
            url: normalize_url(url)?,
            token,
            server_name: None,
            source: ConnectionSource::Temporary,
        });
    }

    if let Some(name) = &options.server {
        let name = normalize_name(name)?;
        let profile = config
            .servers
            .get(&name)
            .ok_or_else(|| anyhow::anyhow!("unknown server `{name}`"))?;
        return Ok(ResolvedConnection {
            url: profile.url.clone(),
            token: options
                .token
                .clone()
                .or_else(|| profile.token.clone())
                .or_else(|| nonempty_env("OPENHUG_TOKEN")),
            server_name: Some(name),
            source: ConnectionSource::Named,
        });
    }

    let default = default_profile(config)?;
    let env_url = nonempty_env("OPENHUG_URL");
    let env_token = nonempty_env("OPENHUG_TOKEN");
    if env_url.is_some() || env_token.is_some() {
        let url = match env_url {
            Some(url) => normalize_url(&url)?,
            None => default
                .as_ref()
                .map(|(_, profile)| profile.url.clone())
                .context("OPENHUG_TOKEN is set but no URL is configured; set OPENHUG_URL or add a default server")?,
        };
        let token = options.token.clone().or(env_token).or_else(|| {
            default
                .as_ref()
                .and_then(|(_, profile)| profile.token.clone())
        });
        return Ok(ResolvedConnection {
            url,
            token,
            server_name: default.as_ref().map(|(name, _)| name.clone()),
            source: ConnectionSource::Environment,
        });
    }

    if let Some((name, profile)) = default {
        return Ok(ResolvedConnection {
            url: profile.url.clone(),
            token: options.token.clone().or_else(|| profile.token.clone()),
            server_name: Some(name),
            source: ConnectionSource::Default,
        });
    }

    bail!(
        "no OpenHug server is configured; run `openhug server add <name> <url> --token <token>` or pass `--server-url <url>`"
    )
}

fn default_profile(config: &CliConfig) -> Result<Option<(String, ServerProfile)>> {
    match &config.default_server {
        Some(name) => {
            let profile = config
                .servers
                .get(name)
                .ok_or_else(|| anyhow::anyhow!("default server `{name}` is missing"))?;
            Ok(Some((name.clone(), profile.clone())))
        }
        None => Ok(None),
    }
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

fn normalize_name(value: &str) -> Result<String> {
    let name = value.to_ascii_lowercase();
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > 32 {
        bail!("server name must be 1-32 lowercase letters, numbers, or hyphens")
    }
    if !bytes[0].is_ascii_lowercase() && !bytes[0].is_ascii_digit() {
        bail!("server name must start with a lowercase letter or number")
    }
    if !bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
    {
        bail!("server name must match ^[a-z0-9][a-z0-9-]{{0,31}}$")
    }
    Ok(name)
}

fn normalize_url(value: &str) -> Result<String> {
    let trimmed = value.trim();
    let url =
        reqwest::Url::parse(trimmed).with_context(|| format!("invalid server URL `{trimmed}`"))?;
    if !matches!(url.scheme(), "http" | "https") {
        bail!("server URL must use http:// or https://")
    }
    if url.host_str().is_none() {
        bail!("server URL must include a host")
    }
    if url.query().is_some() || url.fragment().is_some() {
        bail!("server URL must not include a query string or fragment")
    }
    let mut normalized = url.to_string();
    while normalized.ends_with('/') {
        normalized.pop();
    }
    Ok(normalized)
}

fn ensure_nonempty_token(token: &str) -> Result<()> {
    if token.trim().is_empty() {
        bail!("token must not be empty")
    }
    Ok(())
}

fn validate_config(config: &CliConfig) -> Result<()> {
    if config.version > CONFIG_VERSION {
        bail!(
            "CLI config version {} is newer than this OpenHug CLI supports; upgrade the CLI",
            config.version
        );
    }
    if config.version != CONFIG_VERSION {
        bail!(
            "unsupported CLI config version {}; expected {CONFIG_VERSION}",
            config.version
        );
    }
    if let Some(default_name) = &config.default_server {
        let normalized_default = normalize_name(default_name)?;
        if &normalized_default != default_name {
            bail!("default server name must be normalized")
        }
        if !config.servers.contains_key(default_name.as_str()) {
            bail!("default server `{default_name}` is missing from the server registry")
        }
    }
    for (name, profile) in &config.servers {
        let normalized = normalize_name(name)?;
        if &normalized != name {
            bail!("server name `{name}` must be normalized")
        }
        let normalized_url = normalize_url(&profile.url)?;
        if normalized_url != profile.url {
            bail!("server `{name}` URL must be normalized")
        }
        if let Some(token) = &profile.token {
            ensure_nonempty_token(token)?;
        }
    }
    Ok(())
}

fn reject_symlink(path: &Path) -> Result<()> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("inspect {}", path.display()))?;
    if metadata.file_type().is_symlink() {
        bail!(
            "refusing to use symbolic-link config file at {}",
            path.display()
        );
    }
    Ok(())
}

fn path_exists_rejecting_symlink(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                bail!(
                    "refusing to use symbolic-link config file at {}",
                    path.display()
                );
            }
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("inspect {}", path.display())),
    }
}

fn nonempty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

#[cfg(unix)]
fn sync_dir(path: &Path) {
    let _ = fs::File::open(path).and_then(|file| file.sync_all());
}

#[cfg(not(unix))]
fn sync_dir(_path: &Path) {}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> CliConfig {
        let mut servers = BTreeMap::new();
        servers.insert(
            "home".to_string(),
            ServerProfile {
                url: "https://hub.example.com".to_string(),
                token: Some("oh_home".to_string()),
                username: Some("alice".to_string()),
            },
        );
        CliConfig {
            version: CONFIG_VERSION,
            default_server: Some("home".to_string()),
            servers,
        }
    }

    #[test]
    fn validates_and_normalizes_server_names() {
        assert_eq!(normalize_name("Home-1").unwrap(), "home-1");
        assert!(normalize_name("-home").is_err());
        assert!(normalize_name("home_1").is_err());
        assert!(normalize_name("a23456789012345678901234567890123").is_err());
    }

    #[test]
    fn validates_and_normalizes_urls() {
        assert_eq!(
            normalize_url("https://Hub.Example.com///").unwrap(),
            "https://hub.example.com"
        );
        assert_eq!(
            normalize_url("http://localhost:3000/api/").unwrap(),
            "http://localhost:3000/api"
        );
        assert!(normalize_url("ftp://hub.example.com").is_err());
        assert!(normalize_url("/relative").is_err());
        assert!(normalize_url("https://hub.example.com?token=secret").is_err());
    }

    #[test]
    fn resolves_default_and_cli_overrides_without_mutating_config() {
        let config = sample_config();
        let resolved = resolve_connection(&config, &ConnectionOptions::default()).unwrap();
        assert_eq!(resolved.url, "https://hub.example.com");
        assert_eq!(resolved.token.as_deref(), Some("oh_home"));
        assert_eq!(resolved.server_name.as_deref(), Some("home"));

        let resolved = resolve_connection(
            &config,
            &ConnectionOptions {
                token: Some("oh_temp".to_string()),
                ..ConnectionOptions::default()
            },
        )
        .unwrap();
        assert_eq!(resolved.token.as_deref(), Some("oh_temp"));
        assert_eq!(config.servers["home"].token.as_deref(), Some("oh_home"));
    }

    #[test]
    fn rejects_missing_default_and_unknown_named_server() {
        let mut config = sample_config();
        config.default_server = None;
        assert!(resolve_connection(&config, &ConnectionOptions::default()).is_err());
        assert!(
            resolve_connection(
                &config,
                &ConnectionOptions {
                    server: Some("work".to_string()),
                    ..ConnectionOptions::default()
                },
            )
            .is_err()
        );
    }

    #[test]
    fn serializes_optional_token_and_username_roundtrip() {
        let config = sample_config();
        let data = serde_json::to_string(&config).unwrap();
        assert!(data.contains("oh_home"));
        let decoded: CliConfig = serde_json::from_str(&data).unwrap();
        validate_config(&decoded).unwrap();
        assert_eq!(decoded.servers["home"].username.as_deref(), Some("alice"));
    }

    #[test]
    fn config_store_writes_and_reads_secure_json() {
        let dir = std::env::temp_dir().join(format!(
            "openhug-cli-test-{}-{}",
            std::process::id(),
            unique_suffix()
        ));
        let store = ConfigStore {
            config_path: dir.join("openhug/config.json"),
            legacy_token_path: dir.join("openhug/token"),
        };
        let config = sample_config();
        store.write(&config).unwrap();
        let loaded = store.read_existing().unwrap();
        assert_eq!(loaded.default_server.as_deref(), Some("home"));
        assert_eq!(loaded.servers["home"].url, "https://hub.example.com");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&store.config_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_symlink_config_files() {
        let dir = std::env::temp_dir().join(format!(
            "openhug-cli-symlink-test-{}-{}",
            std::process::id(),
            unique_suffix()
        ));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("target.json");
        fs::write(&target, "{}").unwrap();
        let link = dir.join("config.json");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target, &link).unwrap();
            assert!(reject_symlink(&link).is_err());
        }
        let _ = fs::remove_dir_all(dir);
    }

    fn unique_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }
}
