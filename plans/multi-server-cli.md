# Named Multi-Server CLI Support

## Summary

Replace the CLI's single saved token with a named server registry so one OpenHug CLI installation can authenticate to multiple instances. Users can select stable names such as `home` and `work`, designate one default, or temporarily override both the URL and token without modifying saved configuration.

This change affects only the Rust CLI. Server APIs and the web application remain unchanged.

## Command-Line Interface

Add a `server` command group:

```text
openhug server add <name> <url> --token <token> [--default]
openhug server list
openhug server default <name>
openhug server remove <name>
openhug server rename <old-name> <new-name>
openhug server login <name> --token <token>
openhug server logout <name>
```

- `server add` normalizes the URL by removing trailing slashes, verifies the token with `GET /api/v1/auth/me`, and saves the server only after successful verification. The first saved server automatically becomes the default; `--default` changes the default explicitly.
- `server list` prints the name, normalized URL, authenticated username when known, and a marker beside the default. It never prints tokens. Empty state output includes an example `server add` command.
- `server default` requires an existing named server.
- `server remove` deletes the selected server and credential. Removing the default selects the first remaining server by name; if none remain, the default becomes unset.
- `server rename` preserves the URL, token, and default status. It fails rather than overwriting an existing name.
- `server login` replaces the selected server's token only after verification and records the returned username.
- `server logout` removes the selected token but retains its name and URL. Commands requiring authentication then fail with a targeted login instruction.

Replace the existing global raw-URL `--server` option with these selectors:

```text
--server <name>       Select a saved server by name.
--server-url <url>    Use an unsaved URL for this invocation.
--token <token>       Use an unsaved token for this invocation.
```

Examples:

```sh
openhug server add home http://localhost:3000 --token oh_local
openhug server add work https://hub.example.com --token oh_work
openhug server default home

openhug repo list                         # uses home
openhug --server work repo list           # uses work
openhug --server-url https://temp.example.com --token oh_temp whoami
```

Keep `openhug login --token ...` and `openhug logout` as compatibility aliases operating on the resolved default server. If no saved/default server exists, `login` requires `--server-url`; it creates a server named `default`. Mark these aliases as deprecated in help text and documentation, but do not remove them in this release.

## Configuration and Resolution

Store the registry at `~/.config/openhug/config.json`, with the configuration directory resolved from `XDG_CONFIG_HOME` when set and `HOME/.config` otherwise:

```json
{
  "version": 1,
  "default_server": "home",
  "servers": {
    "home": {
      "url": "http://localhost:3000",
      "token": "oh_...",
      "username": "owen"
    },
    "work": {
      "url": "https://hub.example.com",
      "token": "oh_...",
      "username": "owen-work"
    }
  }
}
```

- Write the file atomically through a temporary file and rename, create parent directories as needed, and enforce mode `0600` on Unix.
- Reject symbolic-link config files and never include tokens in normal output, debug messages, or errors.
- Validate names with `^[a-z0-9][a-z0-9-]{0,31}$`; names are unique and case-sensitive input is normalized to lowercase.
- Accept only absolute `http://` or `https://` URLs with a host. Allow HTTP for local and explicitly configured development instances.
- Treat missing config as an empty registry. Reject unsupported future `version` values with a clear upgrade message instead of silently rewriting them.

Resolve the active connection in this exact precedence order:

1. `--server-url` and `--token` temporary overrides.
2. `--server <name>`, using its saved URL and token unless `--token` overrides the saved token.
3. `OPENHUG_URL` and `OPENHUG_TOKEN` environment variables. When only one is set, combine it with the other value from the selected/default saved server when available.
4. The configured default server.
5. If no URL can be resolved, return an error instructing the user to add a server or pass `--server-url`; do not silently assume localhost.

`--server` and `--server-url` are mutually exclusive. A raw `--token` never updates stored credentials. Server-management commands operate directly on the named registry and do not use the normal active-connection resolver.

## Migration and Internal Design

- On first config access, detect the legacy `~/.config/openhug/token` file. If present and no JSON config exists, create a `default` entry using `OPENHUG_URL` when provided or `http://localhost:3000` otherwise, move the token into the new registry, write the registry securely, and remove the legacy file only after the new file is durable.
- If both legacy and new files exist, use the new registry and leave the legacy file untouched while showing a one-time warning.
- Add serializable `CliConfig` and `ServerProfile` types plus a `ConfigStore` responsible for path discovery, validation, atomic reads/writes, migration, and permission handling.
- Add a `ConnectionOptions` type for Clap's global arguments and a single resolver that returns `ResolvedConnection { url, token, server_name, source }`.
- Refactor `Api` construction to accept only `ResolvedConnection`; command handlers must not read configuration or environment variables independently.
- Preserve authentication verification through `/api/v1/auth/me`. Save the returned username as informational cache data, refreshing it on login; a stale username never blocks requests.
- Update README examples to use named servers and document temporary raw overrides and the migration behavior.

## Errors and Edge Cases

- Reject duplicate names, invalid names, malformed URLs, and empty tokens before making network calls.
- Do not change saved state when token verification fails, the server is unreachable, JSON serialization fails, or the atomic rename fails.
- Distinguish unknown server names, missing default server, missing token, unreachable host, and rejected credentials in user-facing errors.
- `server list` must work with corrupt individual credentials only if the JSON structure remains valid; it performs no network requests.
- Concurrent CLI writes use an advisory lock file in the configuration directory and time out with a retryable error rather than losing updates.
- A selected server without a token may perform public read operations, but authenticated operations receive the existing server authorization error plus a `server login <name>` hint.

## Test Plan

- Unit-test name and URL validation, URL normalization, precedence resolution, default selection, redaction, and serialization round trips.
- Test add, rename, remove, login, logout, and default transitions against temporary configuration directories.
- Test atomic-write failure behavior, Unix `0600` permissions, symlink rejection, concurrent writer locking, unsupported config versions, and malformed JSON.
- Test legacy-token migration, including successful migration, failed writes, simultaneous old/new config files, and `OPENHUG_URL` selection.
- Test precedence combinations for CLI flags, environment variables, named profiles, raw URLs, saved tokens, and missing defaults.
- Add CLI integration tests with a mock HTTP server for successful identity verification, invalid tokens, unavailable servers, and ensuring tokens never appear in stdout or stderr.
- Run `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings` as acceptance checks.

## Acceptance Criteria

- A user can remain logged into `home` and `work`, switch between them by name, and run commands without repeatedly supplying URLs or tokens.
- Commands without a selector consistently use the configured default server.
- Temporary `--server-url` and `--token` values do not mutate saved configuration.
- Existing single-token users migrate without losing their credential.
- Tokens remain permission-restricted at rest and are never printed outside the initial user-provided command input.
