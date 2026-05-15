# cloudseeder

Dynamic template-based server for Ubuntu autoinstall and Red Hat kickstart configs.

> **Status:** early bootstrap. The HTTP scaffold, config loader, obscurity prefix, and static template serving are in place. Dynamic template rendering (variable substitution) is not yet implemented — templates are served as static files for now.

## Quick start

```bash
mise trust          # one-time per clone (and after edits to .mise.toml)
mise install        # installs the toolchain pinned in .mise.toml
cargo run           # starts the server; logs the URL on the "ready" line
```

By default the server binds `127.0.0.1:8080` (localhost only) and generates a fresh 6-character prefix at startup. The full URL is printed on launch, e.g.:

```
INFO cloudseeder: prefix auto-generated ... prefix=qb10o0
INFO cloudseeder: ready url=http://127.0.0.1:8080/qb10o0/
```

`Ctrl+C` or `SIGTERM` triggers a graceful shutdown.

### CLI flags

```bash
cloudseeder --help            # usage
cloudseeder --version         # version (matches Cargo.toml)
cloudseeder --config ./my.toml
```

`--config <PATH>` and the `CLOUDSEEDER_CONFIG` env var are equivalent — the flag wins if both are given.

## Routes

| Path                                       | Status   | Notes                                                          |
|--------------------------------------------|----------|----------------------------------------------------------------|
| `GET /healthz`                             | 200      | `{"status":"ok"}` — liveness probe                             |
| `GET /<prefix>/`                           | 200      | Empty body (placeholder for served data)                       |
| `GET /<prefix>/<template>/`                | 200/404  | HTML index of the template's three files (404 if no folder)    |
| `GET /<prefix>/<template>/kickstart`       | 200/404  | Red Hat kickstart file (200 empty if file absent, 404 if no folder) |
| `GET /<prefix>/<template>/user-data`       | 200/404  | Ubuntu autoinstall / cloud-init user-data (same rules)         |
| `GET /<prefix>/<template>/meta-data`       | 200/404  | cloud-init meta-data (same rules)                              |
| anything else                              | 401      | Including `/<prefix>` without trailing `/`                     |

The prefix exists for "security through obscurity" — it does not authenticate. `/healthz` is intentionally unguarded so orchestrators and the Docker `HEALTHCHECK` can probe without knowing the prefix.

Template names must match `[a-z0-9-]+`. Anything else (uppercase, underscores, dots, slashes) returns 404.

## Templates

Each template is a folder under the configured `templates_dir` containing up to three files:

```
templates/
└── ubuntu-24-04/
    ├── kickstart        (optional)
    ├── user-data        (optional)
    └── meta-data        (optional)
```

Requests for a file that doesn't exist inside an existing template folder return 200 with an empty body — provisioners (Ubuntu autoinstall, cloud-init) often require all three files even when the contents are empty, so this avoids surprise breakage. Requests for a *template* that doesn't exist return 404.

A worked example lives in [`examples/templates/example/`](./examples/templates/example) — point `templates_dir` at `examples/templates` and visit `/<prefix>/example/` to see the index.

Files are served as `text/plain; charset=utf-8`. The index page at `/<prefix>/<template>/` is minimal HTML with three relative links — useful for confirming the server can see the template folder.

## Security posture

**Read this before deploying anywhere reachable from a network you don't control.**

- **No authentication.** The prefix is an accidental-access gate, not an auth mechanism. The server speaks plain HTTP, so the prefix (and anything served under it) is observable on the wire — adding Basic Auth, bearer tokens, or query secrets would all be observable too and would not materially help.
- **No TLS.** Provisioning targets (Ubuntu autoinstall, Red Hat kickstart) typically fetch over plain HTTP at boot, before they have a trust store. TLS would have to be terminated externally if you need it.
- **6-char prefix ≈ 31 bits.** Adequate to deter casual discovery and accidental crawlers; not adequate as a real bearer secret. Treat it accordingly.
- **The prefix appears in logs.** When auto-generated it is logged at `info` (so the operator can find it). Treat logs as sensitive while the service is running.

