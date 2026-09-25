'use strict';

/**
 * Bouton « Tester » du configurateur d'alertes.
 *
 * Le choix de la ligne et la mise en forme de l'événement vivent en Rust, où ils sont
 * testés (`twitch::tests`, `models::channel_point::tests`). Ce qui reste ici est ce que
 * l'utilisateur lit : la ligne **affichée** part telle quelle — c'est ce qui permet
 * d'essayer une phrase avant de l'enregistrer — et un test parti sans destinataire doit
 * le dire, sous peine de laisser chercher pourquoi « rien ne se passe ».
 */

const fs = require('fs');
const path = require('path');
const assert = require('assert');
const { el, makeDocument } = require('./dom-stub.cjs');

const TPL_DIR = path.resolve(__dirname, '../../templates');

function inlineScript(file) {
  const html = fs.readFileSync(`${TPL_DIR}/${file}`, 'utf8');
  return /<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/.exec(html)[1];
}

const OK = { ok: true, status: 200, json: async () => ({ success: true, overlays: 1 }) };

function mountConfig({ respond } = {}) {
  const ids = {
    cardList: el('div', 'card-list'),
    // Les deux nœuds que la carte pose autour du bouton.
    'test-0': el('button', 'btn-test'),
    'testStatus-0': el('span', 'test-status'),
  };

  const document = makeDocument(ids);
  const loadHandlers = [];
  const window = {
    addEventListener(type, fn) { if (type === 'load') loadHandlers.push(fn); },
  };

  const sent = [];
  const fetchStub = async (url, init) => {
    sent.push({ url, init });
    return respond ? respond() : OK;
  };

  const api = new Function(
    'document', 'window', 'fetch', 'console', 'alert',
    inlineScript('channel_point_config.html') + `
    ; return {
        testCard, renderCards,
        setRewards: (v) => { rewards = v; },
    };`
  )(document, window, fetchStub, { error() {}, log() {} }, () => {});

  return { api, ids, sent, loadHandlers };
}

/** Une ligne de bits à palier, telle que la page la tient en mémoire. */
function cheerRow() {
  return {
    kind: 'cheer',
    reward_title: '',
    minAmount: 5000,
    phrase: 'ENORME ! {{amount}} bits de {{user}} !',
    imagePath: '',
    soundPath: '/public/channelpoint/fanfare.mp3',
    transition: 'flip',
  };
}

let passed = 0;
const check = async (label, fn) => {
  try { await fn(); passed++; console.log(`  ok    ${label}`); }
  catch (e) { console.log(`  ECHEC ${label}\n        ${e.message}`); process.exitCode = 1; }
};

