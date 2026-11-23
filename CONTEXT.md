# actserve — état au 2026-11-23

## Rôle

Orchestrateur Rust (Axum, architecture hexagonale) de l'administration de serveurs : serveurs, DNS, registrars, TLS (ACME), WireGuard, moteurs OpenTofu / Ansible / Crossplane, catalogue de recettes. Expose une API REST pour l'UI biorcrux. Source : `README.md`, `src/{domain,application,ports,adapters}`.

## État vérifié (2026-11-21)

- `cargo test` : 79 tests passent, 0 échec (mesuré via `dotfiles heavy run`).
- `cargo clippy --all-targets -- -D warnings` : propre. Les 4 `too_many_arguments` sont levés : `CatalogItemSpec`, `AdminPackageSpec`, `OrchestrationPorts`, `Engines` remplacent les listes d'arguments (appelants et tests mis à jour).
- `cargo fmt --check` : propre, mais `cargo fmt` a reformaté ~60 fichiers qui dérivaient déjà : à isoler dans son propre commit.
- PR #1 (refactor + fmt) ouverte : https://github.com/grosjeanbaptiste/actserve/pull/1
- Tranche 1 (branche `feat/server-status`) : `actserve status [NAME]` lit `$DOTFILES_STATE_ROOT/servers.txt` (défaut `~/machine`), sonde `ssh -o BatchMode=yes <cible> hostname` (`$DOTFILES_SSH`), affiche ok / unreachable / wrong-host, sort 0 seulement si tout est ok. 97 tests verts, clippy et fmt propres. Mesuré sur le VPS réel le 2026-11-21 : `acteble: ok`.
- Comparé à `dotfiles servers status` le 2026-11-21 : même verdict (`acteble` ok), format de ligne différent (`nom: ok` contre colonnes). Pas de mutation testing lancé.

## Tranche 4a faite — 2026-11-23

