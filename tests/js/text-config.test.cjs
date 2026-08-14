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

function mountConfig({ config } = {}) {
  const ids = {
    previewStage: el('div', 'preview-stage'),
    sectionList: el('div', 'card-list'),
  };

  const document = makeDocument(ids);
  const loadHandlers = [];
  const window = {
    addEventListener(type, fn) { if (type === 'load') loadHandlers.push(fn); },
  };

  const sent = [];
  const fetchStub = async (url, init) => {
    sent.push({ url, init });
    if (init && init.method === 'POST') {
      return { ok: true, json: async () => ({ success: true }) };
    }
    return { ok: true, json: async () => (config || { sections: [] }) };
  };

  const createTextRenderer = new Function(
    'document', 'setTimeout', 'clearTimeout',
    inlineScript('partials/_text_render.html') + '\n; return createTextRenderer;'
  )(document, () => 0, () => {});

  const copied = [];

  const api = new Function(
    'document', 'window', 'fetch', 'console', 'location', 'alert', 'confirm',
    'navigator', 'createTextRenderer',
    inlineScript('text_config.html') + `
    ; return {
        loadConfig, saveConfig, addSection, deleteSection, updateSection, setEffect,
        renderSections, previewSection, copyUrl, moveUp, moveDown,
        slugify, displayUrl, escapeHtml, escapeAttr,
        getSections: () => sections, setSections: (v) => { sections = v; },
        previewIndex: () => previewIndex,
    };`
  )(
    document, window, fetchStub,
    { error() {}, log() {} },
    { protocol: 'http:', host: '127.0.0.1:3000', origin: 'http://127.0.0.1:3000' },
    () => {},
    () => true,
    { clipboard: { writeText: async (t) => { copied.push(t); } } },
    createTextRenderer
  );

  return { api, ids, sent, loadHandlers, copied };
}

let passed = 0;
const check = async (label, fn) => {
  try { await fn(); passed++; console.log(`  ok    ${label}`); }
  catch (e) { console.log(`  ECHEC ${label}\n        ${e.message}`); process.exitCode = 1; }
};