(async () => {

await check('le script s’évalue et enregistre son écouteur "load"', () => {
  const { loadHandlers } = mountConfig();
  assert.strictEqual(loadHandlers.length, 1, 'aucun écouteur "load" enregistré');
});

await check('la ligne partie au serveur est celle affichée, enregistrée ou non', async () => {
  const { api, sent } = mountConfig();
  const row = cheerRow();
  // Modification non enregistrée : tout l'intérêt du bouton est de la jouer quand même.
  row.phrase = 'phrase en cours de frappe';
  api.setRewards([row]);

  await api.testCard(0);

  assert.strictEqual(sent.length, 1);
  assert.strictEqual(sent[0].url, '/api/channel-points/test');
  assert.strictEqual(sent[0].init.method, 'POST');
  assert.deepStrictEqual(JSON.parse(sent[0].init.body), row);
});

await check('un index inconnu n’envoie rien', async () => {
  const { api, sent } = mountConfig();
  api.setRewards([]);
  await api.testCard(0);
  assert.strictEqual(sent.length, 0);
});

await check('le nombre de sources touchées est rapporté, au singulier comme au pluriel', async () => {
  const two = { ok: true, status: 200, json: async () => ({ success: true, overlays: 2 }) };
  const { api, ids } = mountConfig({ respond: () => two });
  api.setRewards([cheerRow()]);

  await api.testCard(0);
  assert.match(ids['testStatus-0'].textContent, /2 sources/);
  assert.ok(!ids['testStatus-0'].classList.contains('is-warning'));

  const one = mountConfig();
  one.api.setRewards([cheerRow()]);
  await one.api.testCard(0);
  assert.match(one.ids['testStatus-0'].textContent, /1 source\b/);
});

await check('aucun overlay ouvert est signalé, pas passé sous silence', async () => {
  // Le cas qui fait douter du bouton : le serveur a bien reçu le test, mais aucune
  // source /channel-points n'est là pour l'afficher.
  const empty = { ok: true, status: 200, json: async () => ({ success: true, overlays: 0 }) };
  const { api, ids } = mountConfig({ respond: () => empty });
  api.setRewards([cheerRow()]);

  await api.testCard(0);

  assert.match(ids['testStatus-0'].textContent, /channel-points/);
  assert.ok(ids['testStatus-0'].classList.contains('is-warning'), 'devrait alerter');
});

await check('une réponse en erreur affiche son code', async () => {
  const boom = { ok: false, status: 500, json: async () => ({}) };
  const { api, ids } = mountConfig({ respond: () => boom });
  api.setRewards([cheerRow()]);

  await api.testCard(0);

  assert.match(ids['testStatus-0'].textContent, /500/);
  assert.ok(ids['testStatus-0'].classList.contains('is-warning'));
});

await check('le bouton redevient cliquable même quand la requête échoue', async () => {
  // Sans le `finally`, un serveur arrêté laissait le bouton grisé jusqu'au prochain
  // rendu de la carte — donc jusqu'à une modification quelconque.
  const { api, ids } = mountConfig({ respond: () => { throw new Error('réseau coupé'); } });
  api.setRewards([cheerRow()]);

  await api.testCard(0);

  assert.strictEqual(ids['test-0'].disabled, false);
  assert.ok(ids['testStatus-0'].classList.contains('is-warning'));
});

await check('un succès efface l’avertissement du test précédent', async () => {
  let overlays = 0;
  const { api, ids } = mountConfig({
    respond: () => ({ ok: true, status: 200, json: async () => ({ overlays }) }),
  });
  api.setRewards([cheerRow()]);

  await api.testCard(0);
  assert.ok(ids['testStatus-0'].classList.contains('is-warning'));

  overlays = 1;
  await api.testCard(0);
  assert.ok(!ids['testStatus-0'].classList.contains('is-warning'), 'avertissement resté posé');
});

// ── Animations de la phrase ─────────────────────────────────────────────────

/** Paires `['clé', 'Libellé']` d'une constante de liste, lues dans le fichier. */
function pairsOf(file, name) {
  const html = fs.readFileSync(`${TPL_DIR}/${file}`, 'utf8');
  const block = new RegExp(`const ${name} = \\[([\\s\\S]*?)\\];`).exec(html)[1];
  return Array.from(block.matchAll(/\['([a-z]+)',\s*'([^']+)'\]/g), (m) => [m[1], m[2]]);
}

await check('les animations proposées sont celles de /text-config, mêmes libellés', () => {
  const textEntrances = pairsOf('text_config.html', 'ENTRANCES');
  const textEffects = pairsOf('text_config.html', 'EFFECTS');
  const entrances = pairsOf('channel_point_config.html', 'TEXT_ENTRANCES');
  const effects = pairsOf('channel_point_config.html', 'TEXT_EFFECTS');

  // Garde-fou : une regex qui cesserait de capturer rendrait le test muet.
  assert.ok(textEntrances.length >= 17 && textEffects.length >= 16, 'listes de /text-config non capturées');

  // Toutes les entrées, et tous les effets sauf le défilement.
  assert.deepStrictEqual(
    [...entrances].sort(),
    [...textEntrances].sort()
  );
  assert.deepStrictEqual(
    [...effects].sort(),
    textEffects.filter(([v]) => v !== 'marquee').sort()
  );
});

await check('une ligne présélectionne ses animations enregistrées', () => {
  const { api, ids } = mountConfig();
  api.setRewards([{ ...cheerRow(), textAnimation: 'stamp', textEffect: 'neon' }]);
  api.renderCards();
  const html = ids.cardList.children[0].innerHTML;
  assert.ok(html.includes('<option value="stamp" selected>Tampon</option>'), 'entrée non présélectionnée');
  assert.ok(html.includes('<option value="neon" selected>Néon</option>'), 'effet non présélectionné');
});

await check('une ligne sans réglage montre le rendu d\'avant : aucune entrée, dégradé', () => {
  const { api, ids } = mountConfig();
  api.setRewards([cheerRow()]);
  api.renderCards();
  const html = ids.cardList.children[0].innerHTML;
  assert.ok(html.includes('<option value="none" selected>Aucune</option>'));
  assert.ok(html.includes('<option value="gradient" selected>Dégradé animé</option>'));
});

await check('le test joue les animations choisies, même non enregistrées', async () => {
  const { api, sent } = mountConfig();
  const row = { ...cheerRow(), textAnimation: 'typewriter', textEffect: 'wave' };
  api.setRewards([row]);
  await api.testCard(0);
  const body = JSON.parse(sent[0].init.body);
  assert.strictEqual(body.textAnimation, 'typewriter');
  assert.strictEqual(body.textEffect, 'wave');
});

console.log(`  ${passed} verification(s) OK`);

})();
