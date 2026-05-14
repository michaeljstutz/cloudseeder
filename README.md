# cloudseeder

Dynamic template-based server for Ubuntu autoinstall and Red Hat kickstart configs.

> **Status:** early bootstrap. The HTTP scaffold, config loader, and obscurity prefix are in place; template rendering for autoinstall/kickstart payloads is not yet implemented.

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

| Path                | Status | Notes                                    |
|---------------------|--------|------------------------------------------|
| `GET /healthz`      | 200    | `{"status":"ok"}` — liveness probe       |
| `GET /<prefix>/`    | 200    | Empty body (placeholder for served data) |
| anything else       | 401    | Including `/<prefix>` without trailing `/` |

The prefix exists for "security through obscurity" — it does not authenticate. `/healthz` is intentionally unguarded so orchestrators and the Docker `HEALTHCHECK` can probe without knowing the prefix.

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
addr   = "127.0.0.1:8080"   # listen address; env CLOUDSEEDER_ADDR overrides this field
prefix = "demo01"            # [a-z0-9]; if missing/empty, auto-generated each run
```

**Precedence** (highest wins):

| Source                                          | Affects               |
|-------------------------------------------------|-----------------------|
| `--config <PATH>` flag / `CLOUDSEEDER_CONFIG`   | chooses *which* file to load |
| `CLOUDSEEDER_ADDR` env var                      | overrides `addr` only |
| `cloudseeder.toml` fields                       | any field             |
| built-in defaults                               | fallback              |

Only `addr` has an env-var override today. Other fields come from file or default — adding more env overrides is a deliberate decision, not the path of least resistance.

`RUST_LOG` follows the standard `tracing` env filter (default `info`).

Invalid prefixes are rejected at startup with the offending characters listed:

```
cloudseeder: invalid prefix "BAD-prefix!": only [a-z0-9] allowed (invalid: '!', '-', 'A', 'B', 'D')
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

**Build it locally:**

```bash
docker build -t cloudseeder .
docker run --rm -p 8080:8080 cloudseeder
```

The image runs as a non-root user, exposes `8080`, and ships a `HEALTHCHECK` that polls `/healthz` every 10 s. The container sets `CLOUDSEEDER_ADDR=0.0.0.0:8080` so it's reachable from outside the container.

## Releases & distribution

Two streams ship from this repo:

- **Stable releases** — cut manually from the **Release** workflow. Each is a SemVer tag (`vX.Y.Z`), a `release: vX.Y.Z` commit on `main`, a GHCR image, and a GitHub Release with auto-generated notes.
- **Nightlies** — published automatically on every merge to `main` as a SemVer pre-release version (`0.0.1-nightly.20260513.47`), so users on Dependabot delay or those chasing a recent fix can opt into bleeding-edge without losing version traceability.

### Cutting a release

1. Actions tab → **Release** workflow → *Run workflow*.
2. Pick `patch`, `minor`, or `major` from the dropdown.
3. Click *Run workflow*. The workflow only runs from `main`.

What the workflow does, in order:

1. A read-only **verify** job installs Rust, bumps the version in `Cargo.toml`/`Cargo.lock`, runs `cargo build --release --locked` and `cargo test --all-targets --locked`. It carries no write tokens, so a compromised build dependency cannot push code or images.
2. A separate **publish** job picks up the bumped manifests, commits `release: vX.Y.Z` to `main`, tags `vX.Y.Z`, and pushes both. It builds the release image, publishes it to GHCR with `:vX.Y.Z`, `:latest`, and `:nightly`, then creates a GitHub Release with auto-generated notes.

**Concurrency:** the Release workflow serializes itself — a second dispatch waits for the first to finish rather than racing it.

### Nightly stream

On every push to `main`, after `test` and `docker` jobs pass, the `publish-nightly` job:

1. Reads the current `Cargo.toml` version (the *last released* version, or `0.0.0` pre-first-release).
2. Increments the patch component to get the *next-pending* version.
3. Computes a SemVer pre-release suffix: `nightly.YYYYMMDD.<github.run_number>`. The date is human-readable; the run number is globally monotonic so versions sort correctly and never collide.
4. Mutates `Cargo.toml`/`Cargo.lock` in the runner only (no commit) and builds the image — so `cloudseeder --version` inside the image reports the nightly version.

A release immediately overwrites `:nightly` so it never lags behind. (The release commit doesn't itself trigger CI — `GITHUB_TOKEN` pushes don't trigger workflows by default.)

### Image tag scheme

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
cargo test --all-targets --locked         # tests (config unit + HTTP integration)
```

**Workflows:**

- `.github/workflows/ci.yml` — runs fmt/clippy/test + a Docker build smoke check on every PR and every push to `main`. On `main`, after both pass, a `publish-nightly` job pushes `:main` and `:sha-<short>` images to GHCR.
- `.github/workflows/release.yml` — manual `workflow_dispatch` from the Actions tab. See [Releases & distribution](#releases--distribution).

All third-party actions are SHA-pinned with `# vX.Y.Z` comments; the Docker base images are pinned by digest. Dependabot maintains both — see [Repository setup](#repository-setup-one-time).

See [AGENTS.md](./AGENTS.md) for guidelines that apply to AI coding agents working in this repo.

## License

MIT — see [LICENSE](./LICENSE).
