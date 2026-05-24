# AGENTS.md

Behavioral guidelines for AI coding agents working in this repository.

## 1. Think Before Coding
Don't assume. Don't hide confusion. Surface tradeoffs.

Before implementing:
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them. Do not pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what is confusing. Ask.

## 2. Simplicity First
Minimum code that solves the problem. Nothing speculative.

- No features beyond what was asked.
- No abstractions for single-use code.
- No flexibility or configurability that was not requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: Would a senior engineer say this is overcomplicated? If yes, simplify.

## 3. Surgical Changes
Touch only what you must. Clean up only your own mess.

When editing existing code:
- Do not improve adjacent code, comments, or formatting.
- Do not refactor things that are not broken.
- Match existing style, even if you would do it differently.
- If you notice unrelated dead code, mention it. Do not delete it.

When your changes create orphans:
- Remove imports, variables, or functions that YOUR changes made unused.
- Do not remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

## 4. Goal-Driven Execution
Define success criteria. Loop until verified.

Transform tasks into verifiable goals:
- "Add validation" becomes "Write tests for invalid inputs, then make them pass"
- "Fix the bug" becomes "Write a test that reproduces it, then make it pass"
- "Refactor X" becomes "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:
1. [Step] -> verify: [check]
2. [Step] -> verify: [check]
3. [Step] -> verify: [check]

Strong success criteria let you loop independently. Weak criteria require constant clarification.

## 5. Destructive Actions Need Explicit Instruction
Don't mutate state outside the working tree without being told to.

- Includes `git commit`, `git push`, `git rebase`, `git reset --hard`, `git clean`, branch or file deletion, and dependency installs that modify lockfiles.
- Drafting an artifact is not an instruction to apply it. Writing a commit message is not an instruction to commit. Writing a script is not an instruction to run it.
- Default action after producing an artifact is to present it. The user applies it.

---

## Project Commands
- Toolchain: `mise install` (pinned in `.mise.toml`; first checkout needs `mise trust`)
- Build: `cargo build` (release: `cargo build --release`)
- Test: `cargo test --all-targets --locked`
- Coverage gate: 90% line coverage (CI-enforced via `cargo llvm-cov`). Raise it as tests grow; never lower without recording why.
- Lint: `cargo clippy --all-targets -- -D warnings`
- Format check: `cargo fmt --all -- --check`
- Run: `cargo run` (reads `./cloudseeder.toml` if present; defaults otherwise)

## Conventions
- Config: `--config <PATH>` / `CLOUDSEEDER_CONFIG` selects the file (default `./cloudseeder.toml`, optional). Env overrides for file fields follow the `CLOUDSEEDER_<FIELD>` convention; add new ones for real deployment needs (Line 2 still applies).
- CI uses SHA-pinned actions with `# vX.Y.Z` comments and digest-pinned Docker base images. Dependabot maintains both, but NOT `.mise.toml` (it can't parse it) or `jdx/mise-action`'s `version:` input: bump `rust`/`cargo-llvm-cov`/`actionlint` by hand, `mise outdated` lists them, and keep mise action versions in `ci.yml` and `release.yml` in lockstep. New actions/base images must follow the same form.
- Versioning: `Cargo.toml` carries the last released version. Bumps are made by the `release.yml` workflow only — never by hand. `publish-nightly` builds with a synthetic `<next-patch>-nightly.YYYYMMDD.<run>` version, mutating manifests in the runner only.
- `prefix` is an **obscurity gate, not authentication**. The server is HTTP-only by design — do not add Basic Auth, bearer tokens, or query secrets. New public routes live under `/<prefix>/`. Don't change `/healthz`'s response shape; `HEALTHCHECK` and orchestrators depend on it.
- Tests bind their own `TcpListener` and pass it to `serve_with_shutdown`; do not call `serve()` from tests — it installs process-wide signal handlers. Use the subprocess pattern in `tests/cli.rs` if you need signal-path coverage.

## AI Artifacts
Do not commit scratch notes, plans, drafts, or transcripts to the repository. Do not reference local scratch workspaces in any committed file, including source code, comments, docstrings, or documentation. Follow local artifact conventions if the developer's environment provides them; otherwise keep these out of the tree entirely.
