# actserve — état au 2026-11-21

## Rôle

Orchestrateur Rust (Axum, architecture hexagonale) de l'administration de serveurs : serveurs, DNS, registrars, TLS (ACME), WireGuard, moteurs OpenTofu / Ansible / Crossplane, catalogue de recettes. Expose une API REST pour l'UI biorcrux. Source : `README.md`, `src/{domain,application,ports,adapters}`.

## État vérifié (2026-11-21)

- `cargo test` : 79 tests passent, 0 échec (mesuré via `dotfiles heavy run`).
- `cargo clippy --all-targets -- -D warnings` : propre. Les 4 `too_many_arguments` sont levés : `CatalogItemSpec`, `AdminPackageSpec`, `OrchestrationPorts`, `Engines` remplacent les listes d'arguments (appelants et tests mis à jour).
- `cargo fmt --check` : propre, mais `cargo fmt` a reformaté ~60 fichiers qui dérivaient déjà : à isoler dans son propre commit.
- Branche `feat/fleet-admin`, rien de commité.

## Direction (owner, 2026-11-21)

actserve doit reprendre toute l'administration du VPS et de son réseau que `dotfiles` assure. Inventaire et tranches : `ROADMAP.md`. Frontière (owner) : actserve administre le fonctionnement, actparc répertorie qui contient quoi.

## À faire

1. Commit en deux temps (fmt seul, puis refactor), PR.
2. Tranche 1 du `ROADMAP.md` : `status` des serveurs lus depuis l'inventaire actparc (test d'abord).

## Décisions

- 2026-11-18 — renommage servai → actserve (commit 1b1339d).
