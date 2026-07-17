# OpenHug Codebase Audit Report

Reviewed July 16, 2026. The code builds cleanly, but the review found five high-severity security or privacy risks, along with several correctness and operational issues.

## Severity summary

| Severity | Count | Main themes |
| --- | ---: | --- |
| High | 5 | Account takeover, stored XSS, memory DoS, cross-repository access, privacy defaults |
| Medium | 8 | Information leakage, brute force, storage races, incomplete Hugging Face behavior, dependencies |
| Low | 4 | API fallback behavior, permissions, UI limitations, maintenance gaps |

## High severity

### H-01: Unauthenticated first-run superuser takeover

The setup endpoint requires no bootstrap secret or local-only authorization. Before initialization, the first network client to call it creates the superuser account. The transaction lock prevents races but does not identify an authorized operator.

The default bind address is `0.0.0.0`, and the README instructs operators to open the public URL to finish onboarding, increasing exposure.

Evidence:

- `crates/openhug-server/src/api.rs:84`
- `crates/openhug-server/src/config.rs:32`
- `README.md:18`

Impact: An attacker who reaches a newly deployed instance first gains permanent superuser access.

Recommendation: Require a high-entropy one-time setup token supplied through the environment, or create the first administrator through a local CLI. Consider binding to loopback until initialization is complete.

### H-02: Uploaded HTML/SVG can execute as trusted same-origin content

Downloads use `mime_guess` and return uploaded data inline with types such as `text/html` or `image/svg+xml`. They do not set `Content-Disposition: attachment`, a restrictive Content Security Policy, or `X-Content-Type-Options: nosniff`.

Evidence:

- `crates/openhug-server/src/api.rs:498`
- `crates/openhug-server/src/api.rs:525`

Impact: A user with write access can publish an HTML or SVG file and lure another user to its raw URL. JavaScript then runs on the OpenHug origin and can make authenticated API requests using the victim's session, including administrative requests if the victim is a superuser.

Recommendation: Serve user content from a separate, cookie-less origin. As an immediate safeguard, force `Content-Disposition: attachment`, use `application/octet-stream` for active formats, add `nosniff`, and apply a sandboxing Content Security Policy.

### H-03: Unauthenticated requests may buffer up to 512 MiB each

A global 512 MiB body limit applies to every route, not only blob uploads. JSON endpoints such as login, signup, and setup can therefore buffer extremely large unauthenticated bodies. Blob and Hugging Face commit handlers also use `Bytes`, fully materializing their request bodies in memory.

Evidence:

- `crates/openhug-server/src/main.rs:125`
- `crates/openhug-server/src/api.rs:141`
- `crates/openhug-server/src/api.rs:385`
- `crates/openhug-server/src/api.rs:727`

Impact: A few concurrent requests can exhaust server memory. Login and setup do not require authentication, making this remotely exploitable.

Recommendation: Apply small per-route JSON limits, such as 16–64 KiB. Stream blob uploads directly to storage with incremental hashing, and add concurrency, request-timeout, and connection limits.

### H-04: Revision downloads are not bound to the requested repository

The requested repository is authorized first, but a UUID revision is subsequently looked up only by `commit_id` and path. The query never checks that the commit belongs to the authorized repository.

Evidence:

- `crates/openhug-server/src/api.rs:503`
- `crates/openhug-server/src/api.rs:509`

Impact: If a private commit UUID becomes known, an attacker can place it in the download URL for any public repository and retrieve matching private files without authentication. UUIDv4 makes blind guessing impractical, but leaked identifiers become cross-tenant capabilities.

Recommendation: Join `commits` and require `commits.repository_id = repo.id` in the file lookup. Add integration tests covering revisions from unrelated private repositories.

### H-05: Default visibility is ignored and silently defaults to public

The configured `instance_settings.default_visibility` is never read when repositories are created. The server-side default helper always returns `"public"`, and the frontend independently starts every repository form as public.

