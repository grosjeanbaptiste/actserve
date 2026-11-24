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
| `servers audit` / `acteble edge-audit` (ssh, ports, ufw, updates, fail2ban, containers, ADR-123) | none | 2 |
| `servers bootstrap` / `apply` (reach, harden, runtime, edge, stacks, audit) | partial: Ansible engine | 3 |
| `acteble deploy`, `deploy-caddy`, `cdn-deploy`, `edge-deploy` (+ `--dry-run`) | none (OpenTofu/Ansible/Crossplane engines only) | 4 |
| `acteble rollback`, `status`, `logs`, `edge-status`, `cdn-status`, `cdn-logs` | none | 4 |
| `acteble rotate jwt\|postgres\|minio\|all` | none; secrets belong to actesskey | 5 |
| `acteble edge-backup` | none | 5 |
| DNS, TLS, WireGuard | present (domain, adapters, API) | verify |
| `service start\|stop\|status` (local, no launchd) | out: local processes go to actparc / actautoma | - |

## Boundary (owner, 2026-11-21)

actserve administers how things run (status, audit, bootstrap, deploy, rotate,
backup). actparc lists who holds what (inventory of machines, volumes,
servers). Slice 1 therefore reads the server inventory from actparc instead of
owning `servers.txt`.

## Rules

No silent failure, `--dry-run` on every mutating action, nothing removed,
secrets via actesskey never printed, 100 % TDD with failure-path tests.

## Runtime: Podman + Quadlet (owner, 2026-11-23)

The VPS runs its services as rootless Podman Quadlet units (`.container`,
`.network`, `.volume`) in the `administrator` user session, not Docker Compose.
Checks are `systemctl --user is-active <unit>` and `podman network exists`;
logs come from `journalctl --user`.

Done on `acteble` (2026-11-23): Podman 5.7.0, sysctl `ip_unprivileged_port_start=80`,
linger on, units of `acteble_edge`, `acteble_cdn` and `actif` running, volumes
copied, secrets as Podman secrets. Docker containers are stopped, not removed:
the way back is `systemctl --user stop` the units, then `docker start`.

The bootstrap plan has no Docker left: the `runtime podman` item checks
`podman --version` and installs it (`apt-get install -y podman`); the `logs` item
and `docker-daemon.json` are gone (Quadlet units log to the journal).

Open: the units are installed by hand (deploy is slice 4); secrets are created
from `.env`, not yet from actesskey; `cdn_publisher` and `geo_builder` are not
ported; the older catalog and registry (Docker Compose recipes, `RepoKind::DockerCompose`)
still speak Docker. The VPS still holds the stopped Docker containers.