**Recommended operational model**

- Run only when actively provisioning hosts; stop the service once provisioning is complete.
- Default to `127.0.0.1`; bind `0.0.0.0` only when needed, and prefer an isolated provisioning network (VLAN, dedicated bridge, firewall rules) over an internet-facing host.
- Do not serve any content from cloudseeder that you would not be comfortable publishing — no passwords, no API tokens, no SSH keys you wouldn't paste into a public gist. Use post-install hooks or a separate secret-delivery mechanism for anything sensitive.

## Configuration

`cloudseeder.toml` is optional. Without it, built-in defaults apply.

```toml
# cloudseeder.toml
addr          = "127.0.0.1:8080"   # listen address; env CLOUDSEEDER_ADDR overrides
prefix        = "demo01"           # [a-z0-9]; if missing/empty, auto-generated each run
templates_dir = "./templates"      # folder containing per-template subfolders; env CLOUDSEEDER_TEMPLATES_DIR overrides
```

**Precedence** (highest wins):

| Source                                          | Affects                                |
|-------------------------------------------------|----------------------------------------|
| `--config <PATH>` flag / `CLOUDSEEDER_CONFIG`   | chooses *which* file to load           |
| `CLOUDSEEDER_ADDR` env var                      | overrides `addr`                       |
| `CLOUDSEEDER_TEMPLATES_DIR` env var             | overrides `templates_dir`              |
| `cloudseeder.toml` fields                       | any field                              |
| built-in defaults                               | fallback                               |

Env overrides for file fields follow the `CLOUDSEEDER_<FIELD>` convention. New overrides are added when a deployment need surfaces (containers, per-env switches) — not speculatively.

`RUST_LOG` follows the standard `tracing` env filter (default `info`).

Invalid prefixes are rejected at startup with the offending characters listed:

```
cloudseeder: invalid prefix "BAD-prefix!": only [a-z0-9] allowed (invalid: '!', '-', 'A', 'B', 'D')
```

## Download a binary

Standalone binaries are attached to each release. No runtime dependencies beyond glibc (Linux) or the system libc (macOS).

| Archive                                                       | For                                          |
|---------------------------------------------------------------|----------------------------------------------|
| `cloudseeder-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`          | Intel/AMD Linux (servers, dev VMs)           |
| `cloudseeder-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz`         | ARM Linux (AWS Graviton, Raspberry Pi)       |
| `cloudseeder-vX.Y.Z-aarch64-apple-darwin.tar.gz`              | Apple Silicon Mac (M1, M2, M3, ...)          |
| `cloudseeder-vX.Y.Z-x86_64-apple-darwin.tar.gz`               | Intel Mac                                    |

Each release also ships a `SHA256SUMS` file. Verify before extracting:

```bash
VERSION=vX.Y.Z
TARGET=aarch64-apple-darwin
ARCHIVE="cloudseeder-${VERSION}-${TARGET}.tar.gz"
BASE="https://github.com/michaeljstutz/cloudseeder/releases/download/${VERSION}"

curl -LO "${BASE}/${ARCHIVE}"
curl -LO "${BASE}/SHA256SUMS"
shasum -a 256 -c SHA256SUMS --ignore-missing

tar xzf "${ARCHIVE}"
./cloudseeder --version
```

**macOS first-run note.** Binaries are unsigned (no Apple Developer Program subscription). Gatekeeper will refuse the first launch with *"cannot be opened because the developer cannot be verified"*. Clear the quarantine attribute once:

```bash
xattr -d com.apple.quarantine ./cloudseeder
```

## Docker

**Pull a published image:**

