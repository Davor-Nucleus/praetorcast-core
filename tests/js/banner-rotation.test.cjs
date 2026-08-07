'use strict';
const fs = require('fs');
const path = require('path');
const assert = require('assert');
const { el, buildGoalTemplate, makeDocument } = require('./dom-stub.cjs');

const TPL_DIR = path.resolve(__dirname, '../../templates');

function inlineScript(file) {
  const html = fs.readFileSync(`${TPL_DIR}/${file}`, 'utf8');
  return /<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/.exec(html)[1];
}

/** Monte le script de banner.html sur le DOM factice et expose ses internes. */
function mountBanner() {
  const goalTpl = buildGoalTemplate();
  const bannerOverlay = el('div', 'banner-overlay');
  const emptyState = el('div', 'empty-state');
  // Classes reprises du template : le stub ne parse pas le HTML, mais la suite
  // vérifie plus bas que la déclaration réelle porte bien les mêmes.
  const goalDock = el('div', 'goal-dock goal-banner hide');
  bannerOverlay.appendChild(emptyState);
  bannerOverlay.appendChild(goalDock);

  const document = makeDocument({ goalTpl, bannerOverlay, emptyState, goalDock });
  const window = { addEventListener() {} };

  const createGoalBars = new Function(
    'document',
    inlineScript('partials/_goal_bars.html') + '\n; return createGoalBars;'
  )(document);

  const api = new Function(
    'document', 'window', 'createGoalBars', 'ResizeObserver',
    'requestAnimationFrame', 'setTimeout', 'clearTimeout', 'console', 'location', 'WebSocket',
    inlineScript('banner.html') + `
    ; return {
        rotationSignature, buildRotation, applyAll, displayCard, entriesFor,
        applyBannerConfig,
        setGoalEntries: (v) => { goalEntries = v; },
        state: () => ({ cards, currentIndex, currentCardElement, dock, bannerCards }),
    };`
  )(
    document, window, createGoalBars,
    undefined,                      // pas de ResizeObserver : le code doit rester tolérant
    (fn) => fn(),                   // requestAnimationFrame immédiat
    () => 0, () => {},              // rotation neutralisée : on pilote displayCard à la main
    { error() {}, log() {} },
    { protocol: 'http:', host: '127.0.0.1:3000' },
    function () { throw new Error('aucune WS ne doit être ouverte pendant le test'); }
  );

  return { api, bannerOverlay, emptyState, goalDock };
}

let passed = 0;
const check = (label, fn) => {
  try { fn(); passed++; console.log(`  ok    ${label}`); }
  catch (e) { console.log(`  ECHEC ${label}\n        ${e.message}`); process.exitCode = 1; }
};

const goalEntry = (id, current, percent) => ({
  config: { id, title: 'Objectif ' + id, target: 200, visible: true },
  current, percent, warning: null,
});

const textCard = (id, text) => ({
  id, kind: 'text', text, imagePath: '', transition: 'fade', durationMs: 5000,
});

const goalCard = (id, goalId, over = {}) => ({
  id, kind: 'goal', text: '', imagePath: '', transition: 'fade', durationMs: 6000,
  goalId: goalId ?? null, ...over,
});

const dockCfg = (over = {}) => ({ enabled: true, position: 'bottom', goalId: null, scale: 1, ...over });

// ── Résolution de la cible ──────────────────────────────────────────────────

check('sans goalId, une carte vise tous les objectifs', () => {
  const { api } = mountBanner();
  api.setGoalEntries([goalEntry('g1', 10, 5), goalEntry('g2', 20, 10)]);
  assert.strictEqual(api.entriesFor(null).length, 2);
  assert.strictEqual(api.entriesFor('').length, 2);
});

check('avec un goalId, une carte ne vise que cet objectif', () => {
  const { api } = mountBanner();
  api.setGoalEntries([goalEntry('g1', 10, 5), goalEntry('g2', 20, 10)]);
  const only = api.entriesFor('g2');
  assert.strictEqual(only.length, 1);
  assert.strictEqual(only[0].config.id, 'g2');
});

check('un goalId supprimé ne retombe pas silencieusement sur tous', () => {
  const { api } = mountBanner();
  api.setGoalEntries([goalEntry('g1', 10, 5)]);
  assert.strictEqual(api.entriesFor('disparu').length, 0);
});

// ── Composition de la rotation ──────────────────────────────────────────────

