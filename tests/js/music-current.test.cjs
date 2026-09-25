'use strict';
/**
 * Barre de progression de `/music-current`.
 *
 * JanusCore n'envoie une position qu'aux changements d'état (piste, pause, reprise) :
 * c'est l'overlay qui la fait avancer entre deux messages. Ce qui se teste ici :
 * l'extrapolation, le gel en pause, la borne à la durée, et les cas où la barre doit
 * rester masquée — option coupée, durée inconnue, rien en lecture.
 */
const fs = require('fs');
const path = require('path');
const assert = require('assert');
const { el, makeDocument } = require('./dom-stub.cjs');

const TPL = path.resolve(__dirname, '../../templates/music_current.html');

function inlineScript() {
  const html = fs.readFileSync(TPL, 'utf8');
  return /<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/.exec(html)[1]
    // Balise Askama : remplacée comme le ferait le rendu serveur.
    .replace(/\{\{\s*music_port\s*\}\}/g, '3001');
}

function mountOverlay() {
  const ids = {
    musicTitle: el('h1'),
    iconWrapper: el('div'),
    progress: el('div', 'progress'),
    progressFill: el('div', 'progress-fill'),
    progressElapsed: el('span'),
    progressTotal: el('span'),
  };
  ids.progress.hidden = true;

  let now = 0;
  const clock = {
    performance: { now: () => now },
    set: (ms) => { now = ms; },
  };

  const api = new Function(
    'document', 'window', 'WebSocket', 'console', 'setTimeout', 'clearTimeout',
    'performance', 'requestAnimationFrame',
    inlineScript() + '\n; return { handleMessage, renderProgress, formatTime };'
  )(
    makeDocument(ids),
    { addEventListener() {}, location: { hostname: '127.0.0.1' } },
    function WebSocketStub() { return { addEventListener() {} }; },
    { error() {}, log() {} },
    (fn) => { fn(); return 0; },
    () => {},
    clock.performance,
    () => 0
  );

  return { api, ids, clock };
}

/** Un message de JanusCore, piste en cours de lecture. */
function message(overrides = {}) {
  return {
    has_sink: true,
    paused: false,
    current_music: 'Zoltraak.mp3',
    metadata: { filename: 'Zoltraak.mp3', duration_ms: 180_000, cover_art: null },
    progress_bar_enabled: true,
    position_ms: 30_000,
    ...overrides,
  };
}

const scale = (ids) => ids.progressFill.style.transform;

let passed = 0;
const check = (label, fn) => {
  try { fn(); passed++; console.log(`  ok    ${label}`); }
  catch (e) { console.log(`  ECHEC ${label}\n        ${e.message}`); process.exitCode = 1; }
};

check('le temps est formaté en m:ss, et en h:mm:ss au-delà d\'une heure', () => {
  const { api } = mountOverlay();
  assert.strictEqual(api.formatTime(0), '0:00');
  assert.strictEqual(api.formatTime(65_000), '1:05');
  assert.strictEqual(api.formatTime(59_999), '0:59');
  assert.strictEqual(api.formatTime(3_725_000), '1:02:05');
});

check('option activée : la barre s\'affiche avec la durée et la position reçues', () => {
  const { api, ids } = mountOverlay();
  api.handleMessage(message());
  assert.strictEqual(ids.progress.hidden, false);
  assert.strictEqual(ids.progressTotal.textContent, '3:00');
  assert.strictEqual(ids.progressElapsed.textContent, '0:30');
  assert.strictEqual(scale(ids), `scaleX(${30_000 / 180_000})`);
});

check('entre deux messages, la position avance avec l\'horloge locale', () => {
  const { api, ids, clock } = mountOverlay();
  clock.set(1_000);
  api.handleMessage(message());
  clock.set(11_000);
  api.renderProgress(11_000);
  assert.strictEqual(ids.progressElapsed.textContent, '0:40');
  assert.strictEqual(scale(ids), `scaleX(${40_000 / 180_000})`);
});

check('en pause, la position reste figée', () => {
  const { api, ids } = mountOverlay();
  api.handleMessage(message({ paused: true }));
  api.renderProgress(60_000);
  assert.strictEqual(ids.progressElapsed.textContent, '0:30');
});

check('la position ne dépasse jamais la durée', () => {
  // Entre la fin d'une piste et l'ouverture de la suivante, JanusCore peut mettre
  // jusqu'à une demi-seconde : la barre doit rester pleine, pas déborder.
  const { api, ids } = mountOverlay();
  api.handleMessage(message({ position_ms: 179_000 }));
  api.renderProgress(10_000);
  assert.strictEqual(ids.progressElapsed.textContent, '3:00');
  assert.strictEqual(scale(ids), 'scaleX(1)');
});

check('option désactivée : la barre reste masquée', () => {
  const { api, ids } = mountOverlay();
  api.handleMessage(message({ progress_bar_enabled: false }));
  assert.strictEqual(ids.progress.hidden, true);
});

check('désactiver l\'option en direct masque la barre', () => {
  const { api, ids } = mountOverlay();
  api.handleMessage(message());
  api.handleMessage(message({ progress_bar_enabled: false }));
  assert.strictEqual(ids.progress.hidden, true);
});

check('durée inconnue : pas de barre plutôt qu\'une barre fausse', () => {
  const { api, ids } = mountOverlay();
  api.handleMessage(message({ metadata: { filename: 'x.mp3', duration_ms: null } }));
  assert.strictEqual(ids.progress.hidden, true);
});

check('rien en lecture : la barre disparaît', () => {
  const { api, ids } = mountOverlay();
  api.handleMessage(message());
  api.handleMessage(message({ has_sink: false, current_music: null, metadata: null, position_ms: null }));
  assert.strictEqual(ids.progress.hidden, true);
});

check('une ancienne version de JanusCore (sans position) ne montre pas de barre', () => {
  const { api, ids } = mountOverlay();
  const legacy = message();
  delete legacy.position_ms;
  delete legacy.progress_bar_enabled;
  api.handleMessage(legacy);
  assert.strictEqual(ids.progress.hidden, true);
});

console.log(`  ${passed} verification(s) OK`);
