'use strict';

/**
 * Bouton « Copier le lien » du bandeau de configuration.
 *
 * Le script est chargé depuis `partials/_macros.html`, donc six pages de configuration
 * partagent exactement ce code : une régression ici les touche toutes d'un coup, sans
 * qu'aucune suite de page ne la voie — leurs tests lisent le `<script>` du template,
 * où le macro n'est pas encore développé.
 */

const fs = require('fs');
const path = require('path');
const assert = require('assert');
const { el, makeDocument } = require('./dom-stub.cjs');

const MACROS = path.resolve(__dirname, '../../templates/partials/_macros.html');

function macroScript() {
  const source = fs.readFileSync(MACROS, 'utf8');
  return /<script[^>]*>([\s\S]*?)<\/script>/.exec(source)[1];
}

const flush = () => new Promise((resolve) => setImmediate(resolve));

/**
 * @param {object} options
 * @param {string|null} options.url  `null` : aucun bouton dans la page.
 * @param {object|null} options.clipboard  `null` : pas d'API presse-papiers.
 */
function mount({ url = '/goal', clipboard = {} } = {}) {
  const button = url === null ? null : el('button', 'btn-copy-link');
  if (button) {
    // Le gabarit met le libellé sur sa propre ligne : le script doit le retailler
    // avant de le mémoriser, sinon la restauration réinjecte les espaces.
    button.textContent = '\n            Copier le lien\n        ';
    button.dataset = { displayUrl: url };
  }

  const written = [];
  const prompted = [];
  const timers = [];

  // `makeDocument` ne cherche que par identifiant ; le bouton se trouve par sa classe,
  // et rien d'autre dans la page n'en porte une.
  const document = {
    ...makeDocument({}),
    querySelector: (selector) => (selector === '.btn-copy-link' ? button : null),
  };

  const navigator = clipboard === null ? {} : {
    clipboard: {
      writeText: clipboard.writeText || (async (text) => { written.push(text); }),
    },
  };

  new Function(
    'document', 'location', 'navigator', 'window', 'console', 'setTimeout', 'clearTimeout', 'URL',
    macroScript()
  )(
    document,
    { origin: 'http://127.0.0.1:3000' },
    navigator,
    { prompt: (label, value) => { prompted.push(value); } },
    { error() {} },
    (fn) => { timers.push(fn); return timers.length; },
    () => {},
    URL
  );

  return { button, written, prompted, timers };
}

let passed = 0;
const check = async (label, fn) => {
  try { await fn(); passed++; console.log(`  ok    ${label}`); }
  catch (e) { console.log(`  ECHEC ${label}\n        ${e.message}`); process.exitCode = 1; }
};

(async () => {

await check('le lien copié est absolu, résolu sur l’origine de la page', async () => {
  // Une URL relative collée dans OBS ne mène nulle part : la source navigateur n'a
  // pas d'origine à laquelle la rattacher.
  const { button, written } = mount({ url: '/goal' });
  button.dispatch('click');
  await flush();

  assert.deepStrictEqual(written, ['http://127.0.0.1:3000/goal']);
});

await check('une URL à paramètres survit à la résolution', async () => {
  const { button, written } = mount({ url: '/text?name=start' });
  button.dispatch('click');
  await flush();

  assert.deepStrictEqual(written, ['http://127.0.0.1:3000/text?name=start']);
});

await check('le libellé confirme puis revient à sa valeur d’origine', async () => {
  const { button, timers } = mount();
  button.dispatch('click');
  await flush();

  assert.strictEqual(button.textContent, 'Lien copié');
  assert.strictEqual(timers.length, 1, 'aucune restauration programmée');

  timers[0]();
  assert.strictEqual(button.textContent, 'Copier le lien');
});

await check('sans API presse-papiers, l’URL reste récupérable à la main', async () => {
  // `navigator.clipboard` n'existe pas hors contexte sécurisé.
  const { button, prompted } = mount({ clipboard: null });
  button.dispatch('click');
  await flush();

  assert.deepStrictEqual(prompted, ['http://127.0.0.1:3000/goal']);
});

await check('un refus du presse-papiers retombe sur la même issue', async () => {
  const { button, prompted, written } = mount({
    clipboard: { writeText: async () => { throw new Error('permission refusée'); } },
  });
  button.dispatch('click');
  await flush();

  assert.deepStrictEqual(written, []);
  assert.deepStrictEqual(prompted, ['http://127.0.0.1:3000/goal']);
});

await check('une page sans bouton ne fait pas lever le script', () => {
  // `/settings` et `/text-config` n'ont pas d'URL d'affichage : le macro n'émet alors
  // aucun bouton, mais le script partagé s'évalue quand même sur les autres pages.
  assert.doesNotThrow(() => mount({ url: null }));
});

console.log(`  ${passed} verification(s) OK`);

})();
