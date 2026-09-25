//! Remplacement atomique d'un fichier de configuration.
//!
//! Les WebSockets des overlays relisent leur fichier `data/*.json` chaque seconde,
//! pour chaque source connectée. `fs::write` tronque avant d'écrire : un tick tombant
//! dans cette fenêtre lit un JSON vide ou coupé, et l'overlay se vide en plein direct
//! (ou, pour les alertes, l'événement en cours est perdu). Écrire à côté puis
//! `rename` garantit qu'un lecteur voit soit l'ancien fichier, soit le nouveau.

use std::fs;
use std::path::Path;
use std::sync::Mutex;

/// Sérialise les écritures de ce processus, tous fichiers confondus.
///
/// Deux écritures concurrentes d'un même fichier se disputeraient le même `.tmp` :
/// la seconde pourrait renommer un fichier que la première est en train d'écrire.
/// Les écritures sont rares (un « Enregistrer », un ajustement Stream Deck) : un seul
/// verrou global suffit. Les `WRITE_LOCK` de `goal` et `timer` se prennent toujours
/// **avant** celui-ci, jamais après, ce qui exclut tout interblocage.
static LOCK: Mutex<()> = Mutex::new(());

/// Écrit `contents` dans `path` par fichier temporaire puis `rename`, en créant le
/// dossier parent au besoin. Le `.tmp` est supprimé si le remplacement échoue.
pub fn write(path: &str, contents: &str) -> Result<(), String> {
    let name = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path);

    if let Some(parent) = Path::new(path).parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|e| format!("Error creating data dir: {e}"))?;
    }

    // Un verrou empoisonné ne protège qu'un `()` : rien à invalider, on continue.
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let tmp = format!("{path}.tmp");
    fs::write(&tmp, contents).map_err(|e| format!("Error writing {name}: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("Error replacing {name}: {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dossier propre au test, hors de `data/` : un test qui écrivait dans `data/`
    /// écrasait la vraie configuration de l'utilisateur.
    fn scratch(test: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("praetorcast-fs-atomic-{}-{test}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn remplace_le_contenu_sans_laisser_de_temporaire() {
        let dir = scratch("remplace");
        let path = dir.join("banner.json");
        let path = path.to_str().unwrap();

        write(path, r#"{"v":1}"#).unwrap();
        write(path, r#"{"v":2}"#).unwrap();

        assert_eq!(fs::read_to_string(path).unwrap(), r#"{"v":2}"#);
        assert!(!Path::new(&format!("{path}.tmp")).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cree_le_dossier_parent_manquant() {
        let dir = scratch("parent");
        let path = dir.join("sous").join("goal.json");
        let path = path.to_str().unwrap();

        write(path, "[]").unwrap();

        assert_eq!(fs::read_to_string(path).unwrap(), "[]");
        let _ = fs::remove_dir_all(&dir);
    }
}
