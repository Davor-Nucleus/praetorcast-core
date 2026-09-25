'use strict';
/**
 * Overlays d'effets : pluie d'emotes, cadre caméra, visualiseur — et leur page de
 * réglages, /effects-config.
 *
 * Ce qui se teste ici, c'est la décision : quel événement fait pleuvoir, quelle
 * force d'éclat pour quel montant, comment 64 bandes deviennent N barres. Le rendu
 * (CSS, canvas) se vérifie dans OBS.
 */
const fs = require('fs');
const path = require('path');
const assert = require('assert');
const { el, makeDocument } = require('./dom-stub.cjs');

const TPL_DIR = path.resolve(__dirname, '../../templates');

function inlineScript(file) {
  const html = fs.readFileSync(`${TPL_DIR}/${file}`, 'utf8');
  return /<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/.exec(html)[1]
    .replace(/\{\{\s*music_port\s*\}\}/g, '3001');
}

const NO_WS = function () { throw new Error('aucune WS pendant le test'); };
const WINDOW = { addEventListener() {} };

/** Réglages par défaut, tels que `models::effects` les sérialise. */
const RAIN = {
  onRaid: true, raidMin: 0, onCheer: true, cheerMin: 500, onGift: true, giftMin: 5,
  onSub: false, onGoal: true, count: 60, durationMs: 5000, size: 1,
};
const FRAME = { thickness: 6, animated: true, glow: true, pulse: true, pulseOnFollow: true, intensity: 1 };

// ── Pluie d'emotes ──────────────────────────────────────────────────────────

function mountRain() {
  const body = el('body');
  const document = Object.assign(makeDocument({}), { body });
  const api = new Function(
    'document', 'window', 'WebSocket', 'fetch', 'location', 'setTimeout',
    inlineScript('emote_rain.html') + `
    ; return {
        shouldRain, newlyReached, startRain, handleMessage, handleGoals,
        setEmotes: (v) => { emotes = v; },
        rain: () => rain,
    };`
  )(document, WINDOW, NO_WS, async () => ({ ok: false }), { protocol: 'http:', host: 'x' }, () => 0);
  return { api, body };
}

const drops = (body) => body.querySelectorAll('.drop').length;

let passed = 0;
const check = (label, fn) => {
  try { fn(); passed++; console.log(`  ok    ${label}`); }
  catch (e) { console.log(`  ECHEC ${label}\n        ${e.message}`); process.exitCode = 1; }
};

check('un raid fait pleuvoir, dès le premier spectateur par défaut', () => {
  const { api } = mountRain();
  assert.ok(api.shouldRain({ kind: 'raid', amount: 1 }, RAIN));
  assert.ok(!api.shouldRain({ kind: 'raid', amount: 3 }, { ...RAIN, raidMin: 10 }));
  assert.ok(!api.shouldRain({ kind: 'raid', amount: 50 }, { ...RAIN, onRaid: false }));
});

check('les bits et les dons ne font pleuvoir qu\'à partir de leur seuil', () => {
  const { api } = mountRain();
  assert.ok(!api.shouldRain({ kind: 'cheer', amount: 499 }, RAIN));
  assert.ok(api.shouldRain({ kind: 'cheer', amount: 500 }, RAIN));
  assert.ok(!api.shouldRain({ kind: 'gift', amount: 4 }, RAIN));
  assert.ok(api.shouldRain({ kind: 'gift', amount: 5 }, RAIN));
});

check('les abonnements ne font pleuvoir que si la case est cochée', () => {
  const { api } = mountRain();
  for (const kind of ['sub', 'resub', 'prime']) {
    assert.ok(!api.shouldRain({ kind, amount: 1000 }, RAIN), kind);
    assert.ok(api.shouldRain({ kind, amount: 1000 }, { ...RAIN, onSub: true }), kind);
  }
});

