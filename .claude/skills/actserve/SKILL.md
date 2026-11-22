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
| Audit (ssh, ports, ufw, updates, fail2ban, containers) | `dotfiles servers audit NAME` | dotfiles, slice 2 |
| Bring a server to its declared state | `dotfiles servers bootstrap NAME`, `apply NAME` | dotfiles, slice 3 |
| Deploy API, Caddy, CDN, edge | `dotfiles acteble deploy\|deploy-caddy\|cdn-deploy\|edge-deploy [--dry-run]` | dotfiles, slice 4 |
| Roll back the API | `dotfiles acteble rollback [tag] [--dry-run]` | dotfiles, slice 4 |
| Containers and logs | `dotfiles acteble status`, `logs [service]` | dotfiles, slice 4 |
| Rotate a credential | `dotfiles acteble rotate jwt\|postgres\|minio\|all` | dotfiles, slice 5 |

## Rules

- Run `--dry-run` first on every deploy, rollback and rotate, and show the plan.
- A deploy, a rollback or a rotate changes production: do it only when the owner asked for it in this session. `acteble_api` is never merged by an agent.
- Secrets travel through `actesskey` or `dotfiles secret`, never on a command line.
- `actserve status` exits 0 only when every probed server answers with its expected hostname.

## API

`actserve` with no argument starts the REST API on `0.0.0.0:8080` (`/api/v1`: servers, domains, DNS, TLS). Servers, repositories and jobs are held in memory, and DNS, registrar and TLS providers are mocks: it changes no real domain or certificate yet. It has no authentication: do not expose it.
