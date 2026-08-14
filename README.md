<div align="center">
  <!-- <img src="public/logo.png" alt="PraetorCast-core Logo" width="200"/> -->

  # PraetorCast-Core

  **Le serveur web backend (Rust/Actix-web) pour les overlays OBS de PraetorCast.**

  [![Rust](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)](#)
</div>

---

Ce projet est un serveur web en Rust utilisant **Actix-web** et **Askama** (templates HTML compilés) qui sert de backend pour les overlays OBS **PraetorCast**. Il expose des pages d'affichage, des pages de configuration, des API REST et des WebSockets pour le pilotage en temps réel.

## ✨ Fonctionnalités

- **Overlays OBS prêts à l'emploi** — horloge, bannière tournante, musique en cours, emote corner, présence Discord, infos followers.
- **Chat multi-plateformes** — chat Twitch (horizontal / vertical) et chat YouTube, avec récupération des badges Twitch.
- **Channel Points Twitch** — overlay d'alerte + page de configuration des récompenses (image et son personnalisés par récompense).
- **Bannière configurable** — cartes texte/image **ou objectif**, avec transition et durée, éditables depuis une page web dédiée. Des barres peuvent aussi rester fixées sur un bord pendant que les cartes tournent.
- **Barres d'objectif** — followers, abonnés ou compteur libre, empilables, avec ligne de base.
- **Planning hebdomadaire (Scheduler)** — 7 jours éditables (titre, date, horaire, jaquette, image de fond).
- **Pilotage OBS** — contrôle du filtre Limiter (obs-websocket v5) sur une source audio : activation, seuil en dB, création automatique du filtre.
- **Musique & soundboard** — page de configuration avec raccourcis clavier et intégration MPD.
- **Twitch EventSub** — connexion WebSocket persistante avec reconnexion automatique : followers, points de chaîne, abonnements, bits, raids, début/fin de direct.
- **Alertes d'événements** — image, son et phrase par type d'événement, avec paliers par montant (un cheer de 5 000 bits ≠ un cheer de 50). Les abonnements **Prime** ont leur propre déclencheur, distinct du tier 1 payant.
- **Test d'alerte en un clic** — chaque ligne du configurateur a son bouton « Tester dans OBS » : l'alerte joue dans les sources ouvertes, sans attendre l'événement Twitch et sans enregistrer au préalable.
- **Lien d'affichage copiable** — le bandeau de chaque page de configuration copie l'URL de l'overlay correspondant, prête à coller dans une source navigateur OBS.
- **Timer subathon** — barème événement → temps réglable ; les écritures du compte à rebours sont sérialisées pour qu'un ajout automatique et un clic manuel s'additionnent.
- **Temps réel** — WebSockets pour pousser bannière, état Twitch, channel points et limiteur vers les overlays sans rafraîchissement.
- **Uploads de médias** — images et sons envoyés depuis l'interface, stockés avec un nom UUID.
- **Configuration à chaud** — `env.json` relu à chaque requête, aucun redémarrage nécessaire.

## 📋 Sommaire

- [Fonctionnalités](#-fonctionnalités)

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
| `GET /text` | Texte animé, une section par source (`?name=start`) |
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

| Route | Description | Lien copiable |
|-------|-------------|---------------|
| `GET /music-config` | Config musique / soundboard / limiteur OBS | `/music-current` |
| `GET /banner-config` | Config des cartes de bannière | `/banner` |
| `GET /text-config` | Config des textes animés et de leurs URLs | une par section |
| `GET /channel-points-config` | Config des alertes, avec test par ligne | `/channel-points` |
| `GET /goal-config` | Config des barres d'objectif | `/goal` |
| `GET /timer-config` | Réglage et pilotage du compte à rebours | `/timer` |
| `GET /scheduler` | Éditeur de planning hebdomadaire | `/scheduler` |
| `GET /settings` | Édition d'`env.json` et du thème | — |

La dernière colonne est ce que copie le bouton **Copier le lien** du bandeau : l'URL de la
page d'affichage à coller dans une source navigateur OBS. `/text-config` fait exception,
chaque section y ayant sa propre URL (`/text?name=…`), copiable sur sa carte ; `/settings`
n'a pas de source OBS correspondante.
</details>

<details>
<summary><b>Endpoints API REST</b></summary>

**Banner**
- `GET /api/banner-config`
- `POST /api/banner-config`
- `POST /api/banner-upload`

**Textes animés**
- `GET /api/text-config`
- `POST /api/text-config`

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

**Alertes**
- `GET /api/channel-points-config`, `POST /api/channel-points-config`
- `POST /api/channel-points-upload-image`, `POST /api/channel-points-upload-sound`
- `POST /api/channel-points/test` — corps : **une ligne** de `channel_points.json`, telle
  que le configurateur la tient en mémoire. Réponse : `{ "success": true, "overlays": n }`,
  `n` étant le nombre de sources `/channel-points` qui l'ont reçue — `0` signifie
  qu'aucune n'est ouverte, seule explication d'un test resté sans effet.

Le corps porte la ligne au lieu d'un identifiant : c'est ce qui permet de tester une phrase
ou un son **avant** d'enregistrer. Le serveur en déduit un événement représentatif (montant
du palier de la ligne, `TestUser`, mois cumulés pour un réabonnement) et le diffuse sur un
canal distinct de celui des vraies alertes — sans quoi un cheer de test rallongerait le
compte à rebours du subathon.

**Objectifs** — paramètres dans l'URL et double verbe, comme le compte à rebours : un bouton
de Stream Deck ne sait faire qu'un GET.
- `GET /api/goal-config`, `POST /api/goal-config`
- `GET/POST /api/goal/adjust?id=<uuid>&delta=<i64>`
- `GET/POST /api/goal/set?id=<uuid>&value=<u64>`

Les deux dernières écrivent `manualCurrent` **brut** — pas la valeur affichée, dont
`baseline` est retranchée — et répondent `409` sur une source `followers`/`subs` (l'écriture
n'aurait aucun effet) ou `404` avec la liste des objectifs connus sur un `id` inconnu.
</details>

---

## 🔌 WebSockets

Plusieurs WebSockets poussent les changements en temps réel vers les overlays OBS, sans rafraîchissement manuel :

| Route | Flux poussé | Fréquence |
|-------|-------------|-----------|
| `/api/banner_ws` | Configuration du banner (liste des cartes JSON) | Sur changement (max 1s) |
| `/api/text_ws` | Configuration des textes (toutes les sections) | Sur changement (max 1s) |
| `/api/twitch_ws` | État Twitch : `{ total_followers, last_follower, connected, live, streamStartedAt }` | Sur changement (max 500ms) |
| `/api/obs/limiter_ws` | État du limiteur : `{ enabled, threshold }` (ou `null`) | Sur changement (max 1s) |
| `/api/channel_point_ws` | Alerte déjà résolue : `{ type: "alert", event, config }` | À l'événement, et au clic sur « Tester » |

---

## 💜 Intégration Twitch EventSub

Le module `twitch.rs` se connecte en **WebSocket** à l'EventSub API Twitch
(`wss://eventsub.wss.twitch.tv/ws`) et souscrit à sept types d'événements.

| Type | Version | Condition | Devient |
|---|---|---|---|
| `channel.follow` | 2 | `broadcaster_user_id` + `moderator_user_id` | Compteur de followers |
| `channel.channel_points_custom_reward_redemption.add` | 1 | `broadcaster_user_id` | Alerte |
| `channel.chat.notification` | 1 | `broadcaster_user_id` + `user_id` | Alerte + subathon |
| `channel.cheer` | 1 | `broadcaster_user_id` | Alerte + subathon |
| `channel.raid` | 1 | **`to_broadcaster_user_id`** | Alerte + subathon |
| `stream.online` / `stream.offline` | 1 | `broadcaster_user_id` | État `live` |

- **Connexion persistante** avec reconnexion automatique (toutes les 5s).
- **Détection de token invalide** (HTTP 401 → message d'erreur explicite).
- **Reconnexion à chaud** gérée via `session_reconnect` de Twitch.
- **Deux régimes d'échec.** Les follows et les points de chaîne sont indispensables : leur
  refus coupe la session, ce qui rend le problème visible. Les alertes d'événements
  **loguent et continuent** — sans quoi un `bits:read` pas encore accordé ferait refuser
  `channel.cheer` et emporterait tout le reste avec lui, en rebouclant toutes les 5 s. Le
  corps de la réponse Twitch est repris dans le log, donc il nomme lui-même le droit
  manquant.

> [!NOTE]
> **Tous** les abonnements passent par `channel.chat.notification`, et non par les
> `channel.subscribe` / `channel.subscription.*` qu'on attendrait : c'est le seul type
> EventSub dont la charge utile porte `is_prime`, donc le seul qui distingue un
> abonnement **Prime** d'un tier 1 payant. Il demande le droit `user:read:chat`, et sa
> condition un `user_id` en plus — celui du compte qui lit le chat, ici le diffuseur.
>
> Ne transporte pas les messages de chat ordinaires (c'est `channel.chat.message`), mais
> transporte les annonces et les raids : les `notice_type` non reconnus sont ignorés,
> sans quoi les raids seraient comptés deux fois.

> [!NOTE]
> Un don groupé émet un `community_sub_gift` récapitulatif **puis** un `sub_gift` par
> bénéficiaire, tous porteurs du même `community_gift_id`. Ces derniers sont donc
> ignorés : les compter deux fois doublerait les alertes et le temps du subathon.

---

## 🌟 Fonctionnalités détaillées

### 🎥 OBS Limiter
- Pilotage du filtre **Limiter** d'OBS (obs-websocket v5) appliqué à une source audio.
- Création **automatique** du filtre s'il n'existe pas encore.
- Modification du seuil en dB (clampé entre −60 dB et 0 dB, pas de 1 dB).
- WebSocket temps réel qui reflète aussi les changements faits **directement dans OBS**.

### 🖼️ Banner
- Système de cartes avec transition et durée d'affichage, réordonnables.
- **Deux types de carte**, choisis à l'ajout dans `/banner-config` :
  - `text` — texte et/ou image, la carte historique ;
  - `goal` — une barre d'objectif, ou toutes, selon le `goalId` retenu.
- **Barres fixes** (`dock`) : des objectifs affichés en permanence sur un bord, haut ou
  bas, pendant que les cartes tournent au-dessus. La hauteur réellement occupée est
  mesurée et rétrécit la zone des cartes, qui ne passent donc jamais dessous.
- Normalisation automatique des chemins d'images (`banner/img.png` → `/public/banner/img.png`).
- Fallback automatique en cas d'erreur de parsing JSON.
- Upload d'images avec génération d'UUID.

### ✍️ Textes animés
- **Une section = un texte, une URL.** `/text-config` définit des sections nommées ;
  chacune s'affiche via `/text?name=<nom>` dans sa propre source navigateur OBS.
  Le nom est normalisé à l'enregistrement (minuscules, accents aplatis, tirets) et
  dédoublonné — c'est une clé d'URL, deux homonymes rendraient l'un des deux
  inatteignable.
- **Deux animations cumulables**, deux réglages distincts :
  - *entrée*, jouée une fois — `fade`, `slide`, `zoom`, `flip`, `bounce`, `drop`,
    `swing`, `blur`, `rise`, `reveal`, `spin`, `stamp`, `flicker`, `typewriter`,
    `cascade`, `scatter` ;
  - *effet continu*, en boucle — `marquee`, `pulse`, `wave`, `glitch`, `gradient`,
    `float`, `tilt`, `shake`, `neon`, `rainbow`, `blink`, `heartbeat`, `shine`,
    `jelly`, `revolve`.

  L'effet ne démarre qu'à la fin de l'entrée : les deux animent `transform`, et un
  `wave` posé d'emblée écraserait le dévoilement lettre à lettre du `typewriter`.
  `typewriter`, `cascade` et `scatter` (comme `wave`) découpent le texte en un span
  par lettre ; les autres gardent un nœud texte simple, qui se coupe mieux en fin de
  ligne. `shine` est le seul dégradé qui garde la couleur choisie — `gradient` et
  `rainbow` repeignent tout le texte.
- **Fond transparent par défaut**, contrairement à `/banner` qui est plein écran sur
  noir : cette page est une incrustation. Taille, couleur, alignement, position
  verticale et couleur de fond se règlent par section.
- **Le rendu ne s'écrit qu'une fois** : `templates/partials/_text_render.html` est
  partagé par `/text` et par l'aperçu de `/text-config`, qui montre donc le rendu réel.
  Un partiel Askama et non un asset de `public/`, pour la même raison que les barres
  d'objectif.
- **Mise à jour sans rafraîchir OBS.** `/api/text_ws` pousse toutes les sections ; la
  page retrouve la sienne à chaque envoi. Une empreinte des champs visuels évite de
  reconstruire le DOM quand rien de visible n'a changé — sans elle, un « Sauvegarder »
  sur une **autre** section relancerait l'animation en plein direct.

### 🎯 Barres d'objectif
- Sources `followers` (relevée par EventSub), `subs` (Helix, cache de 60 s) ou `manual`.
- Plusieurs barres empilables, réordonnables, avec ligne de base (« +50 ce stream »
  plutôt qu'un total absolu).
- **Répartition des responsabilités** : `/goal-config` définit les objectifs et rien
  d'autre ; `/banner-config` décide de ceux que la bannière affiche et à quelle place.
- `/banner` lit les **valeurs** sur le même flux `/api/goal_ws` que `/goal`, et le
  **choix** de ce qu'il affiche sur `/api/banner_ws`. Aucun rafraîchissement de source
  OBS n'est nécessaire après un changement de réglage.
- Une carte dont la cible a été supprimée est retirée du cycle plutôt que d'imposer une
  carte vide à chaque tour.
- Le rendu d'une barre vit dans `templates/partials/_goal_bars.html`, partagé par
  `/goal`, `/banner` et l'aperçu de `/banner-config` — un partiel Askama et non un asset
  de `public/`, que la procédure de compilation ne recopie pas vers le dossier
  d'exécution.
- **Deux habillages, un seul composant.** La classe `.goal-banner`, posée par `/banner`
  et par l'aperçu, bascule les barres sur la direction artistique de la bannière :
  titre et chiffres au dégradé animé (`--pc-gradient`, comme `.banner-text`), tailles en
  `clamp()` calées sur la largeur de la source, piste au rayon `--pc-radius` au lieu de
  la pilule. Le remplissage garde l'`accentColor` de l'objectif, pour que deux barres
  restent distinguables. Sans cette classe — c'est le cas de `/goal` — l'aspect
  d'incrustation d'origine est conservé.

### 📅 Scheduler (Planning)
- Planning hebdomadaire avec 7 jours (index 0–6).
- Chaque jour : titre, date, horaire, image de couverture.
- Upload d'images de couverture et de fond.

---

## 🧪 Tests

### Rust — **144 tests**

Intégrés directement dans les fichiers sources (`#[cfg(test)] mod tests`).

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

### Overlays et pages de configuration (JavaScript) — **157 assertions**

```sh
node tests/js/run.cjs
```

L'essentiel du comportement des overlays vit dans le JavaScript des templates, hors de
portée de `cargo test` : réutilisation des barres d'un rafraîchissement à l'autre (sans
quoi la transition CSS repartirait de zéro), empreinte de structure qui empêche la
rotation de la bannière de redémarrer à chaque follower gagné, résolution d'une carte
vers son objectif, et échappement — en texte comme en attribut, deux règles distinctes.

`chat.test.cjs` couvre la modération du chat, qui est entièrement côté client : suppression
d'un message par son `target-msg-id`, purge d'un utilisateur par `user-id` (ou par login
quand le tag manque), vidage complet, et deux régressions de routage — un message dont le
texte contient « CLEARCHAT » doit s'afficher au lieu de vider l'overlay, et un « PING » tapé
dans le chat ne doit pas emporter la trame. C'est la seule suite qui charge un script
externe (`public/js/chat-common.js`) : elle le concatène au bloc inline du template dans le
même contexte, puisque les deux moitiés partagent leurs variables.

`copy-link.test.cjs` est la seule suite qui lit un **partiel** (`partials/_macros.html`) :
le bouton « Copier le lien » y est défini une fois pour six pages de configuration, et le
macro n'est développé qu'à la compilation — aucune suite de page ne verrait donc une
régression sur ce code partagé.

Aucune dépendance à installer : `tests/js/dom-stub.cjs` fournit le minimum de DOM
utilisé par les templates, et chaque suite charge le `<script>` **depuis le fichier
`.html` servi** — c'est donc bien le code de production qui est éprouvé, pas une copie.

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