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

function mountConfig({ fetchImpl } = {}) {
  const ids = { goalTpl: buildGoalTemplate() };
  for (const id of ['previewStage', 'cardList', 'dockFields']) ids[id] = el('div', id);
  for (const id of ['dockGoal', 'dockPosition', 'dockScale']) {
    const input = el('select');
    input.value = '';
    ids[id] = input;
  }
  const enabled = el('input');
  enabled.checked = false;
  ids.dockEnabled = enabled;

  const document = makeDocument(ids);
  const window = { addEventListener() {} };

  const createGoalBars = new Function(
    'document',
    inlineScript('partials/_goal_bars.html') + '\n; return createGoalBars;'
  )(document);

  const sent = [];
  const fetchStub = fetchImpl || (async (url, init) => {
    sent.push({ url, init });
    return { ok: true, json: async () => ({ success: true }) };
  });

  const alerts = [];

  const api = new Function(
    'document', 'window', 'fetch', 'WebSocket', 'console', 'location', 'setTimeout',
    'alert', 'createGoalBars',
    inlineScript('banner_config.html') + `
    ; return {
        loadConfig, loadGoals, saveConfig, renderAll, renderCards, renderDockPanel,
        bindDockPanel, addCard, updateCardGoal, goalOptions, entriesFor, previewCard,
        escapeHtml, escapeAttr,
        getCards: () => bannerCards, getDock: () => dock,
        setCards: (v) => { bannerCards = v; },
        setDock: (v) => { dock = v; },
        setGoals: (v) => { goals = v; },
        setGoalEntries: (v) => { goalEntries = v; },
    };`
  )(
    document, window, fetchStub,
    function () { throw new Error('aucune WS pendant le test'); },
    { error() {}, log() {} },
    { protocol: 'http:', host: '127.0.0.1:3000', origin: 'http://127.0.0.1:3000' },
    () => 0,
    (m) => alerts.push(m),
    createGoalBars
  );

  return { api, ids, sent, alerts, document };
}

let passed = 0;
const check = async (label, fn) => {
  try { await fn(); passed++; console.log(`  ok    ${label}`); }
  catch (e) { console.log(`  ECHEC ${label}\n        ${e.message}`); process.exitCode = 1; }
};

const goal = (id, title) => ({ id, title, target: 200, accentColor: '#9146FF' });
const entry = (id, title) => ({ config: goal(id, title), current: 100, percent: 50, warning: null });