check('follows et points de chaîne ne font jamais pleuvoir', () => {
  const { api } = mountRain();
  const all = { ...RAIN, onSub: true };
  assert.ok(!api.shouldRain({ kind: 'follow' }, all));
  assert.ok(!api.shouldRain({ kind: 'channel_points' }, all));
  assert.ok(!api.shouldRain({ kind: 'raid', amount: 5 }, null), 'sans réglages reçus, rien');
});

const goalEntry = (id, target, percent) => ({ config: { id, target }, current: 0, percent });

check('un objectif qui atteint sa cible fait pleuvoir une fois', () => {
  const { api, body } = mountRain();
  api.handleMessage({ type: 'config', effects: { rain: RAIN } });

  // Premier état reçu : sert de référence, même si un objectif est déjà rempli.
  api.handleGoals({ goals: [goalEntry('a', 100, 100), goalEntry('b', 50, 40)] });
  assert.strictEqual(drops(body), 0, 'un objectif déjà atteint à l\'ouverture ne doit pas pleuvoir');

  api.handleGoals({ goals: [goalEntry('a', 100, 100), goalEntry('b', 50, 100)] });
  assert.strictEqual(drops(body), RAIN.count);

  // Il reste atteint : pas de nouvelle averse à chaque follower.
  api.handleGoals({ goals: [goalEntry('a', 100, 100), goalEntry('b', 50, 104)] });
  assert.strictEqual(drops(body), RAIN.count);
});

check('un objectif à cible nulle ne compte pas comme atteint', () => {
  const { api } = mountRain();
  const first = api.newlyReached(null, []);
  const next = api.newlyReached(first.reached, [goalEntry('z', 0, 100)]);
  assert.strictEqual(next.count, 0);
});

check('l\'averse lâche le nombre réglé, avec les emotes de la chaîne', () => {
  const { api, body } = mountRain();
  api.setEmotes(['https://x/1', 'https://x/2']);
  api.startRain({ ...RAIN, count: 12 }, () => 0.5);
  const all = body.querySelectorAll('.drop');
  assert.strictEqual(all.length, 12);
  assert.strictEqual(all[0].tagName, 'IMG');
  assert.ok(all[0].src.startsWith('https://x/'));
  assert.ok(all[0].style.getPropertyValue('--duration').endsWith('ms'));
});

check('sans emote chargée, des symboles tombent à la place', () => {
  const { api, body } = mountRain();
  api.setEmotes([]);
  api.startRain({ ...RAIN, count: 3 }, () => 0);
  const all = body.querySelectorAll('.drop');
  assert.strictEqual(all[0].tagName, 'SPAN');
  assert.ok(all[0].textContent.length > 0);
});

check('le nombre d\'emotes à l\'écran reste plafonné', () => {
  const { api, body } = mountRain();
  for (let i = 0; i < 5; i++) api.startRain({ ...RAIN, count: 300 }, Math.random);
  assert.strictEqual(drops(body), 600);
});

check('le bouton « Tester » de /effects-config fait pleuvoir sans condition', () => {
  const { api, body } = mountRain();
  api.handleMessage({ type: 'config', effects: { rain: { ...RAIN, onRaid: false, onCheer: false } } });
  api.handleMessage({ type: 'effect-test', effect: 'frame' });
  assert.strictEqual(drops(body), 0, 'le test du cadre ne concerne pas la pluie');
  api.handleMessage({ type: 'effect-test', effect: 'rain' });
  assert.strictEqual(drops(body), RAIN.count);
});

// ── Cadre caméra ────────────────────────────────────────────────────────────

function mountFrame() {
  const ids = { frameWrap: el('div', 'frame-wrap glow'), frame: el('div', 'frame animated') };
  const api = new Function(
    'document', 'window', 'WebSocket', 'location', 'setTimeout',
    inlineScript('camera_frame.html') + '\n; return { pulseStrength, applyConfig, handleMessage };'
  )(makeDocument(ids), WINDOW, NO_WS, { protocol: 'http:', host: 'x' }, () => 0);
  return { api, ids };
}