- `actserve ps NAME api|cdn` (`docker compose ps` dans `/opt/acteble/<stack>`) et `actserve logs NAME api|cdn [SERVICE]` (suit les 100 dernières lignes sur le terminal via `ssh -t` ; défaut `api` / `cdn_checkpoint`). Remplacent `acteble status|logs|cdn-status|cdn-logs`. Plus strict que le bash : refus si l'hôte n'est pas celui attendu (code 3) ; nom de service validé (`[A-Za-z0-9_.-]`) car interpolé dans un shell. Codes : celui de la commande distante, 3 hôte refusé, 2 nom inconnu ou usage, 1 pile ou service refusé.
- Nouveaux : `domain/observe.rs`, `application/observe_service.rs`, `ports/outbound/remote_terminal.rs`, `adapters/outbound/system/ssh_terminal.rs`, `observe_cli.rs`.
- Faux ssh seulement. Reste de la tranche 4 : `edge-status` (a besoin du secret `acteble-edge-prod`, donc d'un port secrets vers actesskey), `deploy`, `deploy-caddy`, `cdn-deploy`, `edge-deploy`, `rollback`.

## Tranche 3d faite — 2026-11-23 (tranche 3 complète)

- `actserve bootstrap NAME [--dry-run]` et `apply NAME` : valide tous les fichiers déclarés avant le premier appel distant (mieux que le bash), puis reach, harden, runtime, edge, stacks, audit ; arrêt au premier échec ; code 0 seulement si l'audit est conforme ; `--dry-run` n'applique rien et n'audite pas. `BootstrapService` compose les services des tranches 3a-3c.
- Limite connue : démarrer Caddy ou cdn attend la tranche 4 (`StackDeployPort`) ; un serveur à démarrer échoue en le disant.
- Faux ssh seulement ; jamais lancé sur le VPS réel. Mutants tous attrapés (un timeout sur la branche `main`).
- Suite : dotfiles délègue `servers audit`/`bootstrap`/`apply` à actserve, puis retire la commande (PR séparées, recherche des appelants) ; tranche 4 (deploy, rollback, logs).

## Tranche 3c faite — 2026-11-23

- `actserve edge NAME [--dry-run]` (réseau `acteble_edge`, Caddy) et `actserve stacks NAME [--dry-run]` (`stacks.txt` : cdn run ou held, api held seulement). Les contrôles et la création du réseau sont portés. Démarrer Caddy ou cdn = le déploiement, tranche 4 : `UnportedStackDeploy` ne démarre rien et échoue en le disant (`StackDeployPort` est le point d'accroche de la tranche 4). Aucun passage par `dotfiles`.
- Faux ssh seulement ; jamais lancé sur le VPS réel. Mutants tous attrapés (timeouts sur les branches de `main`).
- Reste de la tranche 3 : composition `bootstrap`/`apply` (reach, harden, runtime, edge, stacks, audit ; 0 seulement si l'audit passe).

## Tranche 3b faite — 2026-11-23

- `actserve runtime NAME [--dry-run]` : hôte vérifié, `docker-daemon.json` et `dirs.txt` (chemins absolus sans `..`, propriétaires valides) validés avant tout appel distant, puis docker, logs (installé, effectif au prochain redémarrage de Docker, rien n'est redémarré), dirs. Moteur commun check-puis-apply : `application/item_runner.rs`, partagé avec `harden`.
- Faux ssh seulement ; jamais lancé sur le VPS réel. Mutants tous attrapés (un timeout sur la branche `main`).
- Reste : edge, stacks, composition `bootstrap`/`apply`.

## Tranche 3a faite — 2026-11-23

- `actserve harden NAME [--dry-run]` : hôte vérifié d'abord (refus code 3), données déclarées (`servers/<name>/sshd.conf`, `ports.txt` avec 22/tcp) validées avant tout appel distant, puis reach, sshd (ControlMaster, `sshd -t`, reload, preuve par connexion neuve, rollback), ufw, fail2ban, upgrades. Chaque étape : contrôle puis application seulement si le contrôle échoue ; arrêt au premier échec. Codes 0 / 1 / 2 / 3.
- Testé avec de faux ssh uniquement ; jamais lancé sur le VPS réel (go du propriétaire requis). Mutants : tous attrapés (un timeout sur la branche `main`).
- Reste de la tranche 3 : runtime, edge, stacks, composition `bootstrap`/`apply`.

## Tranche 2 faite — 2026-11-23

- `actserve audit NAME` : contrôle du hostname puis `edge audit` en ssh avec sudo ; sortie 0 conforme, 1 écart ou autre hôte, 3 illisible, 2 nom inconnu ou absent. Tests : `tests/fleet_audit_tests.rs`, `tests/cli_audit_tests.rs` (binaire réel, ssh remplacé). Mutation : aucun survivant (2 timeouts = le binaire démarrerait le serveur HTTP si une branche de `main` disparaissait).
- PR #7 (`wrong-host` nomme l'attendu) et #8 (`--` avant la cible ssh, réponse nettoyée) mergées.
- Décision du propriétaire (2026-11-23) : plus rien pour les serveurs ne passe par `dotfiles` ; voir `ROADMAP.md`.

## Direction (owner, 2026-11-21)

actserve doit reprendre toute l'administration du VPS et de son réseau que `dotfiles` assure. Inventaire et tranches : `ROADMAP.md`. Frontière (owner) : actserve administre le fonctionnement, actparc répertorie qui contient quoi.

## Responsabilités (2026-11-21, propriétaire, dites en session)

- actserve administre le fonctionnement des hôtes distants (status, audit, bootstrap, deploy, rotate, backup) ; actparc répertorie qui contient quoi et fournit l'inventaire des serveurs.
- actserve reprend aussi la supervision des services à la demande de `services.txt` (démarrage, arrêt, logs), Baba ADR-045 acceptée le 2026-11-21 ; aucun code déplacé, tranche à planifier dans `ROADMAP.md`. À vérifier : le port d'exécution convient-il à des processus locaux ?
- actserve n'installe pas de logiciel : actinstall le fait, sur demande d'actparc.
- Pour se connecter à un compte, actserve demande le identifiant à actesskey au moment du besoin et ne le garde pas. En attendant actesskey : `dotfiles secret get`.

## À faire

1. Merge de la PR #1 (propriétaire), puis PR de la tranche 1.
2. Faire appeler `actserve status` par `dotfiles servers status` ; brancher l'inventaire sur actparc.
3. Tranche 2 : `audit`.

## Décisions

- 2026-11-18 — renommage servai → actserve (commit 1b1339d).