(async () => {

// ── Le choix du type à l'ajout ──────────────────────────────────────────────

await check('« + Carte objectif » crée bien une carte de type goal', () => {
  const { api } = mountConfig();
  api.setCards([]);
  api.addCard('goal');
  const card = api.getCards()[0];
  assert.strictEqual(card.kind, 'goal');
  // Absent = tous les objectifs, le choix le plus utile par défaut.
  assert.strictEqual(card.goalId, null);
});

await check('« + Carte texte / image » crée une carte texte', () => {
  const { api } = mountConfig();
  api.setCards([]);
  api.addCard('text');
  assert.strictEqual(api.getCards()[0].kind, 'text');
});

await check('un type inconnu retombe sur « texte » plutôt que de passer', () => {
  const { api } = mountConfig();
  api.setCards([]);
  api.addCard('nimporte quoi');
  assert.strictEqual(api.getCards()[0].kind, 'text');
});

await check('la liste se termine par les deux boutons d’ajout', () => {
  const { api, ids } = mountConfig();
  api.setCards([]);
  api.setGoals([]);
  api.renderCards();
  const addRow = ids.cardList.children[ids.cardList.children.length - 1];
  assert.ok(addRow.classList.contains('add-row'));
  assert.strictEqual(addRow.children.length, 2);
  assert.strictEqual(addRow.children[0].textContent, '+ Carte texte / image');
  assert.strictEqual(addRow.children[1].textContent, '+ Carte objectif');
});

// ── Éditeur d'une carte ─────────────────────────────────────────────────────

await check('une carte objectif propose un sélecteur, pas de champ texte', () => {
  const { api, ids } = mountConfig();
  api.setGoals([goal('g1', 'Objectif followers')]);
  api.setCards([{ id: 'c1', kind: 'goal', goalId: 'g1', transition: 'fade' }]);
  api.renderCards();
  const markup = ids.cardList.children[0].innerHTML;
  assert.ok(markup.includes('Objectif affiché'), 'sélecteur d’objectif absent');
  assert.ok(!markup.includes('Text Content'), 'champ texte présent à tort');
  assert.ok(!markup.includes('image-upload-container'), 'téléversement d’image proposé à tort');
});

await check('une carte texte garde son champ texte et son téléversement', () => {
  const { api, ids } = mountConfig();
  api.setGoals([]);
  api.setCards([{ id: 'c1', kind: 'text', text: 'Bonjour', transition: 'fade' }]);
  api.renderCards();
  const markup = ids.cardList.children[0].innerHTML;
  assert.ok(markup.includes('Text Content'));
  assert.ok(markup.includes('image-upload-container'));
  assert.ok(!markup.includes('goal-badge'));
});

await check('la pastille nomme la cible, ou signale sa disparition', () => {
  const { api, ids } = mountConfig();
  api.setGoals([goal('g1', 'Objectif followers')]);

  api.setCards([{ id: 'c1', kind: 'goal', goalId: 'g1' }]);
  api.renderCards();
  assert.ok(ids.cardList.children[0].innerHTML.includes('Objectif followers'));

  api.setCards([{ id: 'c1', kind: 'goal', goalId: null }]);
  api.renderCards();
  assert.ok(ids.cardList.children[0].innerHTML.includes('Tous les objectifs'));

  api.setCards([{ id: 'c1', kind: 'goal', goalId: 'disparu' }]);
  api.renderCards();
  assert.ok(ids.cardList.children[0].innerHTML.includes('Objectif supprimé'),
    'une cible supprimée doit se voir dans la liste');
});

await check('choisir « Tous les objectifs » remet goalId à null, pas à ""', () => {
  const { api } = mountConfig();
  api.setGoals([]);
  api.setCards([{ id: 'c1', kind: 'goal', goalId: 'g1' }]);
  api.updateCardGoal(0, '');
  assert.strictEqual(api.getCards()[0].goalId, null);
  api.updateCardGoal(0, 'g2');
  assert.strictEqual(api.getCards()[0].goalId, 'g2');
});

// ── Listes déroulantes ──────────────────────────────────────────────────────

await check('les options listent « Tous » puis chaque objectif, sélection marquée', () => {
  const { api } = mountConfig();
  api.setGoals([goal('g1', 'Followers'), goal('g2', 'Dons')]);
  const html = api.goalOptions('g2');
  assert.strictEqual((html.match(/<option/g) || []).length, 3);
  assert.ok(html.includes('>Tous les objectifs</option>'));
  assert.ok(/<option value="g2" selected>/.test(html), 'la sélection n’est pas marquée');
  assert.ok(!/<option value="g1" selected>/.test(html));
});

await check('un objectif sans id est ignoré dans la liste', () => {
  const { api } = mountConfig();
  api.setGoals([{ title: 'Jamais enregistré' }, goal('g1', 'Followers')]);
  const html = api.goalOptions(null);
  assert.strictEqual((html.match(/<option/g) || []).length, 2);
  assert.ok(!html.includes('Jamais enregistré'));
});

// ── Échappement ─────────────────────────────────────────────────────────────

await check('escapeAttr neutralise le guillemet, escapeHtml non', () => {
  // C'est toute la raison d'être des deux fonctions : `textContent` → `innerHTML`
  // n'échappe pas les guillemets, donc ne protège pas une valeur d'attribut.
  const { api } = mountConfig();
  assert.ok(api.escapeAttr('a"b').includes('&quot;'));
  assert.ok(!api.escapeHtml('a"b').includes('&quot;'));
  assert.strictEqual(api.escapeAttr('<&>'), '&lt;&amp;&gt;');
});

await check('un titre de carte avec guillemet ne disloque pas sa ligne', () => {
  const { api, ids } = mountConfig();
  api.setGoals([]);
  api.setCards([{ id: 'c1', kind: 'text', text: 'Dis "bonjour" au chat', transition: 'fade' }]);
  api.renderCards();
  const markup = ids.cardList.children[0].innerHTML;
  assert.ok(markup.includes('&quot;bonjour&quot;'), 'le guillemet sort de l’attribut value');
  assert.ok(!markup.includes('"bonjour"'));
});

await check('un titre d’objectif hostile reste du texte dans les options', () => {
  const { api } = mountConfig();
  api.setGoals([goal('g1', '<img onerror=alert(1)>')]);
  const html = api.goalOptions(null);
  assert.ok(html.includes('&lt;img onerror=alert(1)&gt;'));
  assert.ok(!html.includes('<img'));
});

// ── Barres fixes ────────────────────────────────────────────────────────────

await check('les réglages de barres fixes sont masqués tant qu’elles sont éteintes', () => {
  const { api, ids } = mountConfig();
  api.setGoals([]);
  api.setDock({ enabled: false, position: 'bottom', goalId: null, scale: 1 });
  api.renderDockPanel();
  assert.ok(ids.dockFields.classList.contains('hide'));
  assert.strictEqual(ids.dockEnabled.checked, false);

  api.setDock({ enabled: true, position: 'top', goalId: null, scale: 2 });
  api.renderDockPanel();
  assert.ok(!ids.dockFields.classList.contains('hide'));
  assert.strictEqual(ids.dockPosition.value, 'top');
  assert.strictEqual(ids.dockScale.value, 2);
});

await check('la case et les champs pilotent bien l’état', () => {
  const { api, ids } = mountConfig();
  api.setGoals([goal('g1', 'Followers')]);
  api.setDock({ enabled: false, position: 'bottom', goalId: null, scale: 1 });
  api.bindDockPanel();

  ids.dockEnabled.checked = true;
  ids.dockEnabled.dispatch('change');
  assert.strictEqual(api.getDock().enabled, true);

  ids.dockPosition.value = 'top';
  ids.dockPosition.dispatch('change');
  assert.strictEqual(api.getDock().position, 'top');

  ids.dockGoal.value = 'g1';
  ids.dockGoal.dispatch('change');
  assert.strictEqual(api.getDock().goalId, 'g1');

  ids.dockGoal.value = '';
  ids.dockGoal.dispatch('change');
  assert.strictEqual(api.getDock().goalId, null, '« Tous » doit valoir null');
});

await check('une échelle absurde retombe sur le défaut', () => {
  const { api, ids } = mountConfig();
  api.setGoals([]);
  api.setDock({ enabled: true, position: 'bottom', goalId: null, scale: 1 });
  api.bindDockPanel();

  ids.dockScale.value = '-3';
  ids.dockScale.dispatch('change');
  assert.strictEqual(api.getDock().scale, 1);

  ids.dockScale.value = '2.5';
  ids.dockScale.dispatch('change');
  assert.strictEqual(api.getDock().scale, 2.5);
});

// ── Format d'échange ────────────────────────────────────────────────────────

await check('loadConfig lit { cards, dock } et complète les défauts', async () => {
  const { api } = mountConfig({
    fetchImpl: async () => ({
      ok: true,
      json: async () => ({ cards: [{ id: 'c1', kind: 'goal' }], dock: { enabled: true } }),
    }),
  });
  api.setGoals([]);
  await api.loadConfig();
  assert.strictEqual(api.getCards().length, 1);
  assert.deepStrictEqual(api.getDock(),
    { enabled: true, position: 'bottom', goalId: null, scale: 1 });
});

await check('saveConfig envoie { cards, dock }, pas un tableau nu', async () => {
  const { api, sent } = mountConfig();
  api.setGoals([]);
  api.setCards([{ id: 'c1', kind: 'goal', goalId: 'g1' }]);
  api.setDock({ enabled: true, position: 'top', goalId: null, scale: 2 });
  await api.saveConfig();

  const post = sent.find((r) => r.init && r.init.method === 'POST');
  assert.ok(post, 'aucun POST émis');
  const body = JSON.parse(post.init.body);
  assert.ok(Array.isArray(body.cards), 'la clé cards manque');
  assert.strictEqual(body.cards[0].kind, 'goal');
  assert.strictEqual(body.dock.position, 'top');
});

// ── Aperçu ──────────────────────────────────────────────────────────────────

await check('l’aperçu d’une carte objectif rend de vraies barres', () => {
  const { api, ids } = mountConfig();
  api.setGoals([goal('g1', 'Followers')]);
  api.setGoalEntries([entry('g1', 'Followers')]);
  api.setCards([{ id: 'c1', kind: 'goal', goalId: 'g1', transition: 'zoom' }]);
  api.previewCard(0);

  const wrapper = ids.previewStage.children[0];
  assert.ok(wrapper.classList.contains('zoom-in'), 'la transition choisie n’est pas rejouée');
  const holder = wrapper.querySelector('.preview-goals');
  // Même habillage que l'overlay : l'aperçu ne doit pas montrer autre chose.
  assert.ok(holder.classList.contains('goal-banner'), 'aperçu sans habillage bannière');
  assert.strictEqual(holder.children.length, 1);
  assert.strictEqual(holder.querySelector('.goal-title').textContent, 'Followers');
  assert.strictEqual(holder.querySelector('.goal-current').textContent, '100');
});

await check('l’aperçu explique une cible disparue au lieu d’afficher du vide', () => {
  const { api, ids } = mountConfig();
  api.setGoals([]);
  api.setGoalEntries([]);
  api.setCards([{ id: 'c1', kind: 'goal', goalId: 'disparu', transition: 'fade' }]);
  api.previewCard(0);
  const text = ids.previewStage.children[0].querySelector('.preview-hint').textContent;
  assert.ok(text.includes("n'existe plus"), text);
});

await check('l’aperçu d’une carte texte reste du texte', () => {
  const { api, ids } = mountConfig();
  api.setGoals([]);
  api.setCards([{ id: 'c1', kind: 'text', text: 'Bienvenue', transition: 'fade' }]);
  api.previewCard(0);
  assert.ok(ids.previewStage.innerHTML.includes('Bienvenue'));
  assert.ok(ids.previewStage.innerHTML.includes('banner-text'));
});

console.log(`\n  ${passed} assertions de banner-config OK`);
})();
