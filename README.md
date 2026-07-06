# PraetorCast-core

Ce projet est un serveur web en Rust utilisant Actix-web et Askama pour générer des pages dynamiques.

## Lancer le serveur

1. Installez Rust : https://rustup.rs/
2. Installez les dépendances :
   ```sh
   cargo build
   ```
3. Lancez le serveur :
   ```sh
   cargo run
   ```
4. Ouvrez votre navigateur à l'adresse : http://127.0.0.1:3000/ (ou le port défini par `PORT`)

## Structure
- `src/main.rs` : Point d'entrée et déclaration des routes
- `src/controllers/` : Contrôleurs (affichage, banner, scheduler, music, twitch, obs)
- `src/models/` : Configuration et modèles de données
- `templates/` : Templates dynamiques Askama (HTML)
- `Cargo.toml` : Dépendances du projet

## Tests

Des tests unitaires sont intégrés directement dans les fichiers sources (`#[cfg(test)] mod tests`).
Ils couvrent la logique métier des modèles (désérialisation, normalisation de chemins, valeurs par défaut).

```sh
# Lancer tous les tests
cargo test

# Lancer les tests d'un module spécifique (ex: banner)
cargo test models::banner

# Lancer un test précis par son nom
cargo test test_font_path_with_leading_slash
```

> 💡 Les tests sont isolés du code de production : ils ne sont compilés qu'avec `cargo test`, pas en `cargo build`.

## Limiteur OBS

La page `/music-config` peut piloter le filtre **Limiter** d'OBS appliqué à une source
audio, via obs-websocket v5 (crate `obws`). praetorcast-core sert de proxy : le mot de
passe OBS reste côté serveur.

Configuration (`env.json`, toutes optionnelles avec valeurs par défaut) :

| Clé | Défaut | Description |
|-----|--------|-------------|
| `OBS_WS_HOST` | `localhost` | Hôte du serveur obs-websocket |
| `OBS_WS_PORT` | `4455` | Port obs-websocket |
| `OBS_WS_PASSWORD` | `""` | Mot de passe (vide = pas d'authentification) |
| `OBS_AUDIO_SOURCE` | `music` | Nom exact de la source audio à limiter |
| `OBS_LIMITER_FILTER` | `Limiter` | Nom du filtre Limiter (créé automatiquement s'il manque) |

Endpoints (réponse `{ "enabled": bool, "threshold": float }`, ou `503` si OBS injoignable) :

- `GET /api/obs/limiter` — état courant
- `GET` / `POST` `/api/obs/limiter/add` — seuil +1 dB
- `GET` / `POST` `/api/obs/limiter/subtract` — seuil −1 dB
- `GET /api/obs/limiter/toggle` — active/désactive le filtre