check('les cartes texte et objectif cohabitent dans l’ordre configuré', () => {
  const { api } = mountBanner();
  api.applyBannerConfig({ cards: [textCard('c1', 'A'), goalCard('c2', 'g1'), textCard('c3', 'B')] });
  api.setGoalEntries([goalEntry('g1', 10, 5)]);
  const list = api.buildRotation();
  assert.strictEqual(list.length, 3);
  assert.strictEqual(list[1].kind, 'goal');
});

check('une carte visant un objectif supprimé est retirée du cycle', () => {
  // Mieux vaut l'ignorer qu'imposer une carte vide six secondes à chaque tour.
  const { api } = mountBanner();
  api.applyBannerConfig({ cards: [textCard('c1', 'A'), goalCard('c2', 'disparu')] });
  api.setGoalEntries([goalEntry('g1', 10, 5)]);
  const list = api.buildRotation();
  assert.strictEqual(list.length, 1);
  assert.strictEqual(list[0].id, 'c1');
});

check('une carte « tous les objectifs » disparaît s’il n’y en a aucun', () => {
  const { api } = mountBanner();
  api.applyBannerConfig({ cards: [goalCard('c1', null)] });
  api.setGoalEntries([]);
  assert.strictEqual(api.buildRotation().length, 0);
});

check('une carte sans kind reste une carte texte', () => {
  // Tout banner.json antérieur : rien ne doit se transformer en barre.
  const { api } = mountBanner();
  api.applyBannerConfig({ cards: [{ id: 'c1', text: 'Ancien', transition: 'fade' }] });
  api.setGoalEntries([]);
  assert.strictEqual(api.buildRotation().length, 1);
});

// ── Signature : le garde-fou anti-redémarrage ───────────────────────────────

check('une valeur qui bouge ne change PAS la signature', () => {
  const { api } = mountBanner();
  api.applyBannerConfig({ cards: [textCard('c1', 'X'), goalCard('c2', 'g1')], dock: dockCfg() });
  api.setGoalEntries([goalEntry('g1', 100, 50)]);
  const before = api.rotationSignature();

  // Un follower de plus : le cas qui arrive plusieurs fois par stream.
  api.setGoalEntries([goalEntry('g1', 101, 50.5)]);
  assert.strictEqual(api.rotationSignature(), before,
    'la rotation serait reconstruite à chaque follower gagné');
});

check('changer la cible d’une carte change la signature', () => {
  const { api } = mountBanner();
  api.setGoalEntries([goalEntry('g1', 10, 5), goalEntry('g2', 20, 10)]);
  api.applyBannerConfig({ cards: [goalCard('c1', 'g1')] });
  const before = api.rotationSignature();
  api.applyBannerConfig({ cards: [goalCard('c1', 'g2')] });
  assert.notStrictEqual(api.rotationSignature(), before);
});

check('changer le type d’une carte change la signature', () => {
  const { api } = mountBanner();
  api.setGoalEntries([goalEntry('g1', 10, 5)]);
  api.applyBannerConfig({ cards: [textCard('c1', 'X')] });
  const before = api.rotationSignature();
  api.applyBannerConfig({ cards: [goalCard('c1', null)] });
  assert.notStrictEqual(api.rotationSignature(), before);
});

check('chaque réglage de barres fixes change la signature', () => {
  const { api } = mountBanner();
  api.setGoalEntries([goalEntry('g1', 10, 5)]);
  const variants = [
    dockCfg({ enabled: false }), dockCfg(), dockCfg({ position: 'top' }),
    dockCfg({ goalId: 'g1' }), dockCfg({ scale: 2 }),
  ];
  const seen = new Set();
  for (const dock of variants) {
    api.applyBannerConfig({ cards: [], dock });
    seen.add(api.rotationSignature());
  }
  assert.strictEqual(seen.size, variants.length, 'deux réglages produisent la même signature');
});

check('barres fixes désactivées : leurs réglages sortent de la signature', () => {
  // Sinon régler un dock éteint relancerait la rotation pour rien.
  const { api } = mountBanner();
  api.setGoalEntries([goalEntry('g1', 10, 5)]);
  api.applyBannerConfig({ cards: [textCard('c1', 'X')], dock: dockCfg({ enabled: false }) });
  const before = api.rotationSignature();
  api.applyBannerConfig({
    cards: [textCard('c1', 'X')],
    dock: dockCfg({ enabled: false, position: 'top', scale: 3 }),
  });
  assert.strictEqual(api.rotationSignature(), before);
});

check('ajouter ou supprimer un objectif change la signature', () => {
  const { api } = mountBanner();
  api.applyBannerConfig({ cards: [goalCard('c1', null)] });
  api.setGoalEntries([goalEntry('g1', 10, 5)]);
  const before = api.rotationSignature();
  api.setGoalEntries([goalEntry('g1', 10, 5), goalEntry('g2', 20, 10)]);
  assert.notStrictEqual(api.rotationSignature(), before);
});

