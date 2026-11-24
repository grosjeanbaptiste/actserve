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
| `servers bootstrap` / `apply` (reach, harden, runtime, edge, stacks, audit) | 3a done: `actserve harden NAME [--dry-run]` (sshd drop-in with rollback, ufw, fail2ban, unattended-upgrades; `HardenService`). 3b done: `actserve runtime NAME [--dry-run]` (Docker + Compose, `daemon.json` log rotation, stack directories from `dirs.txt`). 3c done: `actserve edge NAME` and `actserve stacks NAME` (network, Caddy, cdn checks; starting them waits for slice 4, the deploy, and fails saying so). 3d done: `actserve bootstrap NAME [--dry-run]` and `apply` (all six steps, every declared file validated first, audit last decides the exit status). Starting Caddy/cdn waits for slice 4 | 3 done |
| `acteble deploy-caddy`, `cdn-deploy`, `edge-deploy` (+ `--dry-run`) | 4d-1 done: `actserve deploy-caddy NAME [--dry-run]` (Caddyfile of `origin/main`, backup, write aside then move, reload, restore on refusal; `CaddyDeployService`, `GitCaddyfileSource`). 4d-2 done: `actserve cdn-deploy NAME [--dry-run]` (`CdnDeployService`: mkdir, copy `compose.yml`, shared network, pull and up with ghcr login and tmpfs env, geo prefix opened). 4d-3 done: `actserve edge-deploy NAME [--dry-run]` (`EdgeDeployService`: mkdir, copy `Caddyfile`, `compose.yml`, `sites/`, binary taken out of the ghcr image, `./edge up` with tmpfs env; `ScpRemoteCopy` now passes `-r`). The real `StackDeployPort` (so bootstrap starts the edge and the cdn) still to do | 4d (3 of 3 commands) |
| `acteble status`, `logs`, `cdn-status`, `cdn-logs` | 4a done: `actserve ps NAME api\|cdn`, `actserve logs NAME api\|cdn [SERVICE]` (`ObserveService`, `SshTerminal`) | 4a done |
| `acteble edge-status` | 4b done: `actserve edge-status NAME` (secret from actesskey on the server tmpfs, removed after; `SecretStorePort`, `with_remote_env`) | 4b done |
| `acteble deploy`, `rollback` | 4c done: `actserve deploy NAME [--dry-run]`, `actserve rollback NAME [TAG] [--dry-run]` (`DeployService`, `ScpRemoteCopy`) | 4c done |
| `acteble rotate jwt\|postgres\|minio\|all` | none; secrets belong to actesskey | 5 |
| `acteble edge-backup` | none | 5 |
| `acteble edge-audit` | none (`actserve audit` runs the same audit over ssh; the standalone command is missing) | 6 |
| `acteble cdn-publish-geo`, `cdn-build-geo` (write to the CDN bucket) | none | 7 |
| Real `StackDeployPort` (bootstrap and apply start the edge and the cdn) | `UnportedStackDeploy` refuses and says so | 4d-4 |
| Delegation: dotfiles calls actserve for every row above, then drops its own commands (one PR each, after a search of every caller) | only `servers audit` (dotfiles PR #62, not merged) | 8 |

Done means (owner, 2026-11-24): every VPS feature of dotfiles runs end to end
in actserve. Slice order: 4d-4, 5, 6, 7, then 8. A row is not finished until a
parity check against the bash script is written down in `CONTEXT.md`.
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