(async () => {

await check('le script s\'évalue et enregistre son écouteur "load"', () => {
  const { loadHandlers } = mountConfig();
  assert.strictEqual(loadHandlers.length, 1);
});

// --- Parité du slug avec src/models/text.rs ----------------------------------
// Le serveur fait autorité, mais cette copie affiche l'URL pendant la frappe :
// une divergence donnerait une adresse qui ne correspond à rien une fois
// sauvegardée. Les cas sont ceux des tests Rust — les modifier ici sans les
// modifier là-bas doit faire échouer cette suite.

await check('slugify reproduit les cas des tests Rust', () => {
  const { api } = mountConfig();
  assert.strictEqual(api.slugify('Écran de début'), 'ecran-de-debut');
  assert.strictEqual(api.slugify('  Je reviens !!  '), 'je-reviens');
  assert.strictEqual(api.slugify('a/b?c=d'), 'a-b-c-d');
  assert.strictEqual(api.slugify('scene_2'), 'scene_2');
  assert.strictEqual(api.slugify('START'), 'start');
  assert.strictEqual(api.slugify('!!!'), '');
  assert.strictEqual(api.slugify(''), '');
});

await check('un nom vide retombe sur section-<n> dans l\'URL, comme le serveur', () => {
  const { api } = mountConfig();
  assert.strictEqual(api.displayUrl({ name: '' }, 0), 'http://127.0.0.1:3000/text?name=section-1');
  assert.strictEqual(api.displayUrl({ name: '!!!' }, 2), 'http://127.0.0.1:3000/text?name=section-3');
});

await check('l\'URL d\'affichage porte le slug, pas la saisie brute', () => {
  const { api } = mountConfig();
  assert.strictEqual(
    api.displayUrl({ name: 'Écran de début' }, 0),
    'http://127.0.0.1:3000/text?name=ecran-de-debut'
  );
});

await check('copier place l\'URL affichée dans le presse-papier', async () => {
  const { api, copied } = mountConfig();
  api.setSections([{ name: 'Mon Écran' }]);
  await api.copyUrl(0);
  assert.deepStrictEqual(copied, ['http://127.0.0.1:3000/text?name=mon-ecran']);
});

// --- Échappement -------------------------------------------------------------

await check('escapeAttr retire les guillemets, escapeHtml non', () => {
  const { api } = mountConfig();
  assert.strictEqual(api.escapeHtml('a<b>&c'), 'a&lt;b&gt;&amp;c');
  assert.strictEqual(api.escapeHtml('a"b'), 'a"b');
  assert.strictEqual(api.escapeAttr('a"b'), 'a&quot;b');
  assert.strictEqual(api.escapeAttr("a'b"), 'a&#39;b');
});

await check('un libellé contenant un guillemet ne casse pas la ligne', () => {
  const { api, ids } = mountConfig();
  api.setSections([{ name: 'start', label: 'Le "gros" titre', content: 'x' }]);
  api.renderSections();
  const html = ids.sectionList.innerHTML;
  assert.ok(html.includes('value="Le &quot;gros&quot; titre"'), 'le libellé doit être échappé en attribut');
});

await check('un contenu balisé n\'est pas injecté dans la page de config', () => {
  const { api, ids } = mountConfig();
  api.setSections([{ name: 'start', content: '<img src=x onerror=alert(1)>' }]);
  api.renderSections();
  const html = ids.sectionList.innerHTML;
  assert.ok(!html.includes('<img src=x'), 'aucune balise ne doit survivre');
  assert.ok(html.includes('&lt;img src=x onerror=alert(1)&gt;'));
});

// --- Période de l'effet ------------------------------------------------------

await check('changer d\'effet emmène sa période par défaut', () => {
  const { api } = mountConfig();
  // 2000 est le défaut de « aucun effet » : l'utilisateur n'y a pas touché.
  api.setSections([{ name: 'a', effect: 'none', effectMs: 2000 }]);
  api.setEffect(0, 'marquee');
  assert.strictEqual(api.getSections()[0].effect, 'marquee');
  assert.strictEqual(api.getSections()[0].effectMs, 12000, 'un défilement à 2 s serait illisible');
});

/**
 * Chaque effet proposé doit avoir sa période par défaut.
 *
 * Une entrée manquante dans `DEFAULT_EFFECT_MS` ne casse rien de visible : la
 * durée devient `undefined`, le rendu retombe sur 2000 ms — et un défilement de
 * 2 s défile trop vite pour être lu.
 */
await check('chaque effet emmène une période par défaut exploitable', () => {
  const config = fs.readFileSync(`${TPL_DIR}/text_config.html`, 'utf8');
  const effects = Array.from(
    /const EFFECTS = \[([^\]]*\])[^;]*/.exec(config)[0].matchAll(/\['([a-z]+)'/g),
    (m) => m[1]
  );
  assert.ok(effects.length >= 15, 'liste des effets non capturée');

  const { api } = mountConfig();
  for (const effect of effects) {
    // 2000 est le défaut de « aucun effet » : la période n'a pas été réglée à la
    // main, `setEffect` doit donc poser celle du nouvel effet.
    api.setSections([{ name: 'a', effect: 'none', effectMs: 2000 }]);
    api.setEffect(0, effect);
    const ms = api.getSections()[0].effectMs;
    assert.ok(
      Number.isFinite(ms) && ms > 0,
      `l'effet ${effect} n'a pas de période par défaut (DEFAULT_EFFECT_MS)`
    );
  }
});

await check('une période réglée à la main survit au changement d\'effet', () => {
  const { api } = mountConfig();
  api.setSections([{ name: 'a', effect: 'none', effectMs: 4321 }]);
  api.setEffect(0, 'marquee');
  assert.strictEqual(api.getSections()[0].effectMs, 4321);
});

// --- Liste de sections -------------------------------------------------------

await check('ajouter une section lui donne un nom et un identifiant', () => {
  const { api } = mountConfig();
  api.addSection();
  const [section] = api.getSections();
  assert.strictEqual(section.name, 'section-1');
  assert.ok(section.id, 'un id stable est posé côté client');
  assert.strictEqual(section.animation, 'none');
  assert.strictEqual(section.effect, 'none');
});

await check('monter et descendre échangent bien deux sections', () => {
  const { api } = mountConfig();
  api.setSections([{ name: 'a' }, { name: 'b' }, { name: 'c' }]);
  api.moveDown(0);
  assert.deepStrictEqual(api.getSections().map((s) => s.name), ['b', 'a', 'c']);
  api.moveUp(2);
  assert.deepStrictEqual(api.getSections().map((s) => s.name), ['b', 'c', 'a']);
});

await check('supprimer avant la section prévisualisée décale l\'aperçu', () => {
  const { api } = mountConfig();
  api.setSections([{ name: 'a' }, { name: 'b' }, { name: 'c' }]);
  api.previewSection(2);
  assert.strictEqual(api.previewIndex(), 2);
  api.deleteSection(0);
  assert.strictEqual(api.previewIndex(), 1, 'l\'aperçu doit suivre la section, pas sa position');
  assert.strictEqual(api.getSections()[api.previewIndex()].name, 'c');
});

await check('supprimer la section prévisualisée éteint l\'aperçu', () => {
  const { api } = mountConfig();
  api.setSections([{ name: 'a' }, { name: 'b' }]);
  api.previewSection(0);
  api.deleteSection(0);
  assert.strictEqual(api.previewIndex(), -1);
});

// --- Chargement et sauvegarde ------------------------------------------------

await check('le chargement lit /api/text-config et remplit la liste', async () => {
  const { api, sent } = mountConfig({ config: { sections: [{ name: 'start', content: 'x' }] } });
  await api.loadConfig();
  assert.strictEqual(sent[0].url, '/api/text-config');
  assert.deepStrictEqual(api.getSections().map((s) => s.name), ['start']);
});

await check('la sauvegarde poste { sections } puis recharge', async () => {
  const { api, sent } = mountConfig({ config: { sections: [] } });
  api.setSections([{ name: 'start', content: 'Bonjour' }]);
  await api.saveConfig();

  const post = sent.find((r) => r.init && r.init.method === 'POST');
  assert.ok(post, 'aucun POST émis');
  assert.strictEqual(post.url, '/api/text-config');
  assert.deepStrictEqual(JSON.parse(post.init.body), { sections: [{ name: 'start', content: 'Bonjour' }] });

  // Le serveur normalise les noms : c'est SA version qui doit être réaffichée.
  assert.ok(sent.some((r) => !r.init || r.init.method !== 'POST'), 'aucun rechargement après le POST');
});

await check('une liste vide affiche une invite plutôt qu\'un cadre vide', () => {
  const { api, ids } = mountConfig();
  api.setSections([]);
  api.renderSections();
  assert.ok(ids.sectionList.innerHTML.includes('Aucune section'));
});

// --- Cohérence avec l'overlay ------------------------------------------------

await check('la page inclut le partiel de rendu, donc l\'aperçu est le rendu réel', () => {
  const html = fs.readFileSync(`${TPL_DIR}/text_config.html`, 'utf8');
  assert.ok(html.includes('{% include "partials/_text_render.html" %}'));
  const overlay = fs.readFileSync(`${TPL_DIR}/text.html`, 'utf8');
  assert.ok(overlay.includes('{% include "partials/_text_render.html" %}'));
});

await check('l\'overlay reste sur fond transparent', () => {
  const overlay = fs.readFileSync(`${TPL_DIR}/text.html`, 'utf8');
  // Une source navigateur OBS se pose par-dessus un gameplay : un fond opaque
  // (celui de /banner) masquerait la scène.
  assert.ok(/background:\s*transparent/.test(overlay));
});

await check('les listes déroulantes couvrent les variantes du modèle Rust', () => {
  const rust = fs.readFileSync(path.resolve(__dirname, '../../src/models/text.rs'), 'utf8');
  const config = fs.readFileSync(`${TPL_DIR}/text_config.html`, 'utf8');
  const partial = fs.readFileSync(`${TPL_DIR}/partials/_text_render.html`, 'utf8');

  // Une variante ajoutée côté Rust mais absente d'un des deux JS serait
  // silencieusement ramenée au défaut par `oneOf` — sans erreur visible.
  const entrances = /pub enum TextAnimation \{([\s\S]*?)\n\}/.exec(rust)[1];
  const effects = /pub enum TextEffect \{([\s\S]*?)\n\}/.exec(rust)[1];
  const variants = (block) => block
    .split('\n')
    .map((l) => l.trim().replace(/,$/, ''))
    .filter((l) => /^[A-Z][A-Za-z]*$/.test(l))
    .map((l) => l.toLowerCase());

  for (const v of variants(entrances)) {
    assert.ok(config.includes(`'${v}'`), `entrée ${v} absente de text_config.html`);
    assert.ok(partial.includes(`'${v}'`), `entrée ${v} absente de _text_render.html`);
  }
  for (const v of variants(effects)) {
    assert.ok(config.includes(`'${v}'`), `effet ${v} absent de text_config.html`);
    assert.ok(partial.includes(`'${v}'`), `effet ${v} absent de _text_render.html`);
  }
});

console.log(`  ${passed} verification(s) OK`);

})();
