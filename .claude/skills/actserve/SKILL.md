---
name: actserve
description: Administer servers through actserve: server status over SSH, and a REST API for servers, domains, DNS, TLS and provisioning engines. Use to check a server, or before calling or changing the actserve API.
---

# actserve

Rust (Axum, hexagonal) orchestrator for server administration. Feeds the Biorcrux UI.

## Commands

- `actserve status [NAME]`: reads the server inventory (`$DOTFILES_STATE_ROOT/servers.txt`, default `~/machine`) and probes each server with `ssh -o BatchMode=yes <target> hostname` (`$DOTFILES_SSH` overrides `ssh`). Exit 0 only when every probed server is ok.
- `actserve` (no argument): starts the REST API on `0.0.0.0:8080`.

## API (`/api/v1`)

`health`; `servers` (create, list, inspect, `health`, `maintenance`, `execute`, `metrics`, `services/{service}/action`, `reboot`, `bind-domain`); `domains` (check, create, list, inspect, `nameservers`, `auto-renew`); `dns/zones`. The full table is in `README.md`.

## Limits

- The API keeps servers, repositories and jobs in memory: a restart loses them.
- DNS, registrar and TLS providers are mocks: no real domain or certificate is changed.
- Provisioning engines (OpenTofu, Ansible, Crossplane, WireGuard) run through their CLIs.
- The API listens on every interface with permissive CORS and no authentication: do not expose it.
