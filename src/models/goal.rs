//! Barres d'objectif (followers / abonnés / compteur libre).
//!
//! Même découpage que `banner` : une liste dans un fichier JSON de `data/`, une
//! lecture qui ne bloque jamais l'overlay, une écriture qui recrée le dossier au
//! besoin. L'ordre du tableau fait foi pour l'affichage.

use serde::{Deserialize, Serialize};
use std::fs;

const GOAL_PATH: &str = "data/goal.json";

/// D'où vient la valeur courante d'une barre.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum GoalSource {
    /// Nombre total de followers, déjà suivi en direct par la session EventSub.
    #[default]
    Followers,
    /// Nombre d'abonnés payants. Demande le scope `channel:read:subscriptions`
    /// sur le jeton ; sans lui la barre s'affiche sans valeur.
    Subs,
    /// Compteur saisi à la main dans le configurateur — pour tout le reste
    /// (dons, objectif de messages, palier arbitraire).
    Manual,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Goal {
    /// Identifiant stable, posé par le configurateur. Sert uniquement côté client
    /// à suivre une ligne pendant les réordonnancements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default)]
    pub source: GoalSource,
    #[serde(default = "default_title")]
    pub title: String,
    #[serde(default = "default_target")]
    pub target: u64,
    /// Valeur courante quand `source` vaut `manual`.
    #[serde(rename = "manualCurrent", default)]
    pub manual_current: u64,
    /// Retranchée de la valeur mesurée : permet un objectif « +50 followers ce
    /// stream » plutôt qu'un total absolu.
    #[serde(default)]
    pub baseline: u64,
    #[serde(rename = "accentColor", default = "default_accent")]
    pub accent_color: String,
    #[serde(rename = "showNumbers", default = "default_true")]
    pub show_numbers: bool,
    #[serde(rename = "showPercent", default = "default_true")]
    pub show_percent: bool,
    #[serde(default = "default_true")]
    pub visible: bool,
}

fn default_title() -> String {
    "Objectif followers".to_string()
}
fn default_target() -> u64 {
    100
}
fn default_accent() -> String {
    "#9146FF".to_string()
}
fn default_true() -> bool {
    true
}

impl Default for Goal {
    fn default() -> Self {
        Self {
            id: None,
            source: GoalSource::default(),
            title: default_title(),
            target: default_target(),
            manual_current: 0,
            baseline: 0,
            accent_color: default_accent(),
            show_numbers: true,
            show_percent: true,
            visible: true,
        }
    }
}

impl Goal {
    /// Valeur affichée, une fois la ligne de base retranchée.
    ///
    /// `measured` est le total brut relevé côté Twitch (ou ignoré si la source est
    /// `manual`). La soustraction sature à 0 : une ligne de base au-dessus du total
    /// mesuré donne une barre vide, jamais un débordement.
    pub fn current_from(&self, measured: Option<u64>) -> Option<u64> {
        match self.source {
            GoalSource::Manual => Some(self.manual_current.saturating_sub(self.baseline)),
            _ => measured.map(|m| m.saturating_sub(self.baseline)),
        }
    }

    /// Pourcentage d'avancement, borné à [0, 100].
    ///
    /// Une cible à 0 renverrait une division par zéro : on la traite comme un
    /// objectif déjà atteint plutôt que de propager un NaN jusqu'au `width` CSS.
    pub fn percent(&self, current: u64) -> f64 {
        if self.target == 0 {
            return 100.0;
        }
        ((current as f64 / self.target as f64) * 100.0).clamp(0.0, 100.0)
    }
}

#[derive(Serialize, Deserialize)]
struct GoalFile {
    goals: Vec<Goal>,
}

/// Tolère l'ancien format à objectif unique.
///
/// `data/goal.json` contenait une seule barre à plat (`{"source": …, "target": …}`)
/// avant le passage au multi-objectif. `Modern` exige la clé `goals`, ce qui évite
/// qu'un ancien fichier soit accepté comme une liste vide et perde son contenu.
#[derive(Deserialize)]
#[serde(untagged)]
enum GoalFileCompat {
    Modern(GoalFile),
    Legacy(Box<Goal>),
}

/// Lit `data/goal.json`. Un fichier absent crée la configuration par défaut plutôt
/// que de remonter une erreur : l'overlay doit toujours pouvoir s'afficher.
pub fn read() -> Result<Vec<Goal>, String> {
    let content = match fs::read_to_string(GOAL_PATH) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let default = vec![Goal::default()];
            write(&default)?;
            return Ok(default);
        }
        Err(e) => return Err(format!("Error reading goal.json: {e}")),
    };

    let parsed: GoalFileCompat =
        serde_json::from_str(&content).map_err(|e| format!("Error parsing goal.json: {e}"))?;

    Ok(match parsed {
        GoalFileCompat::Modern(file) => file.goals,
        GoalFileCompat::Legacy(goal) => vec![*goal],
    })
}