check('l\'éclat grandit avec le montant, sans exploser', () => {
  const { api } = mountFrame();
  const raid = (amount) => api.pulseStrength({ kind: 'raid', amount }, FRAME);
  assert.ok(raid(50) > raid(5), 'un gros raid doit briller davantage');
  assert.ok(raid(100000) <= 2, 'plafonné à 2');
  assert.ok(api.pulseStrength({ kind: 'follow' }, FRAME) < api.pulseStrength({ kind: 'sub', amount: 1000 }, FRAME));
});

check('les réglages coupent l\'éclat ou l\'épargnent aux follows', () => {
  const { api } = mountFrame();
  assert.strictEqual(api.pulseStrength({ kind: 'follow' }, { ...FRAME, pulseOnFollow: false }), 0);
  assert.strictEqual(api.pulseStrength({ kind: 'sub', amount: 1000 }, { ...FRAME, pulse: false }), 0);
  assert.strictEqual(
    api.pulseStrength({ kind: 'sub', amount: 1000 }, { ...FRAME, intensity: 2 }),
    2 * api.pulseStrength({ kind: 'sub', amount: 1000 }, FRAME)
  );
});

check('les réglages s\'appliquent en direct au cadre', () => {
  const { api, ids } = mountFrame();
  api.applyConfig({ ...FRAME, thickness: 14, glow: false, animated: false });
  assert.strictEqual(ids.frameWrap.style.getPropertyValue('--t'), '14px');
  assert.ok(!ids.frameWrap.classList.contains('glow'));
  assert.ok(!ids.frame.classList.contains('animated'));
});

check('un événement ou un test fait briller le cadre', () => {
  const { api, ids } = mountFrame();
  api.handleMessage({ type: 'config', effects: { frame: FRAME } });
  api.handleMessage({ type: 'event', event: { kind: 'cheer', amount: 1000 } });
  assert.ok(ids.frameWrap.classList.contains('pulse'));

  const other = mountFrame();
  other.api.handleMessage({ type: 'config', effects: { frame: { ...FRAME, pulse: false } } });
  other.api.handleMessage({ type: 'effect-test', effect: 'frame' });
  assert.ok(other.ids.frameWrap.classList.contains('pulse'), 'le test brille même éclat coupé');
});

// ── Visualiseur ─────────────────────────────────────────────────────────────

function mountVisualizer() {
  const canvas = el('canvas');
  canvas.getContext = () => ({});
  const api = new Function(
    'document', 'window', 'WebSocket', 'location',
    inlineScript('music_visualizer.html') + `
    ; return { resample, smooth, applyLevels, applySettings,
               target: () => target, settings: () => settings };`
  )(makeDocument({ viz: canvas }), WINDOW, NO_WS, { protocol: 'http:', host: 'x', hostname: 'x' });
  return { api };
}

check('64 bandes se regroupent en moins de barres par moyenne', () => {
  const { api } = mountVisualizer();
  const levels = Array.from({ length: 64 }, (_, i) => (i < 32 ? 0 : 1));
  const bars = api.resample(levels, 8);
  assert.strictEqual(bars.length, 8);
  assert.deepStrictEqual(bars, [0, 0, 0, 0, 1, 1, 1, 1]);
});

check('et s\'interpolent quand il faut plus de barres que de bandes', () => {
  const { api } = mountVisualizer();
  const bars = api.resample([0, 1], 5);
  assert.deepStrictEqual(bars.map((v) => Math.round(v * 100) / 100), [0, 0.25, 0.5, 0.75, 1]);
  assert.deepStrictEqual(api.resample([], 4), [0, 0, 0, 0], 'JanusCore déconnecté : barres à plat');
});

check('les barres montent vite et redescendent lentement', () => {
  const { api } = mountVisualizer();
  const up = api.smooth(0, 1, 0.016);
  const down = api.smooth(1, 0, 0.016);
  assert.ok(up > 0.4, `montée trop lente : ${up}`);
  assert.ok(1 - down < 0.1, `descente trop rapide : ${down}`);
  assert.strictEqual(api.smooth(0.5, 0.5, 1), 0.5);
});