// ── Barres fixes à l'écran ──────────────────────────────────────────────────

check('barres fixes activées : dock ancré, mesuré, place réservée aux cartes', () => {
  const { api, bannerOverlay, goalDock } = mountBanner();
  api.applyBannerConfig({ cards: [], dock: dockCfg({ position: 'bottom' }) });
  api.setGoalEntries([goalEntry('g1', 10, 5), goalEntry('g2', 20, 10)]);
  api.applyAll();

  assert.ok(!goalDock.classList.contains('hide'), 'dock masqué');
  assert.ok(goalDock.classList.contains('dock-bottom'));
  assert.ok(bannerOverlay.classList.contains('dock-bottom'), 'les cartes ne seront pas rétrécies');
  assert.strictEqual(goalDock.children.length, 2);
  // 2 barres x 60px dans le stub : la variable doit suivre, pas rester à 0.
  assert.strictEqual(bannerOverlay.style.getPropertyValue('--goal-dock-h'), '120px');
  assert.strictEqual(goalDock.style.getPropertyValue('--goal-scale'), '1');
});

check('les barres fixes peuvent ne montrer qu’un objectif', () => {
  const { api, goalDock } = mountBanner();
  api.applyBannerConfig({ cards: [], dock: dockCfg({ goalId: 'g2' }) });
  api.setGoalEntries([goalEntry('g1', 10, 5), goalEntry('g2', 20, 10)]);
  api.applyAll();
  assert.strictEqual(goalDock.children.length, 1);
  assert.strictEqual(goalDock.children[0].querySelector('.goal-title').textContent, 'Objectif g2');
});

check('position haute : l’ancrage bascule des deux côtés', () => {
  const { api, bannerOverlay, goalDock } = mountBanner();
  api.applyBannerConfig({ cards: [], dock: dockCfg({ position: 'top' }) });
  api.setGoalEntries([goalEntry('g1', 10, 5)]);
  api.applyAll();
  assert.ok(goalDock.classList.contains('dock-top'));
  assert.ok(bannerOverlay.classList.contains('dock-top'));
  assert.ok(!bannerOverlay.classList.contains('dock-bottom'));
});

check('désactiver les barres fixes vide le dock et libère la place', () => {
  const { api, bannerOverlay, goalDock } = mountBanner();
  api.applyBannerConfig({ cards: [], dock: dockCfg() });
  api.setGoalEntries([goalEntry('g1', 10, 5)]);
  api.applyAll();
  assert.strictEqual(goalDock.children.length, 1);

  api.applyBannerConfig({ cards: [], dock: dockCfg({ enabled: false }) });
  api.applyAll();
  assert.ok(goalDock.classList.contains('hide'));
  assert.strictEqual(goalDock.children.length, 0, 'des barres périmées restent dans le dock');
  assert.strictEqual(bannerOverlay.style.getPropertyValue('--goal-dock-h'), '0px');
  assert.ok(!bannerOverlay.classList.contains('dock-bottom'));
});

check('un dock absent de la charge utile vaut « désactivé »', () => {
  const { api, goalDock } = mountBanner();
  api.applyBannerConfig({ cards: [textCard('c1', 'X')] });
  api.setGoalEntries([goalEntry('g1', 10, 5)]);
  api.applyAll();
  assert.ok(goalDock.classList.contains('hide'));
});

// ── État vide ───────────────────────────────────────────────────────────────

check('sans carte mais avec barres fixes, l’état vide disparaît', () => {
  const { api, emptyState } = mountBanner();
  api.applyBannerConfig({ cards: [], dock: dockCfg() });
  api.setGoalEntries([goalEntry('g1', 10, 5)]);
  api.applyAll();
  assert.strictEqual(emptyState.style.display, 'none');
});

check('rien de configuré du tout : l’état vide reste affiché', () => {
  const { api, emptyState } = mountBanner();
  api.applyBannerConfig({ cards: [], dock: dockCfg({ enabled: false }) });
  api.setGoalEntries([]);
  api.applyAll();
  assert.strictEqual(emptyState.style.display, 'flex');
});

// ── Carte d'objectif à l'écran ──────────────────────────────────────────────

