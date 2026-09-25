//! Compte à rebours piloté depuis `/timer-config`.
//!
//! L'état vit **côté serveur** : `/timer` n'est qu'un afficheur. C'est ce qui permet
//! à une source OBS de se rafraîchir (option « Actualiser quand la scène devient
//! active »), d'être dupliquée dans deux scènes, ou de survivre à un redémarrage du
//! serveur sans que le décompte reparte de zéro.
//!
//! Le temps n'est **pas** décompté seconde par seconde : on garde une ancre
//! (`remaining_ms` au dernier changement d'état, plus `started_at`) et la valeur
//! affichée s'en déduit. Rien n'est réécrit tant que personne ne touche un bouton,
//! et l'afficheur peut extrapoler avec sa propre horloge entre deux messages.

use crate::models::channel_point::AlertKind;
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const TIMER_PATH: &str = "data/timer.json";

/// Plafond de durée : 7 jours. Borne les saisies du configurateur comme les `+1 min`
/// répétés, pour qu'un doigt qui reste appuyé ne produise pas un compteur absurde.
///
/// Sept jours et non vingt-quatre heures : un subathon dépasse régulièrement la
/// journée, et le temps gagné au-delà du plafond serait perdu sans que personne ne
/// soit averti. L'affichage suit déjà (`timer.html` calcule les heures sans borne).
pub const MAX_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// Sérialise les écritures de `data/timer.json` **de ce processus**.
///
/// Sans lui, deux ajouts de temps concurrents — un abonnement reçu par la tâche
/// subathon et un `+1 min` cliqué dans le configurateur — lisent la même valeur et
/// le second écrasé le premier. Voir `update`.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Horloge murale en millisecondes.
///
/// Les mutations prennent l'instant en paramètre plutôt que d'appeler ceci
/// elles-mêmes : c'est ce qui rend le comportement testable sans attendre.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Conversion « événement Twitch reçu » → « temps ajouté au compte à rebours ».
///
/// Le temps par abonnement est **plat**, indépendant du palier : `amount` porte bien
/// le tier, donc un multiplicateur par palier pourra s'ajouter plus tard sans
/// changer le format du fichier.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Subathon {
    /// À `false` (défaut), aucun événement ne touche au compte à rebours.
    #[serde(default)]
    pub enabled: bool,
    #[serde(rename = "msPerSub", default = "default_ms_per_sub")]
    pub ms_per_sub: u64,
    /// Par abonnement offert : un don de 5 subs ajoute cinq fois cette valeur.
    #[serde(rename = "msPerGiftSub", default = "default_ms_per_sub")]
    pub ms_per_gift_sub: u64,
    /// Proratisé : 50 bits ajoutent la moitié de cette valeur.
    #[serde(rename = "msPer100Bits", default = "default_ms_per_100_bits")]
    pub ms_per_100_bits: u64,
    /// Par spectateur amené par un raid. À 0 (défaut), les raids n'ajoutent rien —
    /// un gros raid ferait sinon exploser le compteur.
    #[serde(rename = "msPerRaider", default)]
    pub ms_per_raider: u64,
}

fn default_ms_per_sub() -> u64 {
    5 * 60 * 1000
}
fn default_ms_per_100_bits() -> u64 {
    60 * 1000
}

impl Default for Subathon {
    fn default() -> Self {
        Self {
            enabled: false,
            ms_per_sub: default_ms_per_sub(),
            ms_per_gift_sub: default_ms_per_sub(),
            ms_per_100_bits: default_ms_per_100_bits(),
            ms_per_raider: 0,
        }
    }
}