pub fn write(goals: &[Goal]) -> Result<(), String> {
    let file = GoalFile {
        goals: goals.to_vec(),
    };
    let json = serde_json::to_string_pretty(&file).map_err(|e| format!("Error serializing: {e}"))?;
    fs::create_dir_all("data").map_err(|e| format!("Error creating data dir: {e}"))?;
    fs::write(GOAL_PATH, json).map_err(|e| format!("Error writing goal.json: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goal(source: GoalSource, target: u64) -> Goal {
        Goal {
            source,
            target,
            ..Default::default()
        }
    }

    fn parse(json: &str) -> Vec<Goal> {
        match serde_json::from_str::<GoalFileCompat>(json).unwrap() {
            GoalFileCompat::Modern(f) => f.goals,
            GoalFileCompat::Legacy(g) => vec![*g],
        }
    }

    #[test]
    fn deserialise_une_config_minimale() {
        // Tous les champs sont optionnels : un goal.json écrit par une version
        // antérieure doit continuer de se lire.
        let goals = parse(r#"{"goals":[{}]}"#);
        assert_eq!(goals.len(), 1);
        assert_eq!(goals[0].source, GoalSource::Followers);
        assert_eq!(goals[0].target, 100);
        assert!(goals[0].visible);
    }

    #[test]
    fn lit_plusieurs_objectifs_dans_l_ordre_du_tableau() {
        let goals = parse(
            r#"{"goals":[
                {"source":"followers","title":"A","target":10},
                {"source":"subs","title":"B","target":20},
                {"source":"manual","title":"C","target":30,"manualCurrent":5}
            ]}"#,
        );
        assert_eq!(goals.len(), 3);
        assert_eq!(goals[0].title, "A");
        assert_eq!(goals[1].source, GoalSource::Subs);
        assert_eq!(goals[2].manual_current, 5);
    }

    #[test]
    fn migre_l_ancien_format_a_objectif_unique() {
        // Le fichier tel qu'il existait avant le multi-objectif : un objet à plat.
        // `r##` et pas `r#` : la couleur contient `"#`, qui fermerait la chaîne.
        let goals = parse(
            r##"{"source":"subs","title":"Ancien","target":42,"accentColor":"#ff0000"}"##,
        );
        assert_eq!(goals.len(), 1);
        assert_eq!(goals[0].title, "Ancien");
        assert_eq!(goals[0].source, GoalSource::Subs);
        assert_eq!(goals[0].target, 42);
        assert_eq!(goals[0].accent_color, "#ff0000");
    }

    #[test]
    fn une_liste_vide_reste_une_liste_vide() {
        // Distinct du cas hérité : `goals` présent mais vide veut dire « aucune
        // barre », et ne doit pas être confondu avec un ancien fichier.
        assert!(parse(r#"{"goals":[]}"#).is_empty());
    }

    #[test]
    fn roundtrip_conserve_les_noms_camel_case() {
        let goals = vec![Goal {
            id: Some("abc".to_string()),
            source: GoalSource::Subs,
            manual_current: 12,
            show_percent: false,
            ..Default::default()
        }];
        let json = serde_json::to_string(&GoalFile { goals: goals.clone() }).unwrap();
        assert!(json.contains("\"goals\""));
        assert!(json.contains("\"manualCurrent\""));
        assert!(json.contains("\"accentColor\""));
        assert!(json.contains("\"subs\""));

        let back = parse(&json);
        assert_eq!(back[0].id, Some("abc".to_string()));
        assert_eq!(back[0].source, GoalSource::Subs);
        assert!(!back[0].show_percent);
    }

    #[test]
    fn current_suit_la_source() {
        let mut manual = goal(GoalSource::Manual, 100);
        manual.manual_current = 42;
        // La valeur mesurée est ignorée en mode manuel.
        assert_eq!(manual.current_from(Some(999)), Some(42));

        let followers = goal(GoalSource::Followers, 100);
        assert_eq!(followers.current_from(Some(30)), Some(30));
        // Source indisponible (jeton sans le scope) : pas de valeur, pas de 0 trompeur.
        assert_eq!(followers.current_from(None), None);
    }

    #[test]
    fn la_ligne_de_base_est_retranchee_sans_deborder() {
        let mut g = goal(GoalSource::Followers, 50);
        g.baseline = 1000;
        assert_eq!(g.current_from(Some(1030)), Some(30));
        // Ligne de base au-dessus du mesuré : sature à 0 au lieu de déborder.
        assert_eq!(g.current_from(Some(10)), Some(0));
    }

    #[test]
    fn le_pourcentage_est_borne() {
        let g = goal(GoalSource::Followers, 200);
        assert_eq!(g.percent(0), 0.0);
        assert_eq!(g.percent(100), 50.0);
        assert_eq!(g.percent(200), 100.0);
        // Objectif dépassé : la barre reste pleine, elle ne sort pas du conteneur.
        assert_eq!(g.percent(500), 100.0);
    }

    #[test]
    fn une_cible_nulle_ne_divise_pas_par_zero() {
        let g = goal(GoalSource::Manual, 0);
        let percent = g.percent(0);
        assert!(percent.is_finite());
        assert_eq!(percent, 100.0);
    }
}
