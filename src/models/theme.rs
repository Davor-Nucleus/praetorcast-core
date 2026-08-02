//! Thème global des overlays.
//!
//! Les onze overlays embarquaient chacun leur CSS en dur : neuf accents
//! concurrents, six piles de polices, quatre rayons de bordure, et le même bloc
//! d'animation recopié entre `banner.html` et `channel_point.html`. Changer la
//! couleur d'accent demandait d'éditer six fichiers puis de recompiler, parce que
//! les templates Askama sont compilés dans le binaire.
//!
//! Le thème vit donc dans `data/theme.json`, comme les autres réglages éditables, et
//! est servi en CSS par `GET /theme.css`. Les overlays ne référencent plus que des
//! `var(--pc-*)`, ce qui rend un changement d'apparence instantané et sans
//! recompilation.

use serde::{Deserialize, Serialize};
use std::fs;

const THEME_PATH: &str = "data/theme.json";

/// Nom de la famille déclarée par le `@font-face` généré. Les overlays ne
/// connaissent que `var(--pc-font)`, jamais ce nom ni le fichier derrière.
const FONT_FAMILY: &str = "PCTitle";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Theme {
    /// Couleur principale : jauges, dégradés, mises en avant.
    #[serde(default = "default_accent")]
    pub accent: String,
    /// Seconde couleur du dégradé de titre (bannière, points de chaîne).
    #[serde(rename = "accentSecondary", default = "default_accent_secondary")]
    pub accent_secondary: String,
    #[serde(default = "default_text")]
    pub text: String,
    #[serde(rename = "textMuted", default = "default_text_muted")]
    pub text_muted: String,
    /// Fond des overlays. `transparent` est le seul choix correct pour une Browser
    /// Source OBS ; une valeur opaque n'a de sens qu'en plein écran.
    #[serde(default = "default_bg")]
    pub background: String,
    /// Fond des éléments posés sur la vidéo (bulles de chat, pastilles).
    #[serde(default = "default_panel")]
    pub panel: String,
    #[serde(default = "default_radius")]
    pub radius: String,
    #[serde(rename = "radiusSmall", default = "default_radius_small")]
    pub radius_small: String,
    #[serde(default = "default_shadow")]
    pub shadow: String,
    /// Pile de repli si la police configurée ne charge pas.
    #[serde(rename = "fontFallback", default = "default_font_fallback")]
    pub font_fallback: String,
    /// Multiplicateur global de taille de texte.
    #[serde(rename = "fontScale", default = "default_font_scale")]
    pub font_scale: f32,
}

fn default_accent() -> String {
    // Le violet Twitch, déjà la valeur par défaut de la barre d'objectif.
    "#9146FF".to_string()
}
fn default_accent_secondary() -> String {
    "#ec4899".to_string()
}
fn default_text() -> String {
    "#ffffff".to_string()
}
fn default_text_muted() -> String {
    "rgba(255, 255, 255, 0.65)".to_string()
}
fn default_bg() -> String {
    "transparent".to_string()
}
fn default_panel() -> String {
    "rgba(0, 0, 0, 0.25)".to_string()
}
fn default_radius() -> String {
    "12px".to_string()
}
fn default_radius_small() -> String {
    "4px".to_string()
}
fn default_shadow() -> String {
    "0 2px 8px rgba(0, 0, 0, 0.85)".to_string()
}
fn default_font_fallback() -> String {
    "sans-serif".to_string()
}
fn default_font_scale() -> f32 {
    1.0
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: default_accent(),
            accent_secondary: default_accent_secondary(),
            text: default_text(),
            text_muted: default_text_muted(),
            background: default_bg(),
            panel: default_panel(),
            radius: default_radius(),
            radius_small: default_radius_small(),
            shadow: default_shadow(),
            font_fallback: default_font_fallback(),
            font_scale: default_font_scale(),
        }
    }
}