check('la carte d’objectif porte son propre jeu de barres, à jour', () => {
  const { api } = mountBanner();
  api.applyBannerConfig({ cards: [goalCard('c1', null)] });
  api.setGoalEntries([goalEntry('g1', 100, 50)]);
  api.applyAll();

  const { cards, currentCardElement } = api.state();
  assert.strictEqual(cards.length, 1);
  assert.ok(currentCardElement && currentCardElement.goalBars, 'la carte n’a pas ses barres');
  const holder = currentCardElement.querySelector('.goal-dock-card');
  assert.strictEqual(holder.children.length, 1);
  assert.strictEqual(holder.querySelector('.goal-current').textContent, '100');

  // Une nouvelle valeur doit atteindre la carte affichée sans la reconstruire.
  api.setGoalEntries([goalEntry('g1', 140, 70)]);
  api.applyAll();
  assert.strictEqual(api.state().currentCardElement.uid, currentCardElement.uid,
    'la carte a été recréée alors que seule la valeur a changé');
  assert.strictEqual(holder.querySelector('.goal-current').textContent, '140');
});

check('une carte ciblée n’affiche que son objectif, même après rafraîchissement', () => {
  const { api } = mountBanner();
  api.applyBannerConfig({ cards: [goalCard('c1', 'g2')] });
  api.setGoalEntries([goalEntry('g1', 10, 5), goalEntry('g2', 20, 10)]);
  api.applyAll();

  const holder = api.state().currentCardElement.querySelector('.goal-dock-card');
  assert.strictEqual(holder.children.length, 1);

  // Le rafraîchissement de valeur doit rejouer la même sélection, pas tout afficher.
  api.setGoalEntries([goalEntry('g1', 11, 6), goalEntry('g2', 21, 11)]);
  api.applyAll();
  assert.strictEqual(holder.children.length, 1);
  assert.strictEqual(holder.querySelector('.goal-current').textContent, '21');
});

// ── Habillage bannière ──────────────────────────────────────────────────────

check('la carte d’objectif porte la classe d’habillage bannière', () => {
  // `.goal-banner` est ce qui bascule le composant partagé sur le dégradé animé
  // et les proportions des cartes texte.
  const { api } = mountBanner();
  api.applyBannerConfig({ cards: [goalCard('c1', null)], dock: dockCfg() });
  api.setGoalEntries([goalEntry('g1', 10, 5)]);
  api.applyAll();

  const holder = api.state().currentCardElement.querySelector('.goal-dock-card');
  assert.ok(holder.classList.contains('goal-banner'), 'carte sans habillage');
});

check('les barres fixes déclarent l’habillage dans le template', () => {
  // Le dock est écrit en dur dans le HTML, pas construit par le JS : c'est la
  // déclaration qu'il faut vérifier.
  const html = fs.readFileSync(`${TPL_DIR}/banner.html`, 'utf8');
  const dockTag = /<div[^>]*id="goalDock"[^>]*>/.exec(html);
  assert.ok(dockTag, 'élément #goalDock introuvable');
  assert.ok(/class="[^"]*\bgoal-banner\b/.test(dockTag[0]), dockTag[0]);
});

check('l’overlay /goal n’hérite pas de l’habillage bannière', () => {
  // Le composant est partagé : une variante qui fuiterait sur /goal changerait
  // l'incrustation posée sur la vidéo, ce qui n'est pas le sujet.
  const html = fs.readFileSync(`${TPL_DIR}/goal.html`, 'utf8');
  assert.ok(!html.includes('goal-banner'), '/goal ne doit pas poser la classe');

  const partial = fs.readFileSync(`${TPL_DIR}/partials/_goal_bars.html`, 'utf8');
  // Toutes les règles de la variante doivent être portées par la classe.
  const variantRules = partial.split('.goal-banner').length - 1;
  assert.ok(variantRules > 5, 'la variante semble absente du composant partagé');
  assert.ok(!/\n\s*\.goal-title\s*\{[^}]*background-clip/s.test(partial),
    'le dégradé fuit sur la règle de base de .goal-title');
});

check('la carte d’objectif ne construit aucun HTML par concaténation', () => {
  const { api } = mountBanner();
  api.applyBannerConfig({ cards: [goalCard('c1', null)] });
  const hostile = { ...goalEntry('g1', 10, 5), warning: '<script>alert(1)</script>' };
  hostile.config.title = '<img onerror=alert(1)>';
  api.setGoalEntries([hostile]);
  api.applyAll();

  const card = api.state().currentCardElement;
  assert.strictEqual(card.querySelector('.goal-title').textContent, '<img onerror=alert(1)>');
  assert.strictEqual(card.querySelector('.goal-title').children.length, 0);
  assert.strictEqual(card.querySelector('.goal-warning').children.length, 0);
});

console.log(`\n  ${passed} assertions de rotation OK`);
