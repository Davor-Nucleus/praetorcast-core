'use strict';
/**
 * Carte « Dernier événement » de la bannière, et phrase animée des alertes.
 *
 * Deux sujets voisins : tous deux affichent un pseudo venu de Twitch, et tous deux
 * s'appuient sur un partiel partagé (`_event_card.html`, `_text_render.html`).
 */
const fs = require('fs');
const path = require('path');
const assert = require('assert');
const { el, buildGoalTemplate, makeDocument } = require('./dom-stub.cjs');

const TPL_DIR = path.resolve(__dirname, '../../templates');

function inlineScript(file) {
  const html = fs.readFileSync(`${TPL_DIR}/${file}`, 'utf8');
  return /<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/.exec(html)[1];
}

function loadEventCard(document) {
  return new Function(
    'document',
    inlineScript('partials/_event_card.html') + `
    ; return { EVENT_CARD_FILTERS, eventMatches, latestEvent, describeEvent, buildEventCardContent };`
  )(document);
}

const event = (kind, userName, amount = 0, extra = {}) => ({ kind, userName, amount, months: 0, atMs: Date.now(), ...extra });

let passed = 0;
const check = (label, fn) => {
  try { fn(); passed++; console.log(`  ok    ${label}`); }
  catch (e) { console.log(`  ECHEC ${label}\n        ${e.message}`); process.exitCode = 1; }
};

// ── Partiel ─────────────────────────────────────────────────────────────────

check('sans filtre, tout compte sauf les points de chaîne', () => {
  const card = loadEventCard(makeDocument());
  assert.ok(card.eventMatches(event('follow', 'a'), ''));
  assert.ok(card.eventMatches(event('raid', 'a'), null));
  assert.ok(!card.eventMatches(event('channel_points', 'a'), ''));
  assert.ok(card.eventMatches(event('channel_points', 'a'), 'channel_points'));
});

check('le filtre « Abonnements » couvre Prime et réabonnements', () => {
  const card = loadEventCard(makeDocument());
  for (const kind of ['sub', 'resub', 'prime']) assert.ok(card.eventMatches(event(kind, 'a'), 'sub'), kind);
  assert.ok(!card.eventMatches(event('gift', 'a'), 'sub'));
});

check('un événement de test n\'est jamais retenu', () => {
  const card = loadEventCard(makeDocument());
  assert.ok(!card.eventMatches(event('raid', 'TestUser', 10, { test: true }), ''));
});

check('le plus récent l\'emporte (la liste arrive du plus récent au plus ancien)', () => {
  const card = loadEventCard(makeDocument());
  const events = [event('follow', 'dernier'), event('raid', 'raideur', 12), event('follow', 'ancien')];
  assert.strictEqual(card.latestEvent(events, 'follow').userName, 'dernier');
  assert.strictEqual(card.latestEvent(events, 'raid').userName, 'raideur');
  assert.strictEqual(card.latestEvent(events, 'cheer'), null);
});

check('titre et précision suivent le type, au singulier comme au pluriel', () => {
  const card = loadEventCard(makeDocument());
  assert.deepStrictEqual(card.describeEvent(event('follow', 'a')), { label: 'Dernier follower', detail: '' });
  assert.strictEqual(card.describeEvent(event('raid', 'a', 1)).detail, '1 spectateur');
  assert.match(card.describeEvent(event('raid', 'a', 42)).detail, /^42 spectateurs$/);
  assert.strictEqual(card.describeEvent(event('resub', 'a', 1000, { months: 7 })).detail, '7 mois');
  assert.strictEqual(card.describeEvent(event('sub', 'a', 1000)).detail, '', 'le tier 1 ne mérite pas de mention');
  assert.strictEqual(card.describeEvent(event('sub', 'a', 3000)).detail, 'Tier 3');
});

check('le pseudo est posé en texte, jamais interprété', () => {
  const document = makeDocument();
  const card = loadEventCard(document);
  const node = card.buildEventCardContent(event('follow', '<img src=x onerror=alert(1)>'));
  const name = node.querySelector('.event-name');
  assert.strictEqual(name.textContent, '<img src=x onerror=alert(1)>');
  assert.ok(!name.innerHTML.includes('<img'), 'le pseudo ne doit pas devenir du HTML');
});

// ── Overlay /banner ─────────────────────────────────────────────────────────

function mountBanner() {
  const bannerOverlay = el('div', 'banner-overlay');
  const emptyState = el('div', 'empty-state');
  const goalDock = el('div', 'goal-dock goal-banner hide');
  bannerOverlay.appendChild(emptyState);
  bannerOverlay.appendChild(goalDock);
  const document = makeDocument({ goalTpl: buildGoalTemplate(), bannerOverlay, emptyState, goalDock });

  const createGoalBars = new Function(
    'document', inlineScript('partials/_goal_bars.html') + '\n; return createGoalBars;'
  )(document);
  const card = loadEventCard(document);

  const api = new Function(
    'document', 'window', 'createGoalBars', 'latestEvent', 'buildEventCardContent', 'ResizeObserver',
    'requestAnimationFrame', 'setTimeout', 'clearTimeout', 'console', 'location', 'WebSocket',
    inlineScript('banner.html') + `
    ; return {
        buildRotation, rotationSignature, applyAll, applyEventMessage, applyBannerConfig,
        state: () => ({ cards, currentCardElement, recentEvents }),
    };`
  )(
    document, { addEventListener() {} }, createGoalBars, card.latestEvent, card.buildEventCardContent,
    undefined, (fn) => fn(), () => 0, () => {}, { error() {}, log() {} },
    { protocol: 'http:', host: '127.0.0.1:3000' },
    function () { throw new Error('aucune WS pendant le test'); }
  );
  return { api, bannerOverlay };
}

