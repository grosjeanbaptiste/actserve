# actserve — roadmap

Goal (owner, 2026-11-21): actserve takes over every VPS and network
administration feature `dotfiles` carries today, so `dotfiles` only keeps
configuration files and indexes (Baba ADR-042). Each slice: tests first,
compared with the bash behaviour, then `dotfiles` calls actserve.

## Inventory of what dotfiles does (source: `dotfiles servers --help`,
`dotfiles acteble --help`, `scripts/lib/servers.sh`, `acteble-rotate.sh`)

| dotfiles command | actserve today | Slice |
|---|---|---|
| `servers list`, inventory `servers.txt` (NAME\|SSH-TARGET\|HOSTNAME) | goes to actparc; actserve reads it (`FileServerInventory`) | 1 done |
| `servers status` (ssh hostname, ok/unreachable/wrong-host) | `actserve status [NAME]` (`FleetStatusService`, `SshHostnameProbe`) | 1 done |
| `servers audit` (hostname check, then `edge audit` over ssh with sudo; exit 0/1/3, 2 unknown) | `actserve audit NAME` (`FleetAuditService`, `SshEdgeAudit`) | 2 done (`acteble edge-audit` still dotfiles) |
| `servers bootstrap` / `apply` (reach, harden, runtime, edge, stacks, audit) | 3a done: `actserve harden NAME [--dry-run]` (sshd drop-in with rollback, ufw, fail2ban, unattended-upgrades; `HardenService`). Left: runtime, edge, stacks and the `bootstrap`/`apply` composition | 3 (3a done) |
| `acteble deploy`, `deploy-caddy`, `cdn-deploy`, `edge-deploy` (+ `--dry-run`) | none (OpenTofu/Ansible/Crossplane engines only) | 4 |
| `acteble rollback`, `status`, `logs`, `edge-status`, `cdn-status`, `cdn-logs` | none | 4 |
| `acteble rotate jwt\|postgres\|minio\|all` | none; secrets belong to actesskey | 5 |
| `acteble edge-backup` | none | 5 |
| DNS, TLS, WireGuard | present (domain, adapters, API) | verify |
| `service start\|stop\|status` (local, no launchd) | out: local processes go to actparc / actautoma | - |

## Decision (owner, 2026-11-23)

Nothing for the servers passes through `dotfiles` any more. Every row above
moves to actserve; each slice is its own PR, then `dotfiles` drops the command
in a separate PR after a search of every caller (`~/Source`, `~/machine`, the
Claude settings, hooks and docs). Until a slice is merged and installed,
`dotfiles` keeps its command and delegates where actserve already has it
(`servers status`: dotfiles PR in review).

## Boundary (owner, 2026-11-21)

actserve administers how things run (status, audit, bootstrap, deploy, rotate,
backup). actparc lists who holds what (inventory of machines, volumes,
servers). Slice 1 therefore reads the server inventory from actparc instead of
owning `servers.txt`.

## Rules

No silent failure, `--dry-run` on every mutating action, nothing removed,
secrets via actesskey never printed, 100 % TDD with failure-path tests.