check('la sensibilité amplifie sans dépasser le haut du cadre', () => {
  const { api } = mountVisualizer();
  api.applySettings({ bars: 1000, style: 'inconnu', sensitivity: 2 });
  assert.strictEqual(api.settings().bars, 128);
  assert.strictEqual(api.settings().style, 'bars');
  api.applyLevels([51, 255]);
  assert.deepStrictEqual(api.target(), [0.4, 1]);
});

// ── Page /effects-config ────────────────────────────────────────────────────

async function mountEffectsConfig(respond) {
  const ids = { toast: el('div', 'toast') };
  const inputs = [
    'rainOnRaid', 'rainRaidMin', 'rainOnCheer', 'rainCheerMin', 'rainOnGift', 'rainGiftMin',
    'rainOnSub', 'rainOnGoal', 'rainCount', 'rainDuration', 'rainSize',
    'frameThickness', 'frameAnimated', 'frameGlow', 'framePulse', 'framePulseOnFollow',
    'frameIntensity', 'vizBars', 'vizStyle', 'vizSensitivity',
  ];
  for (const id of inputs) ids[id] = el('input');

  const sent = [];
  const fetchStub = async (url, init) => {
    sent.push({ url, init });
    return respond(url, init);
  };
  const api = new Function(
    'document', 'window', 'fetch', 'location', 'navigator', 'setTimeout', 'clearTimeout',
    inlineScript('effects_config.html') + '\n; return { readForm, fillForm, saveConfig, testEffect };'
  )(makeDocument(ids), WINDOW, fetchStub, { origin: 'http://127.0.0.1:3000' }, {}, () => 0, () => {});
  return { api, ids, sent };
}

const DEFAULTS = {
  rain: RAIN,
  frame: FRAME,
  visualizer: { bars: 48, style: 'mirror', sensitivity: 1.5 },
};

(async () => {
  const run = async (label, fn) => {
    try { await fn(); passed++; console.log(`  ok    ${label}`); }
    catch (e) { console.log(`  ECHEC ${label}\n        ${e.message}`); process.exitCode = 1; }
  };

  await run('le formulaire relit exactement ce qu\'il affiche', async () => {
    const { api } = await mountEffectsConfig(() => ({ ok: true }));
    api.fillForm(DEFAULTS);
    assert.deepStrictEqual(api.readForm(), DEFAULTS);
  });

  await run('la durée se saisit en secondes et part en millisecondes', async () => {
    const { api, ids } = await mountEffectsConfig(() => ({ ok: true }));
    api.fillForm(DEFAULTS);
    assert.strictEqual(ids.rainDuration.value, 5);
    ids.rainDuration.value = '7.5';
    assert.strictEqual(api.readForm().rain.durationMs, 7500);
  });

  await run('l\'enregistrement affiche les valeurs bornées renvoyées par le serveur', async () => {
    const bounded = { ...DEFAULTS, rain: { ...RAIN, count: 300 } };
    const { api, ids, sent } = await mountEffectsConfig(() => ({ ok: true, json: async () => bounded }));
    api.fillForm(DEFAULTS);
    ids.rainCount.value = '100000';
    await api.saveConfig();
    assert.strictEqual(sent[0].url, '/api/effects-config');
    assert.strictEqual(JSON.parse(sent[0].init.body).rain.count, 100000);
    assert.strictEqual(ids.rainCount.value, 300, 'la valeur retenue doit remplacer la saisie');
    assert.ok(ids.toast.className.includes('saved'));
  });

  await run('« Tester » envoie l\'effet et signale une source absente', async () => {
    const { api, ids, sent } = await mountEffectsConfig(() => ({ ok: true, json: async () => ({ sources: 0 }) }));
    await api.testEffect('rain');
    assert.strictEqual(sent[0].url, '/api/effects/test');
    assert.deepStrictEqual(JSON.parse(sent[0].init.body), { effect: 'rain' });
    assert.ok(ids.toast.textContent.includes('Aucune source'), ids.toast.textContent);
  });

  console.log(`  ${passed} verification(s) OK`);
})();