```bash
docker pull ghcr.io/michaeljstutz/cloudseeder:latest                       # most recent stable release
docker pull ghcr.io/michaeljstutz/cloudseeder:v0.0.1                       # specific release
docker pull ghcr.io/michaeljstutz/cloudseeder:nightly                      # latest main-branch nightly
docker pull ghcr.io/michaeljstutz/cloudseeder:0.0.1-nightly.20260513.47    # specific nightly build
```

See [Releases & distribution](#releases--distribution) for the full tag scheme.

**Run it:**

```bash
docker run --rm -p 8080:8080 ghcr.io/michaeljstutz/cloudseeder:latest
```

To pin or override config, mount a file and point `--config` at it:

```bash
docker run --rm -p 8080:8080 \
  -v "$PWD/cloudseeder.toml:/etc/cloudseeder.toml:ro" \
  ghcr.io/michaeljstutz/cloudseeder:latest \
  --config /etc/cloudseeder.toml
```

To serve templates from the host (edit them live, no rebuild):

```bash
docker run --rm -p 8080:8080 \
  -v "$PWD/templates:/etc/cloudseeder/templates:ro" \
  ghcr.io/michaeljstutz/cloudseeder:latest
```

The container's WORKDIR is `/etc/cloudseeder`, so the in-binary default `templates_dir = "./templates"` resolves to `/etc/cloudseeder/templates`. Mount your host directory there and the server picks it up — no config-file mount needed for the templates case. Use `:ro` if you don't want the container to be able to modify the files (it won't, but defense in depth costs nothing).

**Build it locally:**

```bash
docker build -t cloudseeder .
docker run --rm -p 8080:8080 cloudseeder
```

The image runs as a non-root user, exposes `8080`, and ships a `HEALTHCHECK` that polls `/healthz` every 10 s. The container sets `CLOUDSEEDER_ADDR=0.0.0.0:8080` so it's reachable from outside the container.

## Releases & distribution

Two streams ship from this repo:

- **Stable releases** — cut manually from the **Release** workflow. Each is a SemVer tag (`vX.Y.Z`), a `release: vX.Y.Z` commit on `main`, four standalone binaries (Linux x86_64/arm64, macOS x86_64/arm64) with a `SHA256SUMS` file, a multi-arch (`linux/amd64`, `linux/arm64`) GHCR image, and a GitHub Release with auto-generated notes.
- **Nightlies** — published automatically on every merge to `main` as a SemVer pre-release version (`0.0.1-nightly.20260513.47`), so users on Dependabot delay or those chasing a recent fix can opt into bleeding-edge without losing version traceability. Nightlies publish the multi-arch image only (no standalone binaries).

### Cutting a release

1. Actions tab → **Release** workflow → *Run workflow*.
2. Pick `patch`, `minor`, or `major` from the dropdown.
3. Click *Run workflow*. The workflow only runs from `main`.

What the workflow does, in order:

1. A read-only **verify** job installs Rust, bumps the version in `Cargo.toml`/`Cargo.lock`, runs `cargo build --release --locked` and `cargo test --all-targets --locked`. It carries no write tokens, so a compromised build dependency cannot push code or images.
2. A **build** matrix job produces the four standalone binaries (Linux x86_64/arm64 on native runners, macOS x86_64/arm64 on Apple Silicon runners with cross-compile for Intel). Each target tars the binary plus `LICENSE` and uploads the archive as a workflow artifact.
3. A separate **publish** job downloads the bumped manifests and all four binary archives, generates a combined `SHA256SUMS` file, commits `release: vX.Y.Z` to `main`, tags `vX.Y.Z`, and pushes both. It builds the multi-arch image (linux/amd64 + linux/arm64 via QEMU), publishes it to GHCR with `:vX.Y.Z`, `:latest`, and `:nightly`, then creates a GitHub Release with auto-generated notes and the four archives plus `SHA256SUMS` attached.

**Concurrency:** the Release workflow serializes itself — a second dispatch waits for the first to finish rather than racing it.

### Nightly stream

On every push to `main`, after `test` and `docker` jobs pass, the `publish-nightly` job:

1. Reads the current `Cargo.toml` version (the *last released* version, or `0.0.0` pre-first-release).
2. Increments the patch component to get the *next-pending* version.
3. Computes a SemVer pre-release suffix: `nightly.YYYYMMDD.<github.run_number>`. The date is human-readable; the run number is globally monotonic so versions sort correctly and never collide.
4. Mutates `Cargo.toml`/`Cargo.lock` in the runner only (no commit) and builds the image — so `cloudseeder --version` inside the image reports the nightly version.

A release immediately overwrites `:nightly` so it never lags behind. (The release commit doesn't itself trigger CI — `GITHUB_TOKEN` pushes don't trigger workflows by default.)

### Image tag scheme

All published images are multi-arch (`linux/amd64`, `linux/arm64`). Docker picks the right manifest for your host automatically.

| Tag                                    | Source                       | Use it for                                       |
|----------------------------------------|------------------------------|--------------------------------------------------|
| `:vX.Y.Z`                              | Release workflow             | pinned production deployments                    |
| `:latest`                              | Release workflow             | latest stable release                            |
| `:nightly`                             | Nightly + Release            | rolling latest from `main` (resets each release) |
| `:X.Y.Z-nightly.YYYYMMDD.N`            | Nightly                      | pinning a specific nightly build                 |
| `:sha-abc1234`                         | Nightly                      | bisecting a specific commit on `main`            |

### Version roadmap

- **`0.0.x`** — foundational / bootstrap. HTTP scaffold, config loader, prefix gate, CI/CD plumbing. (Current.)
- **`0.1.0`** — first functional milestone: serving a real autoinstall or kickstart template.
- **`0.x.y`** — growing the feature set: URL conventions, structured logging, webhooks, callbacks to remote services.
- **`1.0.0`** — API stability claim once the operational shape has settled.

## Repository setup (one-time)

Both workflows declare their own `permissions:` blocks, so the default repo Workflow-permissions setting is fine — no change needed before the first push. Two things you may want to do after the first publish:

1. **Packages → cloudseeder → Package settings:** set visibility to *Public* if you want anonymous `docker pull`. (The package shows up under your user/org packages after the first publish.)
2. **(Optional) Settings → Branches → Branch protection on `main`:** if you later enable required reviews, the Release workflow's direct push to `main` will be blocked. Either add a bypass for GitHub Actions or switch the release flow to a PR-based model.

**Dependabot** is wired up in `.github/dependabot.yml` and watches three ecosystems weekly:

- `github-actions` — opens PRs to bump SHA-pinned actions (commit prefix `ci`)
- `cargo` — bumps `Cargo.toml` / `Cargo.lock` (commit prefix `deps`)
- `docker` — bumps the digest-pinned base images in `Dockerfile` (commit prefix `docker`)

Merging a Dependabot PR runs CI; if green, the change ships on the next release.

## Development

```bash
cargo fmt --all -- --check                # formatting
cargo clippy --all-targets -- -D warnings # lints
cargo test --all-targets --locked         # tests (config unit + HTTP integration + CLI subprocess)
cargo llvm-cov --all-targets --locked --summary-only  # line coverage (CI gates at 90%)
```

**Workflows:**

- `.github/workflows/ci.yml` — runs fmt/clippy/test + a Docker build smoke check on every PR and every push to `main`. On `main`, after both pass, a `publish-nightly` job pushes `:main` and `:sha-<short>` images to GHCR.
- `.github/workflows/release.yml` — manual `workflow_dispatch` from the Actions tab. See [Releases & distribution](#releases--distribution).

All third-party actions are SHA-pinned with `# vX.Y.Z` comments; the Docker base images are pinned by digest. Dependabot maintains both — see [Repository setup](#repository-setup-one-time).

See [AGENTS.md](./AGENTS.md) for guidelines that apply to AI coding agents working in this repo.

## License

MIT — see [LICENSE](./LICENSE).
