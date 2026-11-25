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

## Podman à la place de Docker — 2026-11-25 (`cdn-build-geo`)

- Décision du propriétaire : option A, Podman rootless avec Quadlet partout. Source : état du VPS relevé en lecture seule (units `acteble_edge-caddy`, `acteble_cdn-minio`, `acteble_cdn-checkpoint`, `actif-api` actifs, `docker` absent) et message de la session `remove-docker-from-vps`.
- `cdn-build-geo` ne lance plus que `podman` : `podman login ghcr.io` (`domain/podman.rs::ghcr_wrapped`), secret `acteble_cdn_manifest_signing_key` créé depuis l'env sur tmpfs (valeur sur stdin), `podman pull`, `podman run -d --name acteble-geo-build` avec `--secret` pour les trois valeurs, puis `podman logs -f`, `podman wait`, `podman rm`. Mêmes limites que l'unité Quadlet `acteble_cdn-geo-builder.container` (réseau `acteble_cdn`, `--cap-drop ALL`, 2800 Mo, `nofile` 262144, volume `acteble_cdn_geo_work`).
- Partage : `ps`, `logs`, `edge-status`, `edge-audit` sont sur la branche `feat/podman-ps-audit` de la session `remove-docker-from-vps`. Plan et ordre dans `ROADMAP.md`.
- Preuve : suite 492 verts, clippy `-D warnings` et fmt propres, mutants 17 testés, 0 survivant. Rien lancé sur le VPS réel ni sur le magasin de secrets. Les scripts sont vérifiés par les tests de chaîne ; leur exécution réelle sous Podman n'est pas essayée.

## Podman : `edge-backup` — 2026-11-25

- `BACKUP_SCRIPT` (`src/domain/backup.rs`) trouve le conteneur `systemd-acteble_edge-caddy` par `podman ps -q --filter name=^systemd-acteble_edge-caddy$` (échec s'il n'y en a pas), lit son volume `/data` en lecture seule par `podman run --rm --volumes-from "$c":ro --entrypoint tar` avec l'image donnée par `podman inspect`, et écrit un tar gzip sur stdout. Aucune commande Docker. Source des noms : message de la session `remove-docker-from-vps` et unité `acteble_edge-caddy.container`.
- Preuve : suite 495 verts, clippy `-D warnings` et fmt propres. Pas de mutant dans le diff (une constante). Rouge vu : 4 tests échouaient sur l'ancien script. Non lancé sur le VPS réel : `edge-backup` lit les clés privées des certificats, il reste au propriétaire de donner le go.

## Retrait de `UnportedStackDeploy` — 2026-11-25

Fichier, module et test retirés (`unported_stack_deploy.rs`, `unported_deploy_tests.rs`). Recherche des appelants : seuls `mod.rs`, son test et ces docs le citaient ; `stack_wiring.rs` utilise `ServiceStackDeploy`. Les mentions plus bas dans ce fichier sont historiques.

## Tranche 7a faite — 2026-11-25 (`cdn-build-geo`)

