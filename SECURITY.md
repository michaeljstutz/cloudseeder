# Security Policy

## Supported versions

cloudseeder is pre-1.0 (`0.0.x`, bootstrap). Only the latest release and
the current `main` receive security fixes. There are no backports.

## Reporting a vulnerability

Report privately via GitHub's **Report a vulnerability** button under the
repository's **Security** tab (Security Advisories). This opens a private
channel; do not file a public issue for a suspected vulnerability.

Please include affected version or commit, reproduction steps, and impact.
This is a small project maintained on a best-effort basis: expect an
initial response within roughly a week, not a same-day SLA.

## What is not a vulnerability

The following are deliberate, documented design decisions, not security
bugs. Reports about them will be closed with a pointer here:

- **No authentication.** The server is unauthenticated by design.
- **HTTP only, no TLS.** Provisioning targets fetch over plain HTTP at
  boot, before they have a trust store. Terminate TLS externally if needed.
- **`prefix` is an obscurity gate, not a secret.** It deters accidental
  discovery; it is not a bearer credential and is observable on the wire
  and in logs.

See the "Security posture" section of [README.md](./README.md) for the
full operational model and threat assumptions. Genuine memory-safety
issues, path-traversal escapes, or anything that lets a request read or
write outside the configured templates directory *are* in scope.
