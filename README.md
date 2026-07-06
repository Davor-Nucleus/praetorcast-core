<div align="center">
  <!-- <img src="public/logo.png" alt="PraetorCast-core Logo" width="200"/> -->

  # 🌐 PraetorCast-Core

  **Le serveur web backend (Rust/Actix-web) pour les overlays OBS de PraetorCast.**

  [![Rust](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)](#)
</div>

---

Ce projet est un serveur web en Rust utilisant **Actix-web** et **Askama** (templates HTML compilés) qui sert de backend pour les overlays OBS **PraetorCast**. Il expose des pages d'affichage, des pages de configuration, des API REST et des WebSockets pour le pilotage en temps réel.

## 📋 Sommaire

- [Démarrage rapide](#-démarrage-rapide)
- [Architecture](#-architecture)
- [Configuration](#-configuration)
- [Routes et API](#-routes-et-api)
- [WebSockets](#-websockets)
- [Intégration Twitch EventSub](#-intégration-twitch-eventsub)
- [Fonctionnalités détaillées](#-fonctionnalités-détaillées)
- [Tests](#-tests)
- [Notes techniques](#-notes-techniques)

---

## 🚀 Démarrage rapide

```sh
# 1. Installer Rust (si pas déjà fait) : https://rustup.rs/

# 2. Compiler le projet
cargo build

# 3. Lancer le serveur
cargo run

# 4. Ouvrir le navigateur
# http://127.0.0.1:3000/ (ou le port défini dans env.json → PORT)
```

---

## 🏗️ Architecture

<details>
<summary><b>Cliquez pour dérouler l'arborescence du projet</b></summary>

```text
praetorcast-core/
├── Cargo.toml                    # Dépendances (actix-web, askama, obws, reqwest, etc.)
├── src/
│   ├── main.rs                   # Point d'entrée, déclaration des routes
│   ├── twitch.rs                 # Intégration Twitch EventSub (WebSocket followers)
│   ├── models/                   # Modèles de données (Config, Banner, Scheduler)
│   └── controllers/              # Contrôleurs pour les différentes routes
├── templates/                    # Templates Askama (HTML compilés)
├── data/                         # Données persistantes (banner.json, scheduler.json, etc.)
└── public/                       # Fichiers statiques (images, polices, JS, etc.)
```
</details>

---

## ⚙️ Configuration

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

## 🗺️ Routes et API

<details>
<summary><b>Pages d'affichage (overlays OBS)</b></summary>

| Route | Description |
|-------|-------------|
| `GET /` | Page d'accueil / index |
| `GET /clock` | Horloge (`?hour=true&minute=true&second=true`) |
| `GET /banner` | Bannière tournante (cartes) |
| `GET /music-current` | Musique en cours de lecture |
| `GET /emote-corner` | Émoticônes / emote wall Twitch |
| `GET /discord-presence` | Présence Discord |
| `GET /followers-info` | Informations followers Twitch |
| `GET /chat-horizontal` | Chat Twitch horizontal |
| `GET /chat-vertical` | Chat Twitch vertical |
| `GET /chat-youtube` | Chat YouTube |
</details>

<details>
<summary><b>Pages de configuration</b></summary>

| Route | Description |
|-------|-------------|
| `GET /music-config` | Config musique / soundboard / limiteur OBS |
| `GET /banner-config` | Config des cartes de bannière |
| `GET /scheduler` | Éditeur de planning hebdomadaire |
</details>

<details>
<summary><b>Endpoints API REST</b></summary>

**Banner**
- `GET /api/banner-config`
- `POST /api/banner-config`
- `POST /api/banner-upload`

**Scheduler**
- `GET /api/scheduler-config`
- `POST /api/scheduler-config`
- `POST /api/scheduler-upload`
- `POST /api/scheduler-background-upload`

**OBS (Limiter)** (Retourne `{ "enabled": bool, "threshold": float }` ou `503`)
- `GET /api/obs/limiter`
- `GET/POST /api/obs/limiter/add` (+1 dB)
- `GET/POST /api/obs/limiter/subtract` (-1 dB)
- `GET /api/obs/limiter/toggle`
</details>

---

## 🔌 WebSockets

Trois WebSockets permettent de pousser les changements en temps réel vers les overlays OBS, sans rafraîchissement manuel :

| Route | Flux poussé | Fréquence |
|-------|-------------|-----------|
| `/api/banner_ws` | Configuration du banner (liste des cartes JSON) | Sur changement (max 1s) |
| `/api/twitch_ws` | État Twitch : `{ total_followers, last_follower, connected }` | Sur changement (max 500ms) |
| `/api/obs/limiter_ws` | État du limiteur : `{ enabled, threshold }` (ou `null`) | Sur changement (max 1s) |

---

## 💜 Intégration Twitch EventSub

Le module `twitch.rs` se connecte en **WebSocket** à l'EventSub API Twitch (`wss://eventsub.wss.twitch.tv/ws`) et souscrit automatiquement aux événements `channel.follow`.

- **Connexion persistante** avec reconnexion automatique (toutes les 5s).
- **Détection de token invalide** (HTTP 401 → message d'erreur explicite).
- **État temps réel** : `total_followers` mis à jour à chaque nouveau follower.
- **Reconnexion à chaud** gérée via `session_reconnect` de Twitch.

---

## 🌟 Fonctionnalités détaillées

### 🎥 OBS Limiter
- Pilotage du filtre **Limiter** d'OBS (obs-websocket v5) appliqué à une source audio.
- Création **automatique** du filtre s'il n'existe pas encore.
- Modification du seuil en dB (clampé entre −60 dB et 0 dB, pas de 1 dB).
- WebSocket temps réel qui reflète aussi les changements faits **directement dans OBS**.

### 🖼️ Banner
- Système de cartes avec texte, image, transition et durée d'affichage.
- Normalisation automatique des chemins d'images (`banner/img.png` → `/public/banner/img.png`).
- Fallback automatique en cas d'erreur de parsing JSON.
- Upload d'images avec génération d'UUID.

### 📅 Scheduler (Planning)
- Planning hebdomadaire avec 7 jours (index 0–6).
- Chaque jour : titre, date, horaire, image de couverture.
- Upload d'images de couverture et de fond.

---

## 🧪 Tests

Des tests unitaires sont intégrés directement dans les fichiers sources (`#[cfg(test)] mod tests`). **Total : 24 tests**

```sh
# Lancer tous les tests
cargo test

# Lancer les tests d'un module spécifique
cargo test models::banner
cargo test models::scheduler
cargo test models::config
```

> [!TIP]
> Les tests sont isolés du code de production : ils ne sont compilés qu'avec `cargo test`, pas en `cargo build`.

---

## ⚙️ Notes techniques

- **Aucun rate limiter** n'est actuellement implémenté sur les routes HTTP.
- **Tous les chemins** `OBS_*` sont optionnels dans `env.json` avec des valeurs par défaut.
- **Configuration rechargée à chaque requête** : vous pouvez modifier `env.json` sans redémarrer le serveur !
- **Obs-websocket** : utilisation de la crate `obws` (v0.14) compatible avec le protocole OBS v5.
- **Templates compilés** : les templates Askama sont vérifiés à la compilation (pas de runtime errors HTML).

<div align="center">
  <i>Développé avec ❤️ en Rust</i>
</div>