- `actserve cdn-build-geo NAME [--stage S] [--dry-run]` : `src/domain/geo_build.rs`, `src/application/geo_build_service.rs`, `src/geo_build_cli.rs`. Tests : `geo_build_domain_tests` (9), `geo_build_service_tests` (15), `cli_cdn_build_geo_tests` (10).
- Parité avec `cmd_cdn_build_geo` : mêmes commandes distantes (`compose --profile tools pull geo_builder`, `run -d --name acteble-geo-build`, `docker logs -f`, `docker wait`, `docker rm`), env sur le tmpfs `/dev/shm/acteble-cdn.env` retiré avant le suivi. Écarts voulus : le nom du serveur est obligatoire ; le hostname est vérifié d'abord (autre hôte : code 3) ; la clé de signature est contrôlée avec les deux secrets avant tout appel serveur ; pas d'option `-n` ni `--help` (seulement `--dry-run`) ; un build tué par signal est un échec ; le dry-run ne lit aucun secret.
- Preuve : suite 483 verts avant ajout du cas `--wat`, clippy `-D warnings` et fmt propres, mutants 32 testés, 0 survivant (un survivant tué par un test d'usage). Rien lancé sur le VPS réel ni sur le magasin de secrets.
- Reste de la tranche 7 : `cdn-publish-geo` (7b).

## Tranche 6 faite — 2026-11-25 (`edge-audit`)

- `actserve edge-audit NAME` est le verbe `acteble edge-audit` : même service que `audit` (`run_audit(verb, name)`), l'usage nomme le verbe tapé. Source : `src/fleet_cli.rs`, `src/main.rs`, `tests/cli_edge_audit_tests.rs` (6 tests).
- Parité avec `cmd_edge_audit` (bash) : même commande distante (`cd /opt/acteble/edge && sudo -n ./edge audit`). Écarts voulus : le nom du serveur est obligatoire (le bash visait `$SSH_HOST`) ; le hostname est vérifié avant (autre hôte : code 1, aucun audit lancé) ; codes 0/1/3 au lieu du code ssh brut.
- Preuve : suite 449 verts, clippy `-D warnings` et fmt propres, mutants 4 testés (3 pris, 1 non viable). Rien lancé sur le VPS réel.

## Tranche 5b faite — 2026-11-25 (`rotate`)

- `actserve rotate NAME jwt|postgres|minio|all [--dry-run] [--yes]` : un run réel exige `--yes` (sinon code 1, rien lancé). Ordre : hôte vérifié (refusé : code 3) ; utilisateur de base valide (`$ACTEBLE_DB_USER`, défaut `acteble_prod`) ; deploy à blanc (fichiers API présents) ; lecture des deux secrets (ghcr, `acteble-api-prod`) ; tirage des valeurs (`/dev/urandom`, jamais de repli plus faible) ; si postgres : `ALTER USER` via psql sur stdin avec `ON_ERROR_STOP` (secret intact et rien déployé si ça échoue) ; `actesskey kv-set acteble-api-prod` ; `machine autocommit` ; redéploiement. Tailles : jwt base64 de 64 octets, postgres hex de 32, minio user hex de 12, minio mot de passe hex de 32.
- Écarts avec `acteble-rotate.sh`, tous volontaires : le mot de passe passe par stdin et non par la ligne de commande ssh ; psql avec `ON_ERROR_STOP` (sans lui, une erreur SQL sur stdin sortirait en 0) ; fichiers locaux et secrets contrôlés avant le changement de la base ; une persistance échouée est signalée (code 1) mais n'empêche pas le redéploiement, car la base et le secret ont déjà changé ; aucune valeur affichée, les erreurs masquent les nouvelles valeurs (`***`) ; `--yes` explicite au lieu d'une question interactive.
- Nouveaux : `domain/encoding.rs`, `domain/rotate.rs`, ports `random`, `secret_writer`, `state_persist`, adaptateurs `OsRandom`, `ActesskeySecretWriter`, `MachineStatePersist`, `application/rotate_service.rs`, `rotate_cli.rs`. Faux ssh/scp/actesskey/machine seulement ; jamais lancé sur le VPS réel ni sur le vrai magasin de secrets.
- Mutants : 106 testés en 4 parties, 0 survivant (un survivant sur `BrokenPipe` tué par un test). Dette payée le 2026-11-25 : `describe` n'existe plus que dans `item_runner.rs` (cinq copies identiques retirées : deploy, caddy_deploy, cdn_deploy, edge_deploy, fleet_status).
- Reste : `edge-audit` autonome (6), `cdn-publish-geo` / `cdn-build-geo` (7), délégation depuis dotfiles puis retrait (8).

## Tranche 5a faite — 2026-11-24 (`edge-backup`)

- `actserve edge-backup NAME [DIR]` : vérifie l'hôte (refusé : code 3, rien téléchargé), puis exécute sur le serveur le script qui trouve le conteneur Caddy du projet `acteble_edge`, lit son volume `/data` en lecture seule et écrit une archive `tar.gz` sur stdout ; elle est écrite octet pour octet dans `DIR/acteble-edge-certs-AAAAMMJJTHHMMSS.tar.gz` (`DIR` par défaut `~/Backups/acteble-edge`). Fichier 0600, dossier créé ou resserré à 0700, jamais d'écrasement, aucun fichier laissé si le transfert échoue (code 1, « transfer failed, nothing saved »).
- Nouveaux : `domain/backup.rs`, `ports/outbound/remote_download.rs` + `SshRemoteDownload`, `application/edge_backup_service.rs`, `backup_cli.rs`. Faux ssh seulement (cible `fake-target.invalid`) ; jamais lancé sur le VPS réel. Le `SKILL.md` de dépôt est remis à jour (il désignait encore dotfiles pour les tranches 2 à 4).
- Reste : `rotate` (5b, demande un port d'écriture de secrets), `edge-audit` autonome (6), `cdn-publish-geo` / `cdn-build-geo` (7), délégation depuis dotfiles (8).

## Tranche 4d-4 faite — 2026-11-24 (vrai `StackDeployPort`)

- `bootstrap`, `apply`, `edge` et `stacks` démarrent maintenant l'edge et le cdn : `ServiceStackDeploy` (`application/stack_deployer.rs`) appelle `EdgeDeployService::deploy_verified` et `CdnDeployService::deploy_verified` (même déploiement que `edge-deploy` / `cdn-deploy`, sur une cible dont l'hôte est déjà vérifié par `StacksService`). Un échec d'étape, un secret verrouillé ou un fichier local manquant (`ACTEBLE_EDGE_DIR`, `ACTEBLE_CDN_DIR`) sortent en `Unavailable` avec le nom de l'étape ; toute autre pile (`api`) est refusée en nommant `actserve deploy`. Câblage CLI : `stack_wiring.rs`. `UnportedStackDeploy` n'est plus utilisé (conservé, à retirer dans une PR à part).
- Incident de test (même jour) : en passant au vrai port, `cli_stacks_tests` a lancé le vrai `scp` (le test ne fixait ni `DOTFILES_SCP` ni les dossiers locaux, et la cible `api.acteble` résout vers le vrai VPS via `~/.ssh/config`) : une copie réelle vers `/opt/acteble/edge/` a très probablement eu lieu, sans redémarrage de l'edge. Non vérifié côté serveur (lecture refusée par le classifieur). Correctif : `cli_stacks_tests` et `cli_bootstrap_tests` fixent `DOTFILES_SCP` et `ACTSERVE_ACTESSKEY` sur `/usr/bin/false`, des dossiers locaux inexistants, et une cible `fake-target.invalid`. Règle : un test du binaire ne doit atteindre ni scp, ni actesskey, ni un nom de `~/.ssh/config`.

## Tranche 4d-3 faite — 2026-11-24 (`edge-deploy`)

- `actserve edge-deploy NAME [--dry-run]` : valide que `Caddyfile`, `compose.yml` et `sites/` existent dans `$ACTEBLE_EDGE_DIR` (défaut `~/Source/Acteble/acteble_edge`), lit `ghcr-read-packages` et `acteble-edge-prod` AVANT de toucher au serveur, puis : `mkdir -p /opt/acteble/edge`, copie, binaire `/edge` sorti de `ghcr.io/acteble/acteble_edge:latest` (login ghcr déconnecté toujours, conteneur temporaire supprimé toujours, aucun socket monté), puis `./edge up --env-file` avec l'env en tmpfs supprimé quoi qu'il arrive. Arrêt au premier échec ; `--dry-run` ne lit aucun secret et n'exécute rien.
- `ScpRemoteCopy` passe maintenant `-r` (un dossier est copié en entier) ; les tests des tranches 4c et 4d restent verts.
- Nouveaux : `domain/edge_deploy.rs`, `application/edge_deploy_service.rs`, `edge_cli.rs`. Faux ssh, scp et actesskey seulement ; jamais lancé sur le VPS réel. Les trois commandes de 4d existent ; reste le vrai `StackDeployPort` (pour que `bootstrap` démarre l'edge et le cdn), puis la délégation depuis dotfiles.

## `cdn-deploy` en Podman + Quadlet — 2026-11-25

- Remplace `compose.yml` : valide les 7 fichiers `quadlet/*` et `quadlet/minio-health.sh` de `$ACTEBLE_CDN_DIR`, les copie (unités dans `~/.config/containers/systemd`, script dans `/opt/acteble/cdn`), puis `systemctl --user daemon-reload` et le réseau `acteble_edge` (démarré par son unité s'il manque). L'étape `start` fait `podman login ghcr.io` (déconnexion toujours), `podman pull`, crée les secrets Podman (`acteble_cdn_*`) depuis l'env en tmpfs (la clé de signature seulement si elle est présente), puis `systemctl --user restart` de MinIO et du checkpoint. L'étape `geo` utilise `podman run`. Les étapes et leurs noms ne changent pas.
- Jamais lancé sur le VPS réel. `cdn_publisher` et `geo_builder` sont copiés mais pas démarrés par ce déploiement. `ghcr_wrapped` (Docker) reste pour `deploy`, `edge-deploy` et `geo-build`, pas encore migrés.

## Tranche 4d-2 faite — 2026-11-24 (`cdn-deploy`)

- `actserve cdn-deploy NAME [--dry-run]` : valide que `compose.yml` existe dans `$ACTEBLE_CDN_DIR` (défaut `~/Source/Acteble/acteble_cdn`), lit les secrets `ghcr-read-packages` et `acteble-cdn-prod` AVANT de toucher au serveur, puis : `mkdir -p /opt/acteble/cdn`, copie par scp, réseau `acteble_edge` créé seulement s'il manque (un échec de création n'est plus masqué, contrairement au bash), `pull` puis `up -d --remove-orphans` avec login ghcr (déconnexion toujours) et env en tmpfs, puis ouverture du préfixe `geo/` en lecture anonyme par un client mc jetable en lecture seule (10 essais). L'env est écrit puis supprimé pour chacune des deux étapes. Arrêt au premier échec ; `--dry-run` ne lit aucun secret et n'exécute rien.
- Nouveaux : `domain/cdn.rs`, `application/cdn_deploy_service.rs`, `cdn_cli.rs`. Faux ssh, scp et actesskey seulement ; jamais lancé sur le VPS réel. Reste de 4d : `edge-deploy`, `StackDeployPort` réel.

## Tranche 4d-1 faite — 2026-11-24 (`deploy-caddy`)

- `actserve deploy-caddy NAME [--dry-run]` : lit le `Caddyfile` de `origin/main` du dépôt `$ACTEBLE_API_DIR` (jamais l'arbre de travail ; `git` = `$ACTSERVE_GIT`, défaut `git`), lit le secret `acteble-api-prod` avant de toucher au serveur, sauvegarde le Caddyfile distant en `.bak`, écrit le nouveau en `.new` puis le déplace (un fichier à moitié écrit ne remplace jamais le vivant), recharge Caddy avec l'env en tmpfs (supprimé quoi qu'il arrive) ; si Caddy refuse, restaure la sauvegarde et sort en 1. `--dry-run` ne lance ni git, ni secret, ni ssh.
- Nouveaux : `domain/caddy.rs`, `ports/outbound/caddyfile_source.rs` + `GitCaddyfileSource`, `application/caddy_deploy_service.rs`, `caddy_cli.rs`. Plus sûr que le bash : pas de fichier temporaire local, et un hôte refusé arrête tout (code 3).
- Faux ssh, git et actesskey seulement ; jamais lancé sur le VPS réel. Reste de 4d : `cdn-deploy`, `edge-deploy`, `StackDeployPort` réel.

## Tranche 4c faite — 2026-11-24 (`deploy`, `rollback` de l'API)

- `actserve deploy NAME [--dry-run]` : valide d'abord que les 4 fichiers (`docker-compose*.yml`, `Caddyfile`) existent dans `$ACTEBLE_API_DIR` (défaut `~/Source/Acteble/acteble_api`), lit les secrets `ghcr-read-packages` et `acteble-api-prod` AVANT de toucher au serveur, copie par scp (`$DOTFILES_SCP`), puis redémarre : env en tmpfs (supprimé quoi qu'il arrive), `docker login` ghcr avec le jeton sur stdin et `logout` toujours, `image tag :previous`, `pull`, `up -d`, `prune`. Arrêt au premier échec ; `--dry-run` ne lit aucun secret et n'exécute rien.
- `actserve rollback NAME [TAG] [--dry-run]` : redémarre sur `previous` (déjà sur le serveur) ou tire le tag demandé (`[A-Za-z0-9_.-]`, validé). Mêmes garanties.
- Nouveaux : `domain/deploy.rs`, `ports/outbound/remote_copy.rs` + `ScpRemoteCopy`, `application/deploy_service.rs`, `deploy_cli.rs` ; `remote_env` gagne `with_remote_env_value` (valeur déjà lue, stdin pour la commande).
- Faux ssh, scp et actesskey seulement ; jamais lancé sur le VPS réel (le propriétaire doit donner son accord). Reste de la tranche 4 (4d) : `deploy-caddy` (depuis `origin/main`, sauvegarde et restauration du Caddyfile), `cdn-deploy`, `edge-deploy`.

## Tranche 4b faite — 2026-11-23 (socle des secrets, `edge-status`)

- `actserve edge-status NAME` : lit le secret `acteble-edge-prod` avec `actesskey get` (`$ACTSERVE_ACTESSKEY`, défaut `actesskey`), l'envoie sur stdin vers `/dev/shm/acteble-edge.env` (umask 077), lance `./edge status --env-file …`, puis supprime le fichier même si la commande échoue ; un fichier non supprimé ou non écrit est une erreur, jamais un silence. Hôte refusé avant la lecture du secret (code 3).
- Socle réutilisé par les déploiements : `domain/secret.rs` (`SecretValue` qui s'affiche `***`, noms et chemins tmpfs validés), `SecretStorePort` + `ActesskeySecretStore` (UTF-8 strict, valeur jamais dans une erreur), `application/remote_env.rs` (`with_remote_env`), `EdgeStatusService`.
- Faux ssh et faux actesskey seulement. Reste de la tranche 4 : `deploy`, `deploy-caddy`, `cdn-deploy`, `edge-deploy` (copie de fichiers par scp, login ghcr avec `ghcr-read-packages`), `rollback`.

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

- `actserve runtime NAME [--dry-run]` : hôte vérifié, `dirs.txt` (chemins absolus sans `..`, propriétaires valides) validé avant tout appel distant, puis podman (installé par apt), dirs. Plus d'item Docker ni `logs` (2026-11-25) : les services sont des unités Quadlet rootless qui journalisent dans le journal. Moteur commun check-puis-apply : `application/item_runner.rs`, partagé avec `harden`.
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
