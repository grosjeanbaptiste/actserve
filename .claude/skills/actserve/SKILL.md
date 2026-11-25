---
name: actserve
description: Manage the owner's VPS: check, audit, bootstrap, deploy, roll back, read logs, rotate credentials. Use for any task on the VPS or its edge, CDN, DNS and TLS, and before calling or changing the actserve API.
---

# actserve — the VPS

`actserve` is becoming the single place to administer the VPS (owner, 2026-11-21; `ROADMAP.md` lists the slices). Each command moves from `dotfiles` to `actserve` when its slice is done; `dotfiles <command> --help` names the new owner when that happens. Follow that answer, not this table.

## Which command, today

| Task | Command | Owner now |
|---|---|---|
| List servers | `dotfiles servers list` | dotfiles (inventory `~/machine/servers.txt`) |
| Is the VPS up and the right host | `actserve status [NAME]` | actserve |
| Audit (ssh, ports, ufw, updates, fail2ban, containers) | `actserve audit NAME` or `actserve edge-audit NAME` (same audit) | actserve |
| Bring a server to its declared state | `actserve bootstrap NAME [--dry-run]`, `apply NAME [--dry-run]` (steps alone: `harden`, `runtime`, `edge`, `stacks`) | actserve |
| Deploy API, Caddy, CDN, edge | `actserve deploy\|deploy-caddy\|cdn-deploy\|edge-deploy NAME [--dry-run]` | actserve |
| Roll back the API | `actserve rollback NAME [TAG] [--dry-run]` | actserve |
| Containers, logs, edge status | `actserve ps NAME api\|cdn`, `logs NAME api\|cdn [SERVICE]`, `edge-status NAME` | actserve |
| Save the edge certificates (private keys, local 0600 archive) | `actserve edge-backup NAME [DIR]` (default `~/Backups/acteble-edge`) | actserve |
| Rotate a credential, then redeploy | `actserve rotate NAME jwt\|postgres\|minio\|all [--dry-run] [--yes]` (a real run needs `--yes`; `$ACTEBLE_DB_USER`, default `acteble_prod`) | actserve |
| Cdn geo publish and build | `dotfiles acteble cdn-publish-geo`, `cdn-build-geo` | dotfiles, slice 7 |

## Rules

- Run `--dry-run` first on every deploy, rollback and rotate, and show the plan.
- A deploy, a rollback or a rotate changes production: do it only when the owner asked for it in this session. `acteble_api` is never merged by an agent.
- Secrets travel through `actesskey` or `dotfiles secret`, never on a command line.
- `actserve status` exits 0 only when every probed server answers with its expected hostname.

## API

`actserve` with no argument starts the REST API on `0.0.0.0:8080` (`/api/v1`: servers, domains, DNS, TLS). Servers, repositories and jobs are held in memory, and DNS, registrar and TLS providers are mocks: it changes no real domain or certificate yet. It has no authentication: do not expose it.