const EVENT_CARD = { id: 'e1', kind: 'event', eventKind: 'follow', transition: 'fade' };

check('une carte d\'événement est sautée tant qu\'aucun événement de son type n\'existe', () => {
  const { api } = mountBanner();
  api.applyBannerConfig({ cards: [{ id: 't', kind: 'text', text: 'Salut' }, EVENT_CARD] });
  api.applyEventMessage({ type: 'snapshot', events: [event('raid', 'raideur', 5)] });
  assert.deepStrictEqual(api.buildRotation().map((c) => c.id), ['t']);

  api.applyEventMessage({ type: 'event', event: event('follow', 'Ronni') });
  assert.deepStrictEqual(api.buildRotation().map((c) => c.id), ['t', 'e1']);
});

check('la carte affichée se met à jour en place au follow suivant', () => {
  const { api } = mountBanner();
  api.applyBannerConfig({ cards: [EVENT_CARD] });
  api.applyEventMessage({ type: 'snapshot', events: [event('follow', 'premier')] });
  const shown = api.state().currentCardElement;
  assert.ok(shown.textContent.includes('premier'), shown.textContent);
  const signature = api.rotationSignature();

  api.applyEventMessage({ type: 'event', event: event('follow', 'second') });
  assert.strictEqual(api.state().currentCardElement, shown, 'la carte ne doit pas être reconstruite');
  assert.strictEqual(api.rotationSignature(), signature, 'le cycle ne doit pas repartir');
  assert.ok(shown.textContent.includes('second'), shown.textContent);
  assert.ok(!shown.textContent.includes('premier'));
});

check('un événement de test n\'atteint pas la bannière', () => {
  const { api } = mountBanner();
  api.applyBannerConfig({ cards: [EVENT_CARD] });
  api.applyEventMessage({ type: 'snapshot', events: [event('follow', 'vrai')] });
  api.applyEventMessage({ type: 'event', event: event('follow', 'TestUser', 0, { test: true }) });
  assert.strictEqual(api.state().recentEvents.length, 1);
  assert.ok(api.state().currentCardElement.textContent.includes('vrai'));
});

check('réglages d\'effets et tests d\'effets sont ignorés par la bannière', () => {
  const { api } = mountBanner();
  api.applyEventMessage({ type: 'snapshot', events: [event('follow', 'a')] });
  api.applyEventMessage({ type: 'config', effects: {} });
  api.applyEventMessage({ type: 'effect-test', effect: 'rain' });
  assert.strictEqual(api.state().recentEvents.length, 1);
});

// ── Phrase des alertes (/channel-points) ────────────────────────────────────

function mountAlerts() {
  const document = makeDocument({ cpOverlay: el('div', 'cp-overlay') });
  const timers = [];
  const createTextRenderer = new Function(
    'document', 'setTimeout', 'clearTimeout',
    inlineScript('partials/_text_render.html') + '\n; return createTextRenderer;'
  )(document, (fn) => { timers.push(fn); return timers.length; }, () => {});

  const window = { addEventListener() {} };
  const api = new Function(
    'document', 'window', 'createTextRenderer', 'requestAnimationFrame', 'setTimeout', 'console',
    inlineScript('channel_point.html') + '\n; return { buildWrapper, phraseSection };'
  )(document, window, createTextRenderer, (fn) => fn(), () => 0, { error() {}, warn() {} });
  return { api, timers };
}

const find = (node, cls) => node.querySelector('.' + cls);

check('une ligne sans réglage garde son rendu : dégradé animé, sans entrée', () => {
  const { api } = mountAlerts();
  const wrapper = api.buildWrapper(
    { phrase: 'Merci {{user}} !', imagePath: '', soundPath: '' },
    { user_name: 'Ronni', amount: 0 }
  );
  const line = find(wrapper, 'text-line');
  const content = find(wrapper, 'text-content');
  assert.strictEqual(content.textContent, 'Merci Ronni !');
  assert.ok(!line.className.includes('anim-'), line.className);
  assert.ok(content.classList.contains('fx-gradient'), content.className);
});

check('l\'animation choisie est appliquée à la phrase, puis son effet', () => {
  const { api, timers } = mountAlerts();
  const wrapper = api.buildWrapper(
    { phrase: '{{amount}} bits !', imagePath: '', soundPath: '', textAnimation: 'stamp', textEffect: 'heartbeat' },
    { user_name: 'x', amount: 500 }
  );
  assert.ok(find(wrapper, 'text-line').classList.contains('anim-stamp'));
  const content = find(wrapper, 'text-content');
  assert.ok(!content.classList.contains('fx-heartbeat'), 'l\'effet attend la fin de l\'entrée');
  timers.forEach((fn) => fn());
  assert.ok(content.classList.contains('fx-heartbeat'));
});

check('le pseudo d\'un viewer reste du texte dans la phrase', () => {
  const { api } = mountAlerts();
  const wrapper = api.buildWrapper(
    { phrase: '{{user}}', imagePath: '', soundPath: '', textAnimation: 'typewriter' },
    { user_name: '<b>gras</b>' }
  );
  // Machine à écrire : une lettre par span, toutes en texte.
  assert.strictEqual(find(wrapper, 'text-content').textContent, '<b>gras</b>');
  assert.ok(!find(wrapper, 'text-content').innerHTML.includes('<b>'));
});

console.log(`  ${passed} verification(s) OK`);