Evidence:

- `crates/openhug-server/src/api.rs:77`
- `crates/openhug-server/src/api.rs:310`
- `crates/openhug-server/src/api.rs:334`
- `web/app/page.tsx:615`

Impact: Administrators can configure private-by-default behavior, yet clients that omit visibility create public repositories. This can expose later uploads contrary to policy.

Recommendation: Make request visibility optional and resolve missing values from `instance_settings` inside the creation transaction. Have the UI fetch and use that value.

## Medium severity

### M-01: Internal database and storage errors are returned to clients

All errors use `self.to_string()` in the JSON response, including database and `anyhow` errors.

Evidence:

- `crates/openhug-server/src/error.rs:32`

Constraint failures can expose schema and constraint names; storage failures can expose local paths or endpoint details. Return a generic server-error message while retaining detailed structured logs.

Relatedly, duplicate repositories, overlong commit messages, invalid instance names, and other predictable constraint failures become 500 responses instead of 400 or 409 responses.

### M-02: No login throttling and observable username timing

Login performs expensive password verification only when the account exists. There is no rate limiting, IP/account backoff, or dummy hash verification for unknown identities.

Evidence:

- `crates/openhug-server/src/api.rs:141`

This enables password guessing, CPU exhaustion through Argon2 verification, and timing-based account enumeration. Add layered IP/account throttling and verify a fixed dummy hash when no user is found.

### M-03: Global blob deduplication crosses authorization boundaries

Commit creation accepts any SHA-256 and size found in the global `blobs` table. It does not establish that the caller uploaded or is authorized to access that blob.

Evidence:

- `crates/openhug-server/src/api.rs:452`

If a private blob's digest and size leak, another user can attach it to their public repository and download it. Track per-user or per-repository upload grants, or require a scoped, expiring upload receipt rather than treating a content hash as authorization.

### M-04: Uploaded blobs race with garbage collection

A blob is written and registered before the later commit request references it. Garbage collection immediately considers every unreferenced blob eligible for deletion.

Evidence:

- `crates/openhug-server/src/api.rs:396`
- `crates/openhug-server/src/main.rs:151`

An hourly collection between upload and commit can delete a valid in-progress upload. Use upload leases or staging records, or exclude recently created blobs for a safe grace period.

### M-05: Startup checks only one blob despite claiming full verification

Startup selects an arbitrary single digest with `LIMIT 1` and verifies only that object. The README says the backend is verified to contain referenced blobs.

Evidence:

- `crates/openhug-server/src/main.rs:52`
- `README.md:31`

A partially missing or incorrect backend may pass startup validation and later return missing files. Validate every referenced blob, or clearly implement a sampled or asynchronous integrity check with health status.

### M-06: Hugging Face commit semantics silently discard operations

The endpoint:

- Ignores both `deletedFile` and `deletedFolder`.
- Ignores the requested revision and always updates the main head.
- Stores incoming blobs before checking that the repository exists and belongs to the caller.
- Allows pre-upload negotiation without validating the target repository.

Evidence:

- `crates/openhug-server/src/api.rs:707`
- `crates/openhug-server/src/api.rs:727`
- `crates/openhug-server/src/api.rs:783`

Clients may receive success even though requested deletions or revision semantics were not applied. Unsupported operations should be rejected explicitly until implemented.

### M-07: Two reachable Rust dependency advisories

`cargo-audit` found:

- `RUSTSEC-2026-0194`: quadratic attribute processing in `quick-xml 0.38.4`.
- `RUSTSEC-2026-0195`: unbounded namespace allocation in `quick-xml 0.38.4`.

The dependency is active through `object_store 0.12.5` and its S3/XML support.

Evidence:

- `Cargo.toml:21`
- `Cargo.lock`

Reachability is conditional: the hostile XML would need to come from the configured S3-compatible endpoint, or an attacker able to tamper with an HTTP endpoint. Upgrade to an `object_store` release that uses `quick-xml >= 0.41.0`.

