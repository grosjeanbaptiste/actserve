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
| `servers bootstrap` / `apply` (reach, harden, runtime, edge, stacks, audit) | 3a done: `actserve harden NAME [--dry-run]` (sshd drop-in with rollback, ufw, fail2ban, unattended-upgrades; `HardenService`). 3b done: `actserve runtime NAME [--dry-run]` (Podman, stack directories from `dirs.txt`; no Docker, no `daemon.json`; PR #33). 3c done: `actserve edge NAME` and `actserve stacks NAME` (network, Caddy, cdn checks; starting them waits for slice 4, the deploy, and fails saying so). 3d done: `actserve bootstrap NAME [--dry-run]` and `apply` (all six steps, every declared file validated first, audit last decides the exit status). Starting Caddy/cdn: see 4d-4 | 3 done |
| `acteble deploy-caddy`, `cdn-deploy`, `edge-deploy` (+ `--dry-run`) | 4d-1 done: `actserve deploy-caddy NAME [--dry-run]` (Caddyfile of `origin/main`, backup, write aside then move, reload, restore on refusal; `CaddyDeployService`, `GitCaddyfileSource`). 4d-2 done: `actserve cdn-deploy NAME [--dry-run]` (`CdnDeployService`: mkdir, copy the Quadlet units and `minio-health.sh`, `daemon-reload`, `acteble_edge` network unit, `podman login`/`pull` and Podman secrets from the tmpfs env, `systemctl --user restart` of MinIO and the checkpoint, geo prefix opened; PR #35). 4d-3 done: `actserve edge-deploy NAME [--dry-run]` (`EdgeDeployService`; ported to Podman, see "Podman, no Docker" below; `ScpRemoteCopy` passes `-r`). The real `StackDeployPort` is done (4d-4, below) | 4d done |
| `acteble status`, `logs`, `cdn-status`, `cdn-logs` | 4a done: `actserve ps NAME api\|cdn`, `actserve logs NAME api\|cdn [SERVICE]` (`ObserveService`, `SshTerminal`) | 4a done |
| `acteble edge-status` | 4b done: `actserve edge-status NAME` (secret from actesskey on the server tmpfs, removed after; `SecretStorePort`, `with_remote_env`) | 4b done |
| `acteble deploy`, `rollback` | 4c done: `actserve deploy NAME [--dry-run]`, `actserve rollback NAME [TAG] [--dry-run]` (`DeployService`, `ScpRemoteCopy`) | 4c done |
| `acteble rotate jwt\|postgres\|minio\|all` | 5b done: `actserve rotate NAME TARGET [--dry-run] [--yes]` (`RotateService`: host, files and both secrets checked first; live DB password changed first through psql on stdin, secret untouched if it fails; `actesskey kv-set`; `machine autocommit`; redeploy; no value printed) | 5b done |
| `acteble edge-backup` | 5a done: `actserve edge-backup NAME [DIR]` (`EdgeBackupService`, `SshRemoteDownload`: archive streamed byte for byte into a private 0600 file, nothing left on failure) | 5a done |
| `acteble edge-audit` | 6 done: `actserve edge-audit NAME` (verb of `acteble`, same `FleetAuditService` as `audit`: host check, `sudo -n ./edge audit`, exit 0/1/3, 2 unknown) | 6 done |
| `acteble cdn-build-geo` | 7a done: `actserve cdn-build-geo NAME [--stage catalog\|tree\|publish\|all] [--dry-run]` (`GeoBuildService`: host, ghcr and cdn secrets and the signing key checked first; builder started detached with the env on the server tmpfs, removed after; logs followed to the build's exit) | 7a done |
| `acteble cdn-publish-geo` (tunnel to cdn_minio, runs `publish.sh` locally with the store keys) | none | 7b |
| Real `StackDeployPort` (bootstrap and apply start the edge and the cdn) | 4d-4 done: `ServiceStackDeploy` runs the edge and cdn deploy services on the verified target; `UnportedStackDeploy` removed (2026-11-25, no caller) | 4d-4 done |
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

## Podman, no Docker (owner, 2026-11-25: "option A")

The VPS runs rootless Podman with Quadlet units only (Docker purged, `remove-docker-from-vps`).
Every actserve command that runs `docker` on it moves to `podman` and `systemctl --user`, over the
plain ssh login user (no sudo, no XDG_RUNTIME_DIR; secrets by `podman secret create`, value on stdin).
Names: unit `acteble_<stack>-<service>.service`, container `systemd-acteble_<stack>-<service>`.
One PR per command; `domain/deploy.rs` keeps the Docker scripts until the last user is ported.

| Command | State |
|---|---|
| `ps`, `logs`, `edge-status`, `edge-audit` | session `remove-docker-from-vps`, branch `feat/podman-ps-audit` |
| `cdn-build-geo` | done (`podman run` with `--secret`, `domain/podman.rs::ghcr_wrapped`) |
| `edge-backup` | done (`podman ps` on `systemd-acteble_edge-caddy`, `podman run --volumes-from`) |
| `edge-deploy` | done (5 items: prepare, copy `Caddyfile` `sites/` `quadlet/`, units into `~/.config/containers/systemd`, binary by `podman pull/create/cp`, start by `./edge up --runtime podman`; needs the edge image built from `acteble_edge` `feat/podman-runtime`) |
| `cdn-deploy`, `runtime`, `stacks` | merged (#35, `feat/podman-bootstrap`) |
| `deploy`, `rollback`, `rotate` (api) | done on the units of `acteble_api` branch `feat/podman-quadlet` (not merged yet, so no real run before it is): `deploy` has items `prepare`, `copy` (10 files of `server/quadlet`), `restart` (secrets by `podman secret create --replace`, `api.env` for the settings, units started, API restarted); `rollback` retags `:TAG` as `:latest` and restarts, ghcr token only. `rotate` redeploys through `deploy`. `domain/deploy.rs` keeps `deploy_script`, `rollback_script`, `API_COMPOSE_FILES` until a search of callers allows removing them |
| `cdn-publish-geo` | frozen: `publish.sh` left `acteble_cdn`; Podman version after the CDN branch merges |
| Delegation from dotfiles (slice 8) | only for commands already ported and valid |

**Known limit before the first real API deploy (2026-11-26).** Behind rootless Podman 5.7.0 (Ubuntu 26.04, passt 0.0~git20260120) a published port on a bridge network goes through `rootlessport`, which rewrites the client address to `10.89.0.x`: Caddy has no `trusted_proxies` and the API's rate limiter keys on the first `X-Forwarded-For` hop, so every client would share one quota. `rootless_port_forwarder="pasta"` (Podman PR #28478, May 2026) keeps the real address, but Podman 5.7 does not know the option and the host repositories carry nothing newer. Until Podman is newer, or the edge leaves the bridge (`Network=pasta` plus upstreams published on `127.0.0.1`, an `acteble_edge` redesign), the API stays undeployed (`stacks.txt` `api|held`).

## Goal: actserve controls the whole VPS (owner, 2026-11-26)

Everything that runs on the VPS is deployed, observed and rolled back through an actserve verb; no
hand-run `ssh` for a reading. Measured gaps on 2026-11-26 (read-only, `podman ps` and `free -m`):

| On the VPS | Verb today | Gap |
|---|---|---|
| `acteble_cdn-minio`, `acteble_cdn-checkpoint` | `cdn-deploy`, `ps cdn`, `logs cdn` | none |
| `acteble_edge-caddy`, network `acteble_edge` | `edge-deploy`, `edge-status`, `edge-backup` | `ps` and `logs` take only `api\|cdn`, not `edge` |
| `acteble_api` stack (postgres, redis, minio, api) | `deploy`, `rollback`, `rotate`, `ps api`, `logs api` | not deployed yet on Podman; no `dev` instance |
| `actif-api` (container up 42 h, 256 MiB, 1 cpu) | none | repository `Actif` (crate `actif-api`, worktree `Actif-wt-biorcrux`, `docs/deployment.md`). Deployed by hand: `git archive` into `/opt/acteble/actif/releases/<commit>`, image built on the host (`podman build`, tag = commit), unit `~/.config/containers/systemd/actif-api.container` written outside the repository, which still documents Docker Compose. No actserve verb |
| CPU, memory, disk of the server | none (only `ssh_executor`, for the web API) | `actserve resources NAME`, slice 1, branch `feat/resources-verb` |
| Host hardening, runtime | `harden`, `runtime`, `stacks` | none known |

Next slices, in order: `ps`/`logs` for `edge`; an `actif-deploy NAME COMMIT` verb (export, build,
unit, start, `/health` check; its unit file must move into the `Actif` repository first, and a Rust
build on the VPS needs a memory check, since there is no swap); then the `prod`/`dev` instance argument for the API (VPS: 2 vCPU, 3.9 GiB RAM, no swap, 79 GB free;
measure with `resources` after the first real API deploy).

## Capacity target: one million simultaneous users (owner, 2026-11-26)

The owner's requirement: the platform must carry one million users at the same time. The VPS
measured on 2026-11-26 (`actserve resources acteble`) cannot: 2 vCPU, 3.9 GiB RAM (about 3 GiB
available, no swap), 79 GB disk free. Memory is the first limit. This section holds the target and
the plan; the numbers below are ASSUMPTIONS to be corrected by the owner, not measurements.

**Target, to be confirmed.** "Simultaneous" is not defined yet. Working assumption until the owner
says otherwise: 1 000 000 users connected at once, each sending one request every 30 s on average,
which is about 33 000 requests per second at the API, plus long-lived connections (gRPC streams,
LiveKit). Static files and published artifacts go through the CDN stack, not the API.

**Plan, in order.**
1. Owner fixes the target: connected at once, requests per second per user, share served by the CDN.
2. Measure what ONE node holds. Load tools (open source, to be checked on their official pages
   before use): `oha` for HTTP, `ghz` for gRPC. The measure runs against a release image (the dev
   stack runs `cargo watch`, a debug build, which says nothing about production), on a machine with
   known resources, with `actserve resources` read before and after.
3. From the per-node figure, size the number of nodes and the split: stateless API nodes behind a
   load balancer, a replicated database, the CDN in front for static content.
4. Only then change the topology; the single-VPS layout of today stays until the figure is known.

**Not done:** no load test has been run, no node count has been decided, nothing was changed on the
VPS.
