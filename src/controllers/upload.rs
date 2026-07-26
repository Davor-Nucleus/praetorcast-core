//! Réception des fichiers envoyés par les pages de configuration.
//!
//! Un seul point d'entrée pour les cinq endpoints d'upload (bannière, planning ×2,
//! points de chaîne ×2), qui étaient auparavant cinq copies de la même quarantaine
//! de lignes — sans aucun contrôle de type ni de taille : n'importe quel fichier,
//! de n'importe quel poids, était accepté puis servi sous `/public/`.

use actix_multipart::Multipart;
use actix_web::HttpResponse;
use bytes::Bytes;
use futures_util::TryStreamExt;
use std::fs;
use std::io::Write;
use uuid::Uuid;

pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "avif"];
pub const AUDIO_EXTENSIONS: &[&str] = &["mp3", "wav", "ogg", "m4a", "flac", "opus"];

/// Plafond par fichier. Large pour ne gêner aucun usage réel (un GIF d'alerte ou un
/// jingle restent très en dessous), mais suffisant pour qu'un envoi accidentel de
/// plusieurs gigaoctets ne remplisse pas le disque.
pub const MAX_UPLOAD_BYTES: usize = 64 * 1024 * 1024;

fn error(message: &str) -> HttpResponse {
    HttpResponse::BadRequest().json(serde_json::json!({ "error": message }))
}

/// Enregistre le premier fichier du corps multipart sous `dir`, avec un nom en UUID,
/// et renvoie son URL publique.
///
/// - `allowed` : extensions acceptées, en minuscules et sans point.
/// - Le fichier est écrit en flux ; s'il dépasse `MAX_UPLOAD_BYTES` l'écriture est
///   interrompue et le fichier partiel supprimé.
pub async fn save_upload(
    mut payload: Multipart,
    dir: &str,
    public_path: &str,
    allowed: &[&str],
) -> HttpResponse {
    if let Err(e) = fs::create_dir_all(dir) {
        eprintln!("Error creating directory {}: {}", dir, e);
        return HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": "Failed to create directory"}));
    }

    while let Ok(Some(mut field)) = payload.try_next().await {
        let Some(filename) = field.content_disposition().get_filename().map(str::to_owned) else {
            continue;
        };

        let extension = filename
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_lowercase())
            .unwrap_or_default();

        if !allowed.contains(&extension.as_str()) {
            return error(&format!(
                "Extension « {} » refusée (acceptées : {})",
                extension,
                allowed.join(", ")
            ));
        }

        let new_filename = format!("{}.{}", Uuid::new_v4(), extension);
        let filepath = format!("{}/{}", dir, new_filename);

        let mut file = match fs::File::create(&filepath) {
            Ok(file) => file,
            Err(e) => {
                eprintln!("Error creating file: {}", e);
                return HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": "Failed to create file"}));
            }
        };

        let mut written = 0usize;
        while let Ok(Some(chunk)) = field.try_next().await {
            let bytes: Bytes = chunk;
            written += bytes.len();

            if written > MAX_UPLOAD_BYTES {
                drop(file);
                let _ = fs::remove_file(&filepath);
                return error(&format!(
                    "Fichier trop volumineux (limite : {} Mo)",
                    MAX_UPLOAD_BYTES / (1024 * 1024)
                ));
            }

            if let Err(e) = file.write_all(&bytes) {
                eprintln!("Error writing chunk: {}", e);
                drop(file);
                let _ = fs::remove_file(&filepath);
                return HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": "Failed to write file"}));
            }
        }

        return HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "path": format!("{}/{}", public_path, new_filename)
        }));
    }

    error("No file provided")
}
