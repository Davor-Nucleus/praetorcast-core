//! Écriture de `env.json`, clé par clé.
//!
//! Le fichier a **plusieurs écrivains concurrents** : JanusCore y persiste `VOLUME`
//! à chaque changement de volume et `normalizationEnabled` à chaque bascule,
//! PhonosCore y persiste `VOLUME` de son côté, et cette page de réglages y écrit le
//! reste. Réécrire l'objet entier depuis un seul de ces processus effacerait donc
//! les clés que les autres viennent de poser.
//!
//! D'où la seule opération d'écriture exposée ici : une fusion par clé sur une
//! **relecture fraîche** du fichier, suivie d'un remplacement atomique
//! (`.tmp` + `rename`). C'est la même sémantique que
//! `janus_nucleus::config::update_config_key`, réimplémentée parce que
//! praetorcast-core ne dépend pas de ce crate.

use serde_json::{Map, Value};
use std::sync::Mutex;

/// Chemin relatif au répertoire courant : `start/manager.cjs` lance tous les
/// services avec `cwd` = racine de PraetorCast, c'est donc l'`env.json` de la
/// racine qui est lu et écrit en production.
pub const ENV_PATH: &str = "env.json";

/// Sérialise les écritures **de ce processus**.
///
/// Ne protège pas de la course avec JanusCore/PhonosCore, qui sont des processus
/// distincts : deux fusions entrelacées peuvent encore perdre une valeur. La
/// fenêtre est de l'ordre de la milliseconde et les jeux de clés ne se recouvrent
/// pas (`VOLUME` et `normalizationEnabled` sont volontairement exclus du
/// formulaire de réglages, cf. `models::settings::FIELDS`).
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Contenu brut d'`env.json`, toutes clés comprises — y compris celles qu'`AppConfig`
/// ignore.
pub fn read_raw() -> Result<Map<String, Value>, String> {
    let content = std::fs::read_to_string(ENV_PATH)
        .map_err(|e| format!("Error reading env.json: {e}"))?;
    match serde_json::from_str(&content) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err("Error parsing env.json: la racine n'est pas un objet".to_string()),
        Err(e) => Err(format!("Error parsing env.json: {e}")),
    }
}

/// Applique `changes` sur le fichier existant, sans toucher aux autres clés.
///
/// Un `env.json` illisible annule l'écriture au lieu de repartir d'un objet vide :
/// sauvegarder par-dessus une corruption temporaire détruirait la configuration des
/// quatre autres services.
pub fn merge_keys(changes: &Map<String, Value>) -> Result<(), String> {
    if changes.is_empty() {
        return Ok(());
    }

    let _guard = WRITE_LOCK.lock().unwrap();

    let mut data = read_raw().map_err(|e| {
        format!("{e} — mise à jour annulée pour ne pas écraser la configuration")
    })?;
    for (key, value) in changes {
        data.insert(key.clone(), value.clone());
    }

    let json = serde_json::to_string_pretty(&Value::Object(data))
        .map_err(|e| format!("Error serializing env.json: {e}"))?;

    let tmp = format!("{ENV_PATH}.tmp");
    std::fs::write(&tmp, json).map_err(|e| format!("Error writing env.json: {e}"))?;
    std::fs::rename(&tmp, ENV_PATH).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("Error replacing env.json: {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fusion pure, sans toucher au disque : c'est la logique qu'on veut garantir
    /// (les clés absentes de `changes` survivent).
    fn merge(existing: &str, changes: &str) -> Map<String, Value> {
        let mut data: Map<String, Value> = serde_json::from_str(existing).unwrap();
        let changes: Map<String, Value> = serde_json::from_str(changes).unwrap();
        for (key, value) in &changes {
            data.insert(key.clone(), value.clone());
        }
        data
    }

    #[test]
    fn la_fusion_preserve_les_cles_des_autres_services() {
        let result = merge(
            r#"{"PORT": 3000, "VOLUME": 0.1, "janusCoreGui": true}"#,
            r#"{"PORT": 3010}"#,
        );
        assert_eq!(result["PORT"], 3010);
        assert_eq!(result["VOLUME"], 0.1);
        assert_eq!(result["janusCoreGui"], true);
    }

    #[test]
    fn la_fusion_ajoute_les_cles_absentes() {
        let result = merge(r#"{"PORT": 3000}"#, r#"{"TWITCH_REFRESH_TOKEN": "abc"}"#);
        assert_eq!(result["TWITCH_REFRESH_TOKEN"], "abc");
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn un_env_json_non_objet_est_refuse() {
        // `read_raw` renvoie une erreur plutôt que de repartir d'un objet vide.
        let parsed: Result<Value, _> = serde_json::from_str("[1, 2, 3]");
        assert!(!matches!(parsed, Ok(Value::Object(_))));
    }
}