`RUSTSEC-2023-0071` for `rsa` was also present in `Cargo.lock`, but `cargo tree` showed it is not in the active build graph; it comes from the unused SQLx MySQL package.

### M-08: Administrators can remove the last usable superuser

Self-suspension is prevented, but a superuser can demote themselves or suspend or demote every other superuser.

Evidence:

- `crates/openhug-server/src/api.rs:870`

This can permanently lock administration. Enforce that at least one active superuser remains, ideally transactionally.

## Lower-severity findings

### L-01: CLI transport and correctness hazards

- The CLI sends bearer tokens to arbitrary HTTP server URLs without warning. Enforce HTTPS except for loopback, or require an explicit insecure flag. Evidence: `crates/openhug-cli/src/main.rs:20`.
- Directory uploads silently discard `WalkDir` errors, potentially committing an incomplete tree as successful. Evidence: `crates/openhug-cli/src/main.rs:257`.
- `repo create OWNER/NAME` discards `OWNER`, always creating under the authenticated account. Evidence: `crates/openhug-cli/src/main.rs:204`.
- Download paths are not percent-encoded, so names containing characters such as `?` or `#` break. Evidence: `crates/openhug-cli/src/main.rs:336`.

### L-02: Unknown API GET paths return the frontend with HTTP 200

The global frontend fallback returns the single-page application for unknown API GET paths instead of an API 404.

Evidence:

- `crates/openhug-server/src/main.rs:124`

This can confuse clients, monitoring, and typo detection. Add a distinct API fallback that returns a structured 404 before the frontend fallback.

### L-03: Local secret-file permissions and retention

- The local `.env` is excluded by `.gitignore`, but its filesystem mode was `0644` during review. On multi-user hosts, database or storage credentials may be readable by other users.
- Expired sessions and API tokens are never purged, causing indefinite table growth.
- The CLI writes the token before changing its mode to `0600`, creating a small exposure window on permissive umasks. Create it atomically with the correct permissions instead.

### L-04: UI and maintenance gaps

- Public repositories are supported by the API, but the web application always displays authentication before repository content. Evidence: `web/app/page.tsx:126`.
- The retention period is part of administrator settings but is not editable through the administration UI.
- The frontend lint script invokes the deprecated `next lint` command and launches an interactive first-time setup prompt instead of running as an automated check. Evidence: `web/package.json:7`.

## Verification results

- `cargo fmt --all -- --check`: passed.
- `cargo test --workspace --all-targets`: passed, but only five unit tests exist.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed.
- `npm run build`: passed.
- `npm audit`: zero vulnerabilities across 105 dependencies.
- `cargo-audit`: three lockfile advisories; two are in the active dependency graph.
- `npm run lint`: failed as an automated check because `next lint` launches an interactive ESLint configuration prompt.

The largest testing gap is the absence of PostgreSQL-backed integration tests for authentication, authorization, revisions, garbage collection, setup, and private repository isolation.

## Recommended remediation order

1. Isolate or force-download uploaded active content.
2. Protect first-run setup with a one-time secret or local-only workflow.
3. Add per-route body limits, streaming uploads, and concurrency or rate controls.
4. Bind revision queries to the authorized repository.
5. Honor the configured default visibility.
6. Stop returning internal errors and address the `quick-xml` advisories.
7. Add integration tests for private-repository boundaries and upload/garbage-collection races.

## Review scope and limitations

The review covered the Rust server, Rust CLI, PostgreSQL migration, Next.js frontend, configuration, dependency lockfiles, and available automated checks. It was primarily a static review supplemented by compilation, unit tests, lint-style checks, and dependency advisory scans. No live PostgreSQL or S3-compatible integration environment was available, so database and object-storage behavior was assessed from code and schema rather than end-to-end runtime tests.
