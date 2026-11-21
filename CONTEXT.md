# actserve — état au 2026-11-21

## Rôle

Orchestrateur Rust (Axum, architecture hexagonale) de l'administration de serveurs : serveurs, DNS, registrars, TLS (ACME), WireGuard, moteurs OpenTofu / Ansible / Crossplane, catalogue de recettes. Expose une API REST pour l'UI biorcrux. Source : `README.md`, `src/{domain,application,ports,adapters}`.

## État vérifié (2026-11-21)

- `cargo test` : 79 tests passent, 0 échec (mesuré via `dotfiles heavy run`).
- `cargo clippy --all-targets -- -D warnings` : propre. Les 4 `too_many_arguments` sont levés : `CatalogItemSpec`, `AdminPackageSpec`, `OrchestrationPorts`, `Engines` remplacent les listes d'arguments (appelants et tests mis à jour).
- `cargo fmt --check` : propre, mais `cargo fmt` a reformaté ~60 fichiers qui dérivaient déjà : à isoler dans son propre commit.
- PR #1 (refactor + fmt) ouverte : https://github.com/grosjeanbaptiste/actserve/pull/1
- Tranche 1 (branche `feat/server-status`) : `actserve status [NAME]` lit `$DOTFILES_STATE_ROOT/servers.txt` (défaut `~/machine`), sonde `ssh -o BatchMode=yes <cible> hostname` (`$DOTFILES_SSH`), affiche ok / unreachable / wrong-host, sort 0 seulement si tout est ok. 97 tests verts, clippy et fmt propres. Mesuré sur le VPS réel le 2026-11-21 : `acteble: ok`.
- Pas de mutation testing lancé ; pas encore comparé systématiquement à la sortie de `dotfiles servers status` (la commande bash n'a pas rendu la main en 60 s).

## Direction (owner, 2026-11-21)

actserve doit reprendre toute l'administration du VPS et de son réseau que `dotfiles` assure. Inventaire et tranches : `ROADMAP.md`. Frontière (owner) : actserve administre le fonctionnement, actparc répertorie qui contient quoi.

## À faire

1. Merge de la PR #1 (propriétaire), puis PR de la tranche 1.
2. Faire appeler `actserve status` par `dotfiles servers status` ; brancher l'inventaire sur actparc.
3. Tranche 2 : `audit`.

## Décisions

- 2026-11-18 — renommage servai → actserve (commit 1b1339d).
