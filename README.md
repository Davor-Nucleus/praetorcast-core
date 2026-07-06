# PraetorCast-core

Ce projet est un serveur web en Rust utilisant **Actix-web** et **Askama** (templates HTML compilés) qui sert de backend pour les overlays OBS **PraetorCast**. Il expose des pages d'affichage, des pages de configuration, des API REST et des WebSockets pour le pilotage en temps réel.

---

## Démarrage rapide

```sh
# 1. Installer Rust (si pas déjà fait)
# https://rustup.rs/

# 2. Compiler le projet
cargo build

# 3. Lancer le serveur
cargo run

# 4. Ouvrir le navigateur
# http://127.0.0.1:3000/ (ou le port défini dans env.json → PORT)
```

---

## Structure du projet

```
praetorcast-core/
├── Cargo.toml                    # Dépendances (actix-web, askama, obws, reqwest, etc.)
├── src/
│   ├── main.rs                   # Point d'entrée, déclaration de toutes les routes
│   ├── twitch.rs                 # Intégration Twitch EventSub (WebSocket followers)
│   ├── models/
│   │   ├── mod.rs                # Réexport des modules models
│   │   ├── config.rs             # Configuration générale (env.json)
│   │   ├── banner.rs             # Modèle + CRUD des cartes de bannière
│   │   └── scheduler.rs          # Modèle + CRUD du planning hebdomadaire
│   └── controllers/
│       ├── mod.rs                # Réexport des contrôleurs
│       ├── display.rs            # Pages d'affichage (overlays OBS)
│       ├── banner_controller.rs  # API + page config bannière
│       ├── scheduler_controller.rs # API + page config planning
│       ├── music_controller.rs   # Page config musique/soundboard
│       ├── twitch_controller.rs  # WebSocket état Twitch
│       └── obs_controller.rs     # API + WebSocket filtre Limiter OBS
├── templates/                    # Templates Askama (HTML compilés)
├── data/                         # Données persistantes (banner.json, scheduler.json, etc.)
└── public/                       # Fichiers statiques (images, polices, JS, etc.)
```

---

## Configuration

### Fichier `env.json`

Le serveur lit la configuration depuis `env.json` à la racine du projet (créé automatiquement avec les valeurs par défaut au premier lancement).

