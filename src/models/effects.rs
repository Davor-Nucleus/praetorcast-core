//! Réglages des effets visuels : pluie d'emotes, cadre caméra, visualiseur audio.
//!
//! Réunis dans un seul fichier et une seule page (`/effects-config`) : ce sont trois
//! overlays sans contenu propre, qui réagissent au direct. Chaque champ a un défaut,
//! et un fichier partiel ou ancien se lit tel quel.

use serde::{Deserialize, Serialize};
use std::fs;

use super::fs_atomic;

pub const EFFECTS_PATH: &str = "data/effects.json";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct EffectsConfig {
    pub rain: RainConfig,
    pub frame: FrameConfig,
    pub visualizer: VisualizerConfig,
}

/// Pluie d'emotes : quels événements la déclenchent, et à partir de quel montant.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct RainConfig {
    pub on_raid: bool,
    /// Spectateurs amenés minimum.
    pub raid_min: u64,
    pub on_cheer: bool,
    /// Bits minimum.
    pub cheer_min: u64,
    pub on_gift: bool,
    /// Abonnements offerts d'un coup, minimum.
    pub gift_min: u64,
    /// Abonnements et réabonnements, Prime compris. Coupé par défaut : sur une
    /// chaîne active, il pleuvrait en continu.
    pub on_sub: bool,
    /// Un objectif de `/goal-config` qui atteint sa cible.
    pub on_goal: bool,
    /// Nombre d'emotes par averse.
    pub count: u32,
    /// Durée de la chute d'une emote, en millisecondes.
    pub duration_ms: u32,
    /// Échelle des emotes : `1` vaut 56 px sur une source 1080p.
    pub size: f32,
}

impl Default for RainConfig {
    fn default() -> Self {
        Self {
            on_raid: true,
            raid_min: 0,
            on_cheer: true,
            cheer_min: 500,
            on_gift: true,
            gift_min: 5,
            on_sub: false,
            on_goal: true,
            count: 60,
            duration_ms: 5000,
            size: 1.0,
        }
    }
}

/// Cadre caméra : bordure aux couleurs du thème, qui s'illumine sur un événement.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct FrameConfig {
    /// Épaisseur de la bordure, en pixels.
    pub thickness: u32,
    /// Dégradé qui tourne en continu autour du cadre.
    pub animated: bool,
    /// Halo lumineux permanent autour de la bordure.
    pub glow: bool,
    /// Illumination sur les abonnements, bits, raids…
    pub pulse: bool,
    /// Illumination aussi sur les follows, plus fréquents.
    pub pulse_on_follow: bool,
    /// Force de l'illumination : `1` normal, `2` le double.
    pub intensity: f32,
}

impl Default for FrameConfig {
    fn default() -> Self {
        Self {
            thickness: 6,
            animated: true,
            glow: true,
            pulse: true,
            pulse_on_follow: true,
            intensity: 1.0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum VisualizerStyle {
    /// Barres qui montent depuis le bas.
    #[default]
    Bars,
    /// Barres symétriques de part et d'autre de la ligne médiane.
    Mirror,
    /// Courbe continue.
    Wave,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct VisualizerConfig {
    pub bars: u32,
    pub style: VisualizerStyle,
    /// Amplification : au-dessus de `1`, une musique calme remplit davantage.
    pub sensitivity: f32,
}

impl Default for VisualizerConfig {
    fn default() -> Self {
        Self {
            bars: 48,
            style: VisualizerStyle::default(),
            sensitivity: 1.0,
        }
    }
}

fn clamp_f32(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() { value.clamp(min, max) } else { fallback }
}

impl EffectsConfig {
    /// Ramène chaque valeur dans des bornes jouables. Le formulaire les borne déjà ;
    /// ceci protège les overlays d'un fichier édité à la main — 100 000 emotes
    /// figeraient la source OBS.
    pub fn sanitized(mut self) -> Self {
        let rain = &mut self.rain;
        rain.count = rain.count.clamp(5, 300);
        rain.duration_ms = rain.duration_ms.clamp(1000, 20_000);
        rain.size = clamp_f32(rain.size, 0.3, 3.0, 1.0);

        let frame = &mut self.frame;
        frame.thickness = frame.thickness.clamp(1, 40);
        frame.intensity = clamp_f32(frame.intensity, 0.2, 3.0, 1.0);

        let visualizer = &mut self.visualizer;
        visualizer.bars = visualizer.bars.clamp(8, 128);
        visualizer.sensitivity = clamp_f32(visualizer.sensitivity, 0.3, 3.0, 1.0);
        self
    }
}

/// Réglages courants. Un fichier absent vaut les défauts, sans le créer : lire ne
/// doit pas écrire.
pub fn read() -> Result<EffectsConfig, String> {
    let content = match fs::read_to_string(EFFECTS_PATH) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(EffectsConfig::default()),
        Err(e) => return Err(format!("Error reading effects.json: {e}")),
    };
    serde_json::from_str::<EffectsConfig>(&content)
        .map(EffectsConfig::sanitized)
        .map_err(|e| format!("Error parsing effects.json: {e}"))
}

pub fn write(config: &EffectsConfig) -> Result<(), String> {
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Error serializing effects: {e}"))?;
    fs_atomic::write(EFFECTS_PATH, &json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_fichier_vide_vaut_les_defauts() {
        let config: EffectsConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config, EffectsConfig::default());
        assert!(config.rain.on_raid);
        assert!(!config.rain.on_sub, "les subs ne doivent pas faire pleuvoir par défaut");
    }

    #[test]
    fn un_fichier_partiel_garde_les_autres_defauts() {
        let config: EffectsConfig =
            serde_json::from_str(r#"{ "rain": { "cheerMin": 1000 }, "frame": { "thickness": 12 } }"#)
                .unwrap();
        assert_eq!(config.rain.cheer_min, 1000);
        assert_eq!(config.rain.count, 60);
        assert_eq!(config.frame.thickness, 12);
        assert!(config.frame.pulse);
        assert_eq!(config.visualizer.bars, 48);
    }

    #[test]
    fn les_valeurs_hors_bornes_sont_ramenees() {
        let mut config = EffectsConfig::default();
        config.rain.count = 100_000;
        config.rain.size = f32::NAN;
        config.frame.thickness = 0;
        config.visualizer.bars = 1000;
        config.visualizer.sensitivity = -4.0;

        let config = config.sanitized();
        assert_eq!(config.rain.count, 300);
        assert_eq!(config.rain.size, 1.0);
        assert_eq!(config.frame.thickness, 1);
        assert_eq!(config.visualizer.bars, 128);
        assert_eq!(config.visualizer.sensitivity, 0.3);
    }

    #[test]
    fn les_cles_sont_en_camel_case_pour_le_javascript() {
        let json = serde_json::to_string(&EffectsConfig::default()).unwrap();
        for key in ["onRaid", "cheerMin", "durationMs", "pulseOnFollow", "\"style\":\"bars\""] {
            assert!(json.contains(key), "{key} absent de {json}");
        }
    }
}