impl Subathon {
    /// Millisecondes à ajouter pour un événement. `0` = rien à faire.
    ///
    /// Les points de chaîne sont volontairement absents : ils ont déjà leur propre
    /// coût en points, les faire aussi rallonger le stream serait un double compte.
    pub fn bonus_ms(&self, kind: AlertKind, amount: u64) -> u64 {
        if !self.enabled {
            return 0;
        }
        match kind {
            // Prime compris : il coûte au diffuseur ce que coûte un tier 1, et le
            // séparer imposerait un réglage de plus pour la même valeur par défaut.
            AlertKind::Sub | AlertKind::Resub | AlertKind::Prime => self.ms_per_sub,
            // `amount` est le nombre de subs offerts d'un coup.
            AlertKind::Gift => self.ms_per_gift_sub.saturating_mul(amount),
            // Proratisé au bit près, en multipliant avant de diviser pour ne pas
            // perdre les petits cheers dans un arrondi.
            AlertKind::Cheer => self.ms_per_100_bits.saturating_mul(amount) / 100,
            AlertKind::Raid => self.ms_per_raider.saturating_mul(amount),
            AlertKind::ChannelPoints => 0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Timer {
    #[serde(default = "default_title")]
    pub title: String,
    /// Durée configurée : celle que « Reset » restaure.
    #[serde(rename = "durationMs", default = "default_duration")]
    pub duration_ms: u64,
    /// Restant au dernier changement d'état. Pendant que le compte à rebours tourne,
    /// la valeur réelle est `remaining_ms - (maintenant - started_at)`.
    #[serde(rename = "remainingMs", default = "default_duration")]
    pub remaining_ms: u64,
    #[serde(default)]
    pub running: bool,
    /// Horodatage du dernier démarrage. N'a de sens que si `running`.
    #[serde(rename = "startedAt", default)]
    pub started_at: u64,
    /// Texte qui remplace les chiffres une fois à zéro.
    #[serde(rename = "endText", default = "default_end_text")]
    pub end_text: String,
    /// Retire complètement l'overlay à zéro, au lieu d'afficher `end_text`.
    #[serde(rename = "hideWhenZero", default)]
    pub hide_when_zero: bool,
    /// Barre de progression sous les chiffres. Par défaut à `true` : un fichier
    /// écrit avant l'ajout de cette option garde l'affichage qu'il avait.
    #[serde(rename = "showProgress", default = "default_true")]
    pub show_progress: bool,
    /// Vide = couleur d'accent du thème global.
    #[serde(rename = "accentColor", default)]
    pub accent_color: String,
    /// Bloc absent d'un fichier existant : subathon désactivé, rien ne change.
    #[serde(default)]
    pub subathon: Subathon,
}

fn default_title() -> String {
    "Retour dans".to_string()
}
fn default_duration() -> u64 {
    5 * 60 * 1000
}
fn default_end_text() -> String {
    "C'est reparti !".to_string()
}
fn default_true() -> bool {
    true
}

impl Default for Timer {
    fn default() -> Self {
        Self {
            title: default_title(),
            duration_ms: default_duration(),
            remaining_ms: default_duration(),
            running: false,
            started_at: 0,
            end_text: default_end_text(),
            hide_when_zero: false,
            show_progress: true,
            accent_color: String::new(),
            subathon: Subathon::default(),
        }
    }
}

/// Réglages éditables depuis le configurateur — tout sauf l'état de marche, qui
/// n'appartient qu'aux boutons.
#[derive(Deserialize)]
pub struct TimerSettings {
    #[serde(default = "default_title")]
    pub title: String,
    #[serde(rename = "durationMs", default = "default_duration")]
    pub duration_ms: u64,
    #[serde(rename = "endText", default = "default_end_text")]
    pub end_text: String,
    #[serde(rename = "hideWhenZero", default)]
    pub hide_when_zero: bool,
    #[serde(rename = "showProgress", default = "default_true")]
    pub show_progress: bool,
    #[serde(rename = "accentColor", default)]
    pub accent_color: String,
    #[serde(default)]
    pub subathon: Subathon,
}

impl Timer {
    /// Temps restant réel à l'instant `now`.
    ///
    /// `saturating_sub` des deux côtés : une horloge qui recule (changement d'heure,
    /// synchro NTP) donnerait sinon un écart négatif, donc un compteur qui remonte.
    pub fn remaining_at(&self, now: u64) -> u64 {
        if !self.running {
            return self.remaining_ms;
        }
        self.remaining_ms
            .saturating_sub(now.saturating_sub(self.started_at))
    }

    /// Démarre, ou reprend après une pause.
    ///
    /// Redémarrer un compteur déjà en marche ne fait rien : sans ce garde-fou, un
    /// double-clic replacerait l'ancre à `now` en gardant l'ancien `remaining_ms`,
    /// et le décompte gagnerait le temps déjà écoulé.
    pub fn start_at(&mut self, now: u64) {
        if self.running {
            return;
        }
        // Relance après la fin : on repart de la durée configurée plutôt que de
        // rester bloqué à zéro.
        if self.remaining_ms == 0 {
            self.remaining_ms = self.duration_ms;
        }
        self.running = true;
        self.started_at = now;
    }

    /// Fige la valeur courante.
    pub fn pause_at(&mut self, now: u64) {
        if !self.running {
            return;
        }
        self.remaining_ms = self.remaining_at(now);
        self.running = false;
        self.started_at = 0;
    }

    pub fn toggle_at(&mut self, now: u64) {
        if self.running {
            self.pause_at(now);
        } else {
            self.start_at(now);
        }
    }

    /// Restaure la durée configurée **et** arrête le compteur : « Reset » sert à
    /// repartir d'une base propre, pas à relancer par surprise en plein direct.
    pub fn reset(&mut self) {
        self.remaining_ms = self.duration_ms;
        self.running = false;
        self.started_at = 0;
    }

    /// Ajoute ou retire du temps, compteur en marche ou à l'arrêt.
    ///
    /// L'ancre est replacée à `now` : sans ça, le temps déjà écoulé depuis le
    /// démarrage serait décompté une seconde fois du nouveau total.
    pub fn adjust_at(&mut self, now: u64, delta_ms: i64) {
        let base = self.remaining_at(now);
        self.remaining_ms = if delta_ms >= 0 {
            base.saturating_add(delta_ms as u64).min(MAX_MS)
        } else {
            base.saturating_sub(delta_ms.unsigned_abs())
        };
        if self.running {
            self.started_at = now;
        }
    }

    /// Applique les réglages du configurateur.
    ///
    /// Changer la durée pendant que le compteur tourne ne touche pas au décompte en
    /// cours — la nouvelle valeur prendra effet au prochain « Reset ». Corriger une
    /// durée en plein direct ne doit pas faire sauter l'affichage.
    pub fn apply_settings(&mut self, settings: TimerSettings) {
        self.title = settings.title;
        self.duration_ms = settings.duration_ms.min(MAX_MS);
        self.end_text = settings.end_text;
        self.hide_when_zero = settings.hide_when_zero;
        self.show_progress = settings.show_progress;
        self.accent_color = settings.accent_color;
        self.subathon = settings.subathon;
        if !self.running {
            self.remaining_ms = self.duration_ms;
        }
    }
}

/// Lit `data/timer.json`. Un fichier absent vaut la configuration par défaut plutôt
/// qu'une erreur : l'overlay doit toujours pouvoir s'afficher.
///
/// Ne prend pas le verrou : un lecteur n'a rien à sérialiser, et l'écriture atomique
/// de `write_unlocked` garantit qu'il ne verra jamais un fichier à moitié écrit.
fn read_unlocked() -> Result<Timer, String> {
    let content = match fs::read_to_string(TIMER_PATH) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Timer::default()),
        Err(e) => return Err(format!("Error reading timer.json: {e}")),
    };

    serde_json::from_str(&content).map_err(|e| format!("Error parsing timer.json: {e}"))
}

/// Écrit par fichier temporaire puis `rename`.
///
/// `fs::write` tronque avant d'écrire : un tick de `/api/timer_ws` tombant dans cette
/// fenêtre lisait un JSON invalide, et `timer_controller::current()` l'avalait en
/// `Timer::default()` — soit un overlay qui repart à « Retour dans 5:00 » tout seul,
/// en plein direct. Chaque client relit le fichier chaque seconde, la fenêtre n'est
/// pas étroite.
fn write_unlocked(timer: &Timer) -> Result<(), String> {
    let json = serde_json::to_string_pretty(timer).map_err(|e| format!("Error serializing: {e}"))?;
    super::fs_atomic::write(TIMER_PATH, &json)
}

pub fn read() -> Result<Timer, String> {
    read_unlocked()
}

/// Lit, modifie et réécrit en un seul geste sérialisé.
///
/// Seule porte d'écriture, volontairement : il n'existe pas de `write` public, sinon
/// un appelant pourrait persister un état lu hors du verrou et perdre l'ajout d'un
/// autre écrivain.
///
/// La lecture est **dans** le verrou : deux ajouts de temps simultanés — un
/// abonnement encaissé par la tâche subathon et un `+1 min` cliqué dans le
/// configurateur — doivent s'additionner, pas s'écraser. Actix sert les requêtes sur
/// plusieurs threads, la course est réelle et pas théorique.
///
/// `read_unlocked` / `write_unlocked` sont appelés directement : passer par les
/// versions publiques reprendrait le verrou, et `std::sync::Mutex` n'est pas
/// réentrant — ce serait un interblocage.
pub fn update(change: impl FnOnce(&mut Timer, u64)) -> Result<Timer, String> {
    let _guard = WRITE_LOCK.lock().unwrap();
    let mut timer = read_unlocked()?;
    change(&mut timer, now_ms());
    write_unlocked(&timer)?;
    Ok(timer)
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_700_000_000_000;
    const MIN: u64 = 60_000;

    fn timer() -> Timer {
        Timer {
            duration_ms: 5 * MIN,
            remaining_ms: 5 * MIN,
            ..Default::default()
        }
    }

    #[test]
    fn deserialise_une_config_minimale() {
        // Tous les champs sont optionnels : un fichier écrit par une version
        // antérieure doit continuer de se lire.
        let t: Timer = serde_json::from_str("{}").unwrap();
        assert_eq!(t.duration_ms, 5 * MIN);
        assert_eq!(t.remaining_ms, 5 * MIN);
        assert!(!t.running);
    }

    #[test]
    fn roundtrip_conserve_les_noms_camel_case() {
        let json = serde_json::to_string(&timer()).unwrap();
        assert!(json.contains("\"durationMs\""));
        assert!(json.contains("\"remainingMs\""));
        assert!(json.contains("\"startedAt\""));
        assert!(json.contains("\"hideWhenZero\""));
        let back: Timer = serde_json::from_str(&json).unwrap();
        assert_eq!(back.duration_ms, 5 * MIN);
    }

    #[test]
    fn a_l_arret_la_valeur_ne_bouge_pas() {
        let t = timer();
        assert_eq!(t.remaining_at(T0), 5 * MIN);
        // Une heure plus tard, toujours la même : c'est tout l'intérêt de l'ancre.
        assert_eq!(t.remaining_at(T0 + 3_600_000), 5 * MIN);
    }

    #[test]
    fn en_marche_la_valeur_decompte() {
        let mut t = timer();
        t.start_at(T0);
        assert_eq!(t.remaining_at(T0), 5 * MIN);
        assert_eq!(t.remaining_at(T0 + MIN), 4 * MIN);
        // Dépassement : sature à zéro, jamais de valeur négative qui déborderait.
        assert_eq!(t.remaining_at(T0 + 10 * MIN), 0);
    }

    #[test]
    fn un_second_demarrage_ne_decale_pas_l_ancre() {
        let mut t = timer();
        t.start_at(T0);
        // Double-clic sur « Démarrer » deux minutes plus tard.
        t.start_at(T0 + 2 * MIN);
        assert_eq!(t.started_at, T0);
        assert_eq!(t.remaining_at(T0 + 2 * MIN), 3 * MIN);
    }

    #[test]
    fn la_pause_fige_la_valeur() {
        let mut t = timer();
        t.start_at(T0);
        t.pause_at(T0 + 2 * MIN);
        assert!(!t.running);
        assert_eq!(t.remaining_ms, 3 * MIN);
        // Le temps passé en pause n'est pas décompté.
        assert_eq!(t.remaining_at(T0 + 30 * MIN), 3 * MIN);

        t.start_at(T0 + 30 * MIN);
        assert_eq!(t.remaining_at(T0 + 31 * MIN), 2 * MIN);
    }

    #[test]
    fn relancer_apres_la_fin_repart_de_la_duree() {
        let mut t = timer();
        t.start_at(T0);
        t.pause_at(T0 + 10 * MIN);
        assert_eq!(t.remaining_ms, 0);

        t.start_at(T0 + 11 * MIN);
        assert_eq!(t.remaining_at(T0 + 11 * MIN), 5 * MIN);
    }

    #[test]
    fn ajouter_du_temps_en_marche_ne_compte_pas_deux_fois() {
        let mut t = timer();
        t.start_at(T0);
        // Deux minutes écoulées, il reste 3 min ; +1 min doit donner 4 min.
        t.adjust_at(T0 + 2 * MIN, MIN as i64);
        assert_eq!(t.remaining_at(T0 + 2 * MIN), 4 * MIN);
        assert!(t.running);
        // Et le décompte reprend depuis là, sans rattrapage.
        assert_eq!(t.remaining_at(T0 + 3 * MIN), 3 * MIN);
    }

    #[test]
    fn retirer_du_temps_sature_a_zero() {
        let mut t = timer();
        t.adjust_at(T0, -(10 * MIN as i64));
        assert_eq!(t.remaining_ms, 0);
    }

    #[test]
    fn ajouter_du_temps_est_plafonne() {
        let mut t = timer();
        // Assez de « +1 h » pour dépasser le plafond quelle que soit sa valeur : un
        // nombre d'itérations en dur casserait ce test au prochain relèvement.
        const HOUR: u64 = 3_600_000;
        for _ in 0..(MAX_MS / HOUR + 5) {
            t.adjust_at(T0, HOUR as i64);
        }
        assert_eq!(t.remaining_ms, MAX_MS);
    }

    #[test]
    fn reset_restaure_la_duree_et_arrete() {
        let mut t = timer();
        t.start_at(T0);
        t.adjust_at(T0 + MIN, 10 * MIN as i64);
        t.reset();
        assert!(!t.running);
        assert_eq!(t.remaining_ms, 5 * MIN);
        assert_eq!(t.started_at, 0);
    }

    #[test]
    fn changer_la_duree_en_marche_ne_touche_pas_au_decompte() {
        let mut t = timer();
        t.start_at(T0);
        t.apply_settings(TimerSettings {
            title: "Pause".to_string(),
            duration_ms: 20 * MIN,
            end_text: "Fin".to_string(),
            hide_when_zero: true,
            show_progress: true,
            accent_color: String::new(),
            subathon: Subathon::default(),
        });
        assert_eq!(t.title, "Pause");
        assert_eq!(t.duration_ms, 20 * MIN);
        // L'affichage en cours est intact ; la nouvelle durée attend le Reset.
        assert_eq!(t.remaining_at(T0 + MIN), 4 * MIN);
        t.reset();
        assert_eq!(t.remaining_ms, 20 * MIN);
    }

    #[test]
    fn changer_la_duree_a_l_arret_l_applique_tout_de_suite() {
        let mut t = timer();
        t.apply_settings(TimerSettings {
            title: default_title(),
            // Au-delà du plafond : ramené à MAX_MS.
            duration_ms: MAX_MS * 3,
            end_text: default_end_text(),
            hide_when_zero: false,
            show_progress: true,
            accent_color: "#ff0000".to_string(),
            subathon: Subathon::default(),
        });
        assert_eq!(t.duration_ms, MAX_MS);
        assert_eq!(t.remaining_ms, MAX_MS);
        assert_eq!(t.accent_color, "#ff0000");
    }

    #[test]
    fn la_barre_de_progression_se_masque_et_reste_affichee_par_defaut() {
        // Absente du fichier : la barre reste visible, comme avant l'option.
        let t: Timer = serde_json::from_str("{}").unwrap();
        assert!(t.show_progress);

        let mut t = timer();
        t.apply_settings(TimerSettings {
            title: default_title(),
            duration_ms: 5 * MIN,
            end_text: default_end_text(),
            hide_when_zero: false,
            show_progress: false,
            accent_color: String::new(),
            subathon: Subathon::default(),
        });
        assert!(!t.show_progress);

        // Et le choix survit à l'aller-retour JSON.
        let back: Timer = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        assert!(!back.show_progress);
    }

    #[test]
    fn toggle_alterne_marche_et_pause() {
        let mut t = timer();
        t.toggle_at(T0);
        assert!(t.running);
        t.toggle_at(T0 + MIN);
        assert!(!t.running);
        assert_eq!(t.remaining_ms, 4 * MIN);
    }

    #[test]
    fn une_horloge_qui_recule_ne_fait_pas_remonter_le_compteur() {
        let mut t = timer();
        t.start_at(T0);
        // `now` antérieur à l'ancre : l'écart sature à 0, la valeur reste la durée
        // pleine au lieu de dépasser par le haut.
        assert_eq!(t.remaining_at(T0 - 10 * MIN), 5 * MIN);
    }

    // ── Subathon ────────────────────────────────────────────────────────────────

    /// Le plafond doit laisser passer un subathon réel : à 24 h, le temps gagné
    /// au-delà était silencieusement perdu.
    #[test]
    fn le_plafond_couvre_une_semaine() {
        assert_eq!(MAX_MS, 7 * 24 * 3_600_000);
    }

    #[test]
    fn un_fichier_sans_bloc_subathon_se_relit() {
        // Rétro-compatibilité : un timer.json écrit avant cette version doit
        // continuer de se lire, subathon éteint.
        let t: Timer = serde_json::from_str("{}").unwrap();
        assert!(!t.subathon.enabled);
        assert_eq!(t.subathon.ms_per_sub, 5 * MIN);
        assert_eq!(t.subathon.ms_per_raider, 0);
    }

    #[test]
    fn roundtrip_conserve_les_noms_du_subathon() {
        let json = serde_json::to_string(&timer()).unwrap();
        assert!(json.contains("\"msPerSub\""), "{json}");
        assert!(json.contains("\"msPer100Bits\""), "{json}");
        assert!(json.contains("\"msPerRaider\""), "{json}");
    }

    fn subathon() -> Subathon {
        Subathon {
            enabled: true,
            ..Default::default()
        }
    }

    #[test]
    fn desactive_aucun_evenement_n_ajoute_de_temps() {
        // Le garde-fou est dans `bonus_ms` et pas seulement dans l'appelant : c'est
        // ce qui garantit qu'activer la fonctionnalité reste un acte explicite.
        let s = Subathon::default();
        assert!(!s.enabled);
        assert_eq!(s.bonus_ms(AlertKind::Sub, 1000), 0);
        assert_eq!(s.bonus_ms(AlertKind::Cheer, 10_000), 0);
    }

    #[test]
    fn un_abonnement_ajoute_le_meme_temps_quel_que_soit_le_palier() {
        // Choix assumé : `amount` porte le tier, un multiplicateur par palier
        // pourra s'ajouter plus tard sans changer le format du fichier.
        let s = subathon();
        assert_eq!(s.bonus_ms(AlertKind::Sub, 1000), 5 * MIN);
        assert_eq!(s.bonus_ms(AlertKind::Sub, 3000), 5 * MIN);
        assert_eq!(s.bonus_ms(AlertKind::Resub, 2000), 5 * MIN);
    }

    #[test]
    fn un_abonnement_prime_credite_comme_un_abonnement() {
        // Régression à éviter : avant d'avoir son propre type, un Prime arrivait en
        // `Sub` et créditait déjà ce temps. L'oublier ici l'aurait fait disparaître
        // du subathon sans le moindre message.
        let s = subathon();
        assert_eq!(s.bonus_ms(AlertKind::Prime, 1000), 5 * MIN);
    }

    #[test]
    fn les_subs_offerts_comptent_un_par_un() {
        let s = subathon();
        assert_eq!(s.bonus_ms(AlertKind::Gift, 5), 25 * MIN);
    }

    #[test]
    fn les_bits_sont_proratises() {
        // 100 bits = la valeur pleine, 50 bits = la moitié. La multiplication
        // précède la division pour qu'un petit cheer ne disparaisse pas.
        let s = subathon();
        assert_eq!(s.bonus_ms(AlertKind::Cheer, 100), MIN);
        assert_eq!(s.bonus_ms(AlertKind::Cheer, 50), MIN / 2);
        assert_eq!(s.bonus_ms(AlertKind::Cheer, 1), 600);
    }

    #[test]
    fn un_raid_n_ajoute_rien_par_defaut() {
        // Un raid de 3 000 spectateurs ferait exploser le compteur : le réglage
        // part de zéro et c'est au streamer de l'ouvrir.
        assert_eq!(subathon().bonus_ms(AlertKind::Raid, 3000), 0);

        let s = Subathon {
            enabled: true,
            ms_per_raider: 1000,
            ..Default::default()
        };
        assert_eq!(s.bonus_ms(AlertKind::Raid, 42), 42_000);
    }

    #[test]
    fn les_points_de_chaine_ne_rallongent_pas_le_stream() {
        // Ils ont déjà leur coût en points : les compter ici serait un double compte.
        assert_eq!(subathon().bonus_ms(AlertKind::ChannelPoints, 0), 0);
    }

    #[test]
    fn un_afflux_d_evenements_ne_deborde_pas() {
        // `saturating_mul` plutôt qu'un `*` nu : un `amount` aberrant venu de
        // l'API ne doit pas provoquer un panic en release ni un temps négatif.
        let s = Subathon {
            enabled: true,
            ms_per_gift_sub: u64::MAX / 2,
            ..Default::default()
        };
        let bonus = s.bonus_ms(AlertKind::Gift, 1000);
        // La valeur sature, et `adjust_at` la ramène ensuite sous le plafond.
        let mut t = timer();
        t.adjust_at(T0, bonus.min(i64::MAX as u64) as i64);
        assert_eq!(t.remaining_ms, MAX_MS);
    }
}