| Clé | Défaut | Description |
|-----|--------|-------------|
| `PORT` | `3000` | Port du serveur HTTP |
| `TITLE_FONT` | `""` | Chemin vers une police personnalisée (ex: `/public/font/monolisa.woff2`) |
| `TWITCH_CHANNEL_NAME` | `""` | Nom de la chaîne Twitch (pour EventSub et les pages chat) |
| `TWITCH_CLIENT_ID` | `""` | Client ID de l'application Twitch |
| `TWITCH_OAUTH_TOKEN` | `""` | Token OAuth Twitch (préfixé `oauth:` ou token brut) |
| `MUSIC_PORT` | `6600` | Port du serveur MPD/music |
| `SOUNDBOARD_SHORTCUTS` | `{}` | Raccourcis clavier pour le soundboard |
| `DISCORD_PORT` | `8080` | Port du serveur Discord Presence |
| `WS_YOUTUBE_CHAT_PORT` | `5050` | Port du WebSocket YouTube Chat |
| `OBS_WS_HOST` | `localhost` | Hôte du serveur obs-websocket |
| `OBS_WS_PORT` | `4455` | Port obs-websocket |
| `OBS_WS_PASSWORD` | `""` | Mot de passe (vide = pas d'authentification) |
| `OBS_AUDIO_SOURCE` | `music` | Nom exact de la source audio à limiter |
| `OBS_LIMITER_FILTER` | `Limiter` | Nom du filtre Limiter (créé automatiquement s'il manque) |

---

## Routes

### Pages d'affichage (overlays OBS)

| Méthode | Route | Description | Paramètres |
|---------|-------|-------------|------------|
| GET | `/` | Page d'accueil / index | — |
| GET | `/clock` | Horloge pour overlay OBS | `?hour=true&minute=true&second=true` (booléens) |
| GET | `/banner` | Bannière tournante (cartes) | — |
| GET | `/music-current` | Musique en cours de lecture | — |
| GET | `/emote-corner` | Émoticônes / emote wall Twitch | — |
| GET | `/discord-presence` | Présence Discord | — |
| GET | `/followers-info` | Informations followers Twitch | — |
| GET | `/chat-horizontal` | Chat Twitch horizontal | — |
| GET | `/chat-vertical` | Chat Twitch vertical | — |
| GET | `/chat-youtube` | Chat YouTube | — |

### Pages de configuration

| Méthode | Route | Description |
|---------|-------|-------------|
| GET | `/music-config` | Configuration musique / soundboard / limiteur OBS |
| GET | `/banner-config` | Configuration des cartes de bannière |
| GET | `/scheduler` | Configuration du planning hebdomadaire |

### API Banner

| Méthode | Route | Description |
|---------|-------|-------------|
| GET | `/api/banner-config` | Récupère la configuration des cartes |
| POST | `/api/banner-config` | Sauvegarde la configuration des cartes |
| POST | `/api/banner-upload` | Upload d'une image pour le banner |
| GET | `/api/banner_ws` | **WebSocket** — pousse la config banner en temps réel |

### API Scheduler

| Méthode | Route | Description |
|---------|-------|-------------|
| GET | `/api/scheduler-config` | Récupère le planning |
| POST | `/api/scheduler-config` | Sauvegarde le planning |
| POST | `/api/scheduler-upload` | Upload d'une image de couverture |
| POST | `/api/scheduler-background-upload` | Upload d'une image de fond |

### API Twitch

| Méthode | Route | Description |
|---------|-------|-------------|
| GET | `/api/twitch_ws` | **WebSocket** — état des followers Twitch en temps réel |

### API OBS (filtre Limiter)

| Méthode | Route | Description |
|---------|-------|-------------|
| GET | `/api/obs/limiter` | État courant du filtre |
| GET / POST | `/api/obs/limiter/add` | Augmente le seuil de +1 dB |
| GET / POST | `/api/obs/limiter/subtract` | Diminue le seuil de −1 dB |
| GET | `/api/obs/limiter/toggle` | Active/désactive le filtre |
| GET | `/api/obs/limiter_ws` | **WebSocket** — état du limiteur en continu |

> Réponse API OBS : `{ "enabled": bool, "threshold": float }` ou `503` si OBS est injoignable.

### Fichiers statiques

| Route | Description |
|-------|-------------|
| `GET /public/**` | Serve les fichiers statiques (images, polices, JS, CSS) |

---

## WebSockets

trois WebSockets permettent de pousser les changements en temps réel vers les overlays OBS, sans rafraîchissement manuel :

| Route | Flux poussé | Fréquence |
|-------|-------------|-----------|
| `/api/banner_ws` | Configuration du banner (liste des cartes JSON) | 1 seconde (sur changement) |
| `/api/twitch_ws` | État Twitch : `{ total_followers, last_follower, connected }` | 500 ms (sur changement) |
| `/api/obs/limiter_ws` | État du limiteur : `{ enabled, threshold }` (ou `null` si OBS déconnecté) | 1 seconde (sur changement) |

---

## Intégration Twitch EventSub

Le module `twitch.rs` se connecte en **WebSocket** à l'EventSub API Twitch (`wss://eventsub.wss.twitch.tv/ws`) et souscrit automatiquement aux événements `channel.follow`.

- **Connexion persistante** avec reconnexion automatique (toutes les 5s)
- **Détection de token invalide** (HTTP 401 → message d'erreur explicite)
- **État temps réel** : `total_followers` mis à jour à chaque nouveau follower
- **Reconnexion à chaud** gérée via `session_reconnect` de Twitch

---

## Fonctionnalités détaillées

### OBS Limiter

- Pilotage du filtre **Limiter** d'OBS (obs-websocket v5) appliqué à une source audio
- Création **automatique** du filtre s'il n'existe pas encore
- Modification du seuil en dB (clampé entre −60 dB et 0 dB, pas de 1 dB)
- Activation/désactivation du filtre
- WebSocket temps réel qui reflète aussi les changements faits **directement dans OBS**
- Connexion obs-websocket persistante (une seule connexion réutilisée)

### Banner

- Système de cartes avec texte, image, transition et durée d'affichage
- Normalisation automatique des chemins d'images (`banner/img.png` → `/public/banner/img.png`)
- Fallback : `data/banner.json` → `data/banner.example.json` → config vide
- Durée par carte optionnelle (`durationMs`), avec valeur par défaut côté overlay
- Upload d'images avec UUID pour éviter les collisions de noms

### Scheduler (Planning)

- Planning hebdomadaire avec 7 jours (index 0–6)
- Chaque jour : titre, date, horaire, image de couverture
- Image de fond optionnelle
- Upload d'images de couverture et de fond
- Normalisation automatique des chemins

### Affichages (overlays OBS)

- **Horloge** : format configurable (heures/minutes/secondes) via query string
- **Bannière** : rotation automatique des cartes
- **Musique** : affichage du morceau en cours (via port MPD)
- **Emote Corner** : affichage des émoticônes Twitch
- **Discord Presence** : statut Discord via WebSocket externe
- **Followers Info** : compteur de followers + dernier follower
- **Chat** : chat Twitch (horizontal/vertical) + chat YouTube
- **Police personnalisée** : configurable via `TITLE_FONT` dans `env.json`

---

## Tests

Des tests unitaires sont intégrés directement dans les fichiers sources (`#[cfg(test)] mod tests`).

**Total : 24 tests**

### Modèle Banner (`models::banner`) — 9 tests

| Test | Description |
|------|-------------|
| `test_banner_card_serialization_roundtrip` | Sérialisation/désérialisation complète d'une carte |
| `test_banner_card_optional_duration` | Champ `durationMs` optionnel |
| `test_banner_card_optional_id` | Champ `id` optionnel |
| `test_normalize_path_already_public` | Chemin déjà préfixé `/public` |
| `test_normalize_path_banner_slash` | Chemin avec `/banner/` |
| `test_normalize_path_banner_no_slash` | Chemin avec `banner/` |
| `test_normalize_path_relative` | Chemin relatif simple |
| `test_normalize_path_empty` | Chemin vide |
| `test_write_orders_correctly` | Ré-indexation des ordres à l'écriture |

### Modèle Scheduler (`models::scheduler`) — 10 tests

| Test | Description |
|------|-------------|
| `test_day_schedule_deserialization` | Désérialisation d'un jour complet |
| `test_day_schedule_optional_time_defaults_to_empty` | Champ `time` optionnel (défaut chaîne vide) |
| `test_scheduler_config_deserialization` | Désérialisation complète du planning |
| `test_scheduler_config_optional_background` | Image de fond optionnelle |
| `test_normalize_path_already_public` | Chemin déjà préfixé `/public` |
| `test_normalize_path_scheduler_slash` | Chemin avec `/scheduler/` |
| `test_normalize_path_scheduler_no_slash` | Chemin avec `scheduler/` |
| `test_normalize_path_relative` | Chemin relatif simple |
| `test_normalize_path_empty` | Chemin vide |
| `test_normalize_path_absolute_other` | Chemin absolu autre (inchangé) |

### Modèle Config (`models::config`) — 5 tests

| Test | Description |
|------|-------------|
| `test_deserialize_config` | Désérialisation complète depuis du JSON |
| `test_default_obs_values` | Valeurs par défaut OBS |
| `test_font_path_with_leading_slash` | Police avec `/` au début |
| `test_font_path_without_leading_slash` | Police sans `/` au début |
| `test_default_functions` | Vérification des constantes par défaut |

### Exécution des tests

```sh
# Lancer tous les tests
cargo test

# Lancer les tests d'un module spécifique
cargo test models::banner
cargo test models::scheduler
cargo test models::config

# Lancer un test précis par son nom
cargo test test_banner_card_serialization_roundtrip
```

> 💡 Les tests sont isolés du code de production : ils ne sont compilés qu'avec `cargo test`, pas en `cargo build`.

---

## Rate Limiter

**Aucun rate limiter n'est actuellement implémenté** dans `praetorcast-core`. Les routes sont accessibles sans limitation de débit. Si nécessaire, un rate limiter peut être ajouté via un middleware Actix-web (ex: `actix-web-ratelimit`).

---

## Notes techniques

- **Tous les chemins** `OBS_*` sont optionnels dans `env.json` avec des valeurs par défaut
- **Configuration rechargée à chaque requête** : modification de `env.json` sans redémarrer le serveur
- **Obs-websocket** : utilisation de la crate `obws` (v0.14) compatible protocole OBS v5
- **Templates compilés** : les templates Askama sont vérifiés à la compilation (pas de runtime errors HTML)
- **WebSockets** : utilisation de `actix-ws` avec diffusion uniquement sur changement (pas de spam réseau)
- **Twitch EventSub** : connexion persistante avec reconnexion automatique et détection de token expiré