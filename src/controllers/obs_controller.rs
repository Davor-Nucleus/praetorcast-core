use actix_web::{HttpResponse, Responder};
use obws::requests::filters::{Create, SetEnabled, SetSettings};
use obws::requests::sources::SourceId;
use obws::Client;
use crate::models::config::{load_config, AppConfig};

const THRESHOLD_MIN: f64 = -60.0;
const THRESHOLD_MAX: f64 = 0.0;
const THRESHOLD_STEP: f64 = 1.0;
/// Valeur par défaut du seuil du filtre Limiter d'OBS (dB) si OBS ne renvoie pas la clé.
const DEFAULT_THRESHOLD: f64 = -6.0;
/// Identifiant interne OBS du type de filtre "Limiteur".
const LIMITER_KIND: &str = "limiter_filter";

/// Ouvre une connexion obs-websocket à partir de la configuration courante.
async fn connect(config: &AppConfig) -> Result<Client, obws::error::Error> {
    let password: Option<&str> = if config.obs_ws_password.is_empty() {
        None
    } else {
        Some(config.obs_ws_password.as_str())
    };
    Client::connect(&config.obs_ws_host, config.obs_ws_port, password).await
}

/// Récupère le filtre Limiter sur la source audio configurée, en le **créant**
/// automatiquement s'il n'existe pas encore. La source, elle, doit déjà exister :
/// `list` renvoie une erreur si la source est introuvable, qu'on propage telle quelle
/// (on ne crée jamais de source).
async fn get_or_create_filter(
    client: &Client,
    config: &AppConfig,
) -> Result<obws::responses::filters::SourceFilter, obws::error::Error> {
    let source = SourceId::Name(&config.obs_audio_source);
    let existing = client.filters().list(source).await?;
    if let Some(filter) = existing
        .into_iter()
        .find(|f| f.name == config.obs_limiter_filter)
    {
        return Ok(filter);
    }
    // Filtre absent → on le crée avec les réglages par défaut d'OBS.
    client
        .filters()
        .create(Create {
            source,
            filter: &config.obs_limiter_filter,
            kind: LIMITER_KIND,
            settings: None::<serde_json::Value>,
        })
        .await?;
    client
        .filters()
        .get(source, &config.obs_limiter_filter)
        .await
}

/// Récupère l'état (activé + seuil) du filtre Limiter sur la source audio configurée.
async fn read_state(config: &AppConfig) -> Result<(bool, f64), obws::error::Error> {
    let client = connect(config).await?;
    let filter = get_or_create_filter(&client, config).await?;
    Ok((filter.enabled, extract_threshold(&filter.settings)))
}

/// OBS n'émet pas les réglages laissés à leur valeur par défaut : fallback explicite.
fn extract_threshold(settings: &serde_json::Value) -> f64 {
    settings
        .get("threshold")
        .and_then(|v| v.as_f64())
        .unwrap_or(DEFAULT_THRESHOLD)
}

fn state_response(enabled: bool, threshold: f64) -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "enabled": enabled,
        "threshold": threshold,
    }))
}

fn obs_error(e: obws::error::Error) -> HttpResponse {
    // Le champ `message` d'une erreur API porte le détail humain d'OBS
    // (ex. quelle source/quel filtre est introuvable) ; on le remonte.
    let detail = match &e {
        obws::error::Error::Api { code, message } => format!(
            "{:?}: {}",
            code,
            message.as_deref().unwrap_or("(aucun détail fourni par OBS)")
        ),
        other => other.to_string(),
    };
    eprintln!("OBS websocket error: {}", detail);
    HttpResponse::ServiceUnavailable().json(serde_json::json!({ "error": detail }))
}

/// GET /api/obs/limiter — état courant du filtre Limiter.
pub async fn get_limiter() -> impl Responder {
    let config = load_config();
    match read_state(&config).await {
        Ok((enabled, threshold)) => state_response(enabled, threshold),
        Err(e) => obs_error(e),
    }
}

/// GET /api/obs/limiter/add — augmente le seuil de 1 dB.
pub async fn add_limiter() -> impl Responder {
    adjust_threshold(THRESHOLD_STEP).await
}

/// GET /api/obs/limiter/subtract — diminue le seuil de 1 dB.
pub async fn subtract_limiter() -> impl Responder {
    adjust_threshold(-THRESHOLD_STEP).await
}

async fn adjust_threshold(delta: f64) -> HttpResponse {
    let config = load_config();
    match adjust_threshold_inner(&config, delta).await {
        Ok((enabled, threshold)) => state_response(enabled, threshold),
        Err(e) => obs_error(e),
    }
}

async fn adjust_threshold_inner(
    config: &AppConfig,
    delta: f64,
) -> Result<(bool, f64), obws::error::Error> {
    let client = connect(config).await?;
    let filter = get_or_create_filter(&client, config).await?;
    let new_threshold = (extract_threshold(&filter.settings) + delta).clamp(THRESHOLD_MIN, THRESHOLD_MAX);
    client
        .filters()
        .set_settings(SetSettings {
            source: SourceId::Name(&config.obs_audio_source),
            filter: &config.obs_limiter_filter,
            settings: serde_json::json!({ "threshold": new_threshold }),
            overlay: Some(true),
        })
        .await?;
    Ok((filter.enabled, new_threshold))
}

/// GET /api/obs/limiter/toggle — active/désactive le filtre Limiter.
pub async fn toggle_limiter() -> impl Responder {
    let config = load_config();
    match toggle_limiter_inner(&config).await {
        Ok((enabled, threshold)) => state_response(enabled, threshold),
        Err(e) => obs_error(e),
    }
}

async fn toggle_limiter_inner(config: &AppConfig) -> Result<(bool, f64), obws::error::Error> {
    let client = connect(config).await?;
    let filter = get_or_create_filter(&client, config).await?;
    let new_enabled = !filter.enabled;
    client
        .filters()
        .set_enabled(SetEnabled {
            source: SourceId::Name(&config.obs_audio_source),
            filter: &config.obs_limiter_filter,
            enabled: new_enabled,
        })
        .await?;
    Ok((new_enabled, extract_threshold(&filter.settings)))
}
