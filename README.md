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
4. Ouvrez votre navigateur à l'adresse : http://127.0.0.1:8080/

## Structure
- `src/main.rs` : Code principal du serveur
- `templates/index.html` : Template dynamique Askama
- `Cargo.toml` : Dépendances du projet
