# actserve — état au 2026-11-21

## Rôle

Orchestrateur Rust (Axum, architecture hexagonale) de l'administration de serveurs : serveurs, DNS, registrars, TLS (ACME), WireGuard, moteurs OpenTofu / Ansible / Crossplane, catalogue de recettes. Expose une API REST pour l'UI biorcrux. Source : `README.md`, `src/{domain,application,ports,adapters}`.

## État vérifié (2026-11-21)

- `cargo test` : 79 tests passent, 0 échec (mesuré via `dotfiles heavy run`).
- `cargo clippy --all-targets -- -D warnings` : propre. Les 4 `too_many_arguments` sont levés : `CatalogItemSpec`, `AdminPackageSpec`, `OrchestrationPorts`, `Engines` remplacent les listes d'arguments (appelants et tests mis à jour).
- `cargo fmt --check` : propre, mais `cargo fmt` a reformaté ~60 fichiers qui dérivaient déjà : à isoler dans son propre commit.
- PR #1 (refactor + fmt) ouverte : https://github.com/grosjeanbaptiste/actserve/pull/1
- Tranche 1 (branche `feat/server-status`) : `actserve status [NAME]` lit `$DOTFILES_STATE_ROOT/servers.txt` (défaut `~/machine`), sonde `ssh -o BatchMode=yes <cible> hostname` (`$DOTFILES_SSH`), affiche ok / unreachable / wrong-host, sort 0 seulement si tout est ok. 97 tests verts, clippy et fmt propres. Mesuré sur le VPS réel le 2026-11-21 : `acteble: ok`.
- Comparé à `dotfiles servers status` le 2026-11-21 : même verdict (`acteble` ok), format de ligne différent (`nom: ok` contre colonnes). Pas de mutation testing lancé.

## Direction (owner, 2026-11-21)

actserve doit reprendre toute l'administration du VPS et de son réseau que `dotfiles` assure. Inventaire et tranches : `ROADMAP.md`. Frontière (owner) : actserve administre le fonctionnement, actparc répertorie qui contient quoi.

## Responsabilités (2026-11-21, propriétaire, dites en session)

- actserve administre le fonctionnement des hôtes distants (status, audit, bootstrap, deploy, rotate, backup) ; actparc répertorie qui contient quoi et fournit l'inventaire des serveurs.
- actserve n'installe pas de logiciel : actinstall le fait, sur demande d'actparc.
- Pour se connecter à un compte, actserve demande le identifiant à actesskey au moment du besoin et ne le garde pas. En attendant actesskey : `dotfiles secret get`.

## À faire

1. Merge de la PR #1 (propriétaire), puis PR de la tranche 1.
2. Faire appeler `actserve status` par `dotfiles servers status` ; brancher l'inventaire sur actparc.
3. Tranche 2 : `audit`.

## Décisions

- 2026-11-18 — renommage servai → actserve (commit 1b1339d).
