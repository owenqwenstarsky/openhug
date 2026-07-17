# OpenHug

OpenHug is a self-hosted model and dataset hub. It combines a Rust server and CLI,
PostgreSQL metadata, local or S3-compatible blob storage, and an embedded Next.js UI.

## Run it

OpenHug ships as a server binary with the static web application embedded in it. PostgreSQL
and the selected blob store remain external services.

```sh
cp .env.example .env
npm --prefix web install
npm --prefix web run build
cargo build --release -p openhug-server -p openhug-cli
```

Export the values from `.env`, run `target/release/openhug-server`, and open the configured
public URL to complete onboarding. Bind to loopback for local setup, or set a high-entropy
`OPENHUG_SETUP_TOKEN` and enter it during onboarding before exposing first-run setup over a
network.

Infrastructure credentials and connection URLs are read only from environment variables.
They are never accepted by the setup API or written to the database.

## Storage

Set `OPENHUG_STORAGE_DRIVER` to `local`, `s3`, `minio`, `digitalocean`, or `hetzner`.
S3-compatible drivers read their bucket, region, endpoint, and credentials from the
`OPENHUG_STORAGE_*` environment variables shown in `.env.example`. Local storage reads
only `OPENHUG_STORAGE_LOCAL_PATH`.

If PostgreSQL already references blobs, the server verifies the selected backend contains
all referenced blobs and refuses to start after an accidental backend switch.

## CLI

Create a token in **Settings → API tokens**, then authenticate and upload:

```sh
openhug --server https://hub.example.com login --token oh_...
openhug repo create alice/my-model --kind model
openhug upload alice/my-model ./model --message "Initial weights"
openhug download alice/my-model config.json
```

`OPENHUG_URL` and `OPENHUG_TOKEN` can be used instead of command-line configuration.
Saved CLI tokens are written atomically with user-only permissions to `~/.config/openhug/token`.
The CLI refuses to send bearer tokens to non-loopback HTTP URLs unless `--allow-insecure-http`
is supplied.

## Compatibility and current limits

The server provides Hugging Face-style repository info, revision-aware `resolve` downloads,
pre-upload negotiation, and regular inline commits. Git transport, LFS, organizations, and
inference are not part of the first release. Native direct uploads are currently limited to
512 MiB per file; compatible inline uploads are limited to 10 MiB per file.
