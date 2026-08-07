'use strict';
const fs = require('fs');
const path = require('path');
const assert = require('assert');
const { el, makeDocument } = require('./dom-stub.cjs');

const TPL_DIR = path.resolve(__dirname, '../../templates');

function inlineScript(file) {
  const html = fs.readFileSync(`${TPL_DIR}/${file}`, 'utf8');
  return /<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/.exec(html)[1];
}

function mountConfig({ fetchImpl } = {}) {
  const ids = {};
  for (const id of ['goalList', 'previewStack', 'overlayUrl', 'status', 'addBtn',
                    'goalCardTpl', 'previewTpl']) {
    ids[id] = el('div', id);
  }

  const document = makeDocument(ids);
  const loadHandlers = [];
  const window = {
    addEventListener(type, fn) { if (type === 'load') loadHandlers.push(fn); },
  };

  const sent = [];
  const fetchStub = fetchImpl || (async (url, init) => {
    sent.push({ url, init });
    return { ok: true, json: async () => ({ success: true }) };
  });

  const api = new Function(
    'document', 'window', 'fetch', 'crypto', 'WebSocket', 'console', 'location', 'setTimeout',
    inlineScript('goal_config.html') + `
    ; return {
        loadConfig, saveConfig, newGoal, previewCurrent, previewPercent,
        getGoals: () => goals, setGoals: (v) => { goals = v; },
    };`
  )(
    document, window, fetchStub,
    { randomUUID: () => 'uuid-' + Math.random().toString(16).slice(2) },
    function () { throw new Error('aucune WS pendant le test'); },
    { error() {}, log() {} },
    { protocol: 'http:', host: '127.0.0.1:3000', origin: 'http://127.0.0.1:3000' },
    () => 0
  );

  return { api, ids, sent, loadHandlers };
}

let passed = 0;
const check = async (label, fn) => {
  try { await fn(); passed++; console.log(`  ok    ${label}`); }
  catch (e) { console.log(`  ECHEC ${label}\n        ${e.message}`); process.exitCode = 1; }
};

(async () => {

// ── Le bug bloquant ─────────────────────────────────────────────────────────

await check('le script s’évalue sans lever, et enregistre son écouteur "load"', () => {
  // C'est exactement ce qui échouait : `$("saveBtn")` renvoyait null, le
  // TypeError coupait l'évaluation avant `window.addEventListener("load", …)`,
  // et la page ne chargeait donc jamais sa configuration. Pire, `goals` restait
  // vide, donc « Sauvegarder » écrasait goal.json avec une liste vide.
  const { loadHandlers } = mountConfig();
  assert.strictEqual(loadHandlers.length, 1, 'aucun écouteur "load" enregistré');
});

await check('aucune page de config ne référence un id de bouton inexistant', () => {
  // La régression d'origine : le refactor du macro `ui::header` a retiré le
  // bouton mais laissé son écouteur. Le contrôle porte sur toutes les pages.
  const pages = fs.readdirSync(TPL_DIR).filter((f) => f.endsWith('.html'));
  const offenders = [];

  for (const page of pages) {
    const html = fs.readFileSync(`${TPL_DIR}/${page}`, 'utf8');
    // Les commentaires sont retirés : celui qui documente le bug cite le code fautif.
    const code = html.replace(/\/\/[^\n]*/g, '').replace(/<!--[\s\S]*?-->/g, '');
    const declared = new Set(
      [...html.matchAll(/id\s*=\s*["']([^"']+)["']/g)].map((m) => m[1])
    );
    for (const m of code.matchAll(/(?:getElementById|\$)\(\s*["']([A-Za-z][\w-]*)["']\s*\)/g)) {
      if (!declared.has(m[1])) offenders.push(`${page} → #${m[1]}`);
    }
  }

  assert.deepStrictEqual(offenders, [], 'ids référencés sans être déclarés');
});

// ── Format d'échange ────────────────────────────────────────────────────────

await check('loadConfig lit le tableau d’objectifs', async () => {
  const payload = [
    { id: 'g1', title: 'Followers', target: 200, source: 'followers' },
    { id: 'g2', title: 'Dons', target: 250, source: 'manual' },
  ];
  const { api } = mountConfig({
    fetchImpl: async () => ({ ok: true, json: async () => payload }),
  });
  await api.loadConfig();
  assert.strictEqual(api.getGoals().length, 2);
  assert.strictEqual(api.getGoals()[1].title, 'Dons');
});

await check('un objectif sans id en reçoit un (réordonnancement et aperçu)', async () => {
  const { api } = mountConfig({
    fetchImpl: async () => ({ ok: true, json: async () => [{ title: 'Ancien' }] }),
  });
  await api.loadConfig();
  assert.ok(api.getGoals()[0].id, 'aucun id posé sur la config héritée');
});

await check('saveConfig envoie le tableau tel quel', async () => {
  const { api, sent } = mountConfig();
  api.setGoals([{ id: 'g1', title: 'Followers', target: 200 }]);
  await api.saveConfig();
  assert.strictEqual(sent.length, 1);
  const body = JSON.parse(sent[0].init.body);
  assert.ok(Array.isArray(body), 'la charge utile n’est pas un tableau');
  assert.strictEqual(body[0].id, 'g1');
  assert.strictEqual(sent[0].init.method, 'POST');
});

await check('un nouvel objectif ne porte aucun réglage de bannière', () => {
  // La bannière se règle désormais dans /banner-config : goal-config définit
  // les objectifs, rien de plus.
  const { api } = mountConfig();
  const created = api.newGoal();
  assert.ok(!('showInBanner' in created));
  assert.ok(!('banner' in created));
  assert.strictEqual(created.source, 'followers');
  assert.strictEqual(created.visible, true);
});

// ── Aperçu local (miroir du calcul Rust) ────────────────────────────────────

await check('l’aperçu retranche la ligne de base sans passer sous zéro', () => {
  const { api } = mountConfig();
  const g = { source: 'manual', manualCurrent: 30, baseline: 100, target: 50 };
  assert.strictEqual(api.previewCurrent(g), 0);
  g.manualCurrent = 130;
  assert.strictEqual(api.previewCurrent(g), 30);
});

await check('l’aperçu borne le pourcentage et ne divise pas par zéro', () => {
  const { api } = mountConfig();
  assert.strictEqual(api.previewPercent({ target: 0 }, 0), 100);
  assert.strictEqual(api.previewPercent({ target: 200 }, 500), 100);
  assert.strictEqual(api.previewPercent({ target: 200 }, 100), 50);
});

console.log(`\n  ${passed} assertions de goal-config OK`);
})();