impl Theme {
    /// Feuille de style servie à tous les overlays.
    ///
    /// `font_url` vient de `config::font_path` : la police reste une clé d'`env.json`
    /// (`FRONT_FONT_TITLE`) parce que c'est un chemin de fichier, pas un token
    /// visuel — mais c'est ici, une seule fois, qu'elle devient un `@font-face`,
    /// au lieu d'être redéclarée dans six templates.
    pub fn to_css(&self, font_url: &str) -> String {
        format!(
            "/* Généré par praetorcast-core — modifiable depuis /settings */\n\
             @font-face {{\n  \
             font-family: \"{FONT_FAMILY}\";\n  \
             src: url(\"{font}\");\n  \
             font-display: swap;\n\
             }}\n\n\
             :root {{\n  \
             --pc-accent: {accent};\n  \
             --pc-accent-2: {accent2};\n  \
             --pc-text: {text};\n  \
             --pc-text-muted: {muted};\n  \
             --pc-bg: {bg};\n  \
             --pc-panel: {panel};\n  \
             --pc-radius: {radius};\n  \
             --pc-radius-sm: {radius_sm};\n  \
             --pc-shadow: {shadow};\n  \
             --pc-font-scale: {scale};\n  \
             --pc-font: \"{FONT_FAMILY}\", {fallback};\n  \
             --pc-gradient: linear-gradient(to right, {accent}, {accent2}, {accent});\n\
             }}\n",
            font = escape(font_url),
            accent = escape(&self.accent),
            accent2 = escape(&self.accent_secondary),
            text = escape(&self.text),
            muted = escape(&self.text_muted),
            bg = escape(&self.background),
            panel = escape(&self.panel),
            radius = escape(&self.radius),
            radius_sm = escape(&self.radius_small),
            shadow = escape(&self.shadow),
            scale = self.font_scale,
            fallback = escape(&self.font_fallback),
        )
    }
}

/// Neutralise ce qui permettrait de sortir d'une déclaration CSS.
///
/// Le serveur n'écoute que sur 127.0.0.1 et le thème n'est écrit que par son
/// propriétaire, mais une valeur contenant `}` casserait silencieusement toute la
/// feuille pour tous les overlays — autant l'empêcher à la source.
fn escape(value: &str) -> String {
    value
        .chars()
        .filter(|c| !matches!(c, '{' | '}' | ';' | '\n' | '\r' | '"' | '\\'))
        .collect()
}

pub fn read() -> Result<Theme, String> {
    let content = match fs::read_to_string(THEME_PATH) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let default = Theme::default();
            write(&default)?;
            return Ok(default);
        }
        Err(e) => return Err(format!("Error reading theme.json: {e}")),
    };

    serde_json::from_str(&content).map_err(|e| format!("Error parsing theme.json: {e}"))
}

pub fn write(theme: &Theme) -> Result<(), String> {
    let json =
        serde_json::to_string_pretty(theme).map_err(|e| format!("Error serializing theme: {e}"))?;
    fs::create_dir_all("data").map_err(|e| format!("Error creating data dir: {e}"))?;
    fs::write(THEME_PATH, json).map_err(|e| format!("Error writing theme.json: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Theme {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn un_theme_vide_prend_les_valeurs_par_defaut() {
        // Un data/theme.json amputé ne doit pas laisser un overlay sans couleur.
        assert_eq!(parse("{}"), Theme::default());
    }

    #[test]
    fn les_champs_camel_case_sont_relus() {
        let theme = parse(r##"{"accent": "#123456", "radiusSmall": "2px", "fontScale": 1.5}"##);
        assert_eq!(theme.accent, "#123456");
        assert_eq!(theme.radius_small, "2px");
        assert_eq!(theme.font_scale, 1.5);
        // Les autres gardent leur défaut.
        assert_eq!(theme.text, default_text());
    }

    #[test]
    fn le_css_declare_tous_les_tokens() {
        let css = Theme::default().to_css("/public/font/x.ttf");
        for token in [
            "--pc-accent:",
            "--pc-accent-2:",
            "--pc-text:",
            "--pc-text-muted:",
            "--pc-bg:",
            "--pc-panel:",
            "--pc-radius:",
            "--pc-radius-sm:",
            "--pc-shadow:",
            "--pc-font-scale:",
            "--pc-font:",
            "--pc-gradient:",
        ] {
            assert!(css.contains(token), "token manquant : {token}");
        }
    }

    #[test]
    fn la_police_est_declaree_une_seule_fois() {
        let css = Theme::default().to_css("/public/font/CinzelDecorative-Regular.ttf");
        assert_eq!(css.matches("@font-face").count(), 1);
        assert!(css.contains("url(\"/public/font/CinzelDecorative-Regular.ttf\")"));
    }

    #[test]
    fn une_valeur_hostile_ne_peut_pas_sortir_de_la_declaration() {
        let theme = Theme { accent: "red; } body { display: none".to_string(), ..Theme::default() };
        let css = theme.to_css("/f.ttf");
        // Deux blocs seulement, le @font-face et le :root : la valeur reste
        // enfermée dans sa déclaration, aucune règle supplémentaire n'est créée.
        assert_eq!(css.matches('{').count(), 2);
        assert_eq!(css.matches('}').count(), 2);
        assert_eq!(escape("a; } b { c"), "a  b  c");
    }
}
