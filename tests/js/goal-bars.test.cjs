'use strict';
const fs = require('fs');
const path = require('path');
const assert = require('assert');
const { el, buildGoalTemplate, makeDocument } = require('./dom-stub.cjs');

const TPL_DIR = path.resolve(__dirname, '../../templates');

function inlineScript(file, index = 0) {
  const html = fs.readFileSync(`${TPL_DIR}/${file}`, 'utf8');
  const blocks = [...html.matchAll(/<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/g)];
  return blocks[index][1];
}

// ── Chargement du composant partagé ─────────────────────────────────────────
const goalTpl = buildGoalTemplate();
const document = makeDocument({ goalTpl });
const createGoalBars = new Function(
  'document',
  inlineScript('partials/_goal_bars.html') + '\n; return createGoalBars;'
)(document);

let passed = 0;
const check = (label, fn) => {
  try { fn(); passed++; console.log(`  ok    ${label}`); }
  catch (e) { console.log(`  ECHEC ${label}\n        ${e.message}`); process.exitCode = 1; }
};

const entry = (id, over = {}) => ({
  config: {
    id, title: `Objectif ${id}`, target: 200, accentColor: '#9146FF',
    showNumbers: true, showPercent: true, visible: true, showInBanner: true,
    ...(over.config || {}),
  },
  current: over.current !== undefined ? over.current : 100,
  percent: over.percent !== undefined ? over.percent : 50,
  warning: over.warning ?? null,
});

const text = (bar, cls) => bar.querySelector('.' + cls).textContent;

// ── Rendu d'une barre ───────────────────────────────────────────────────────
check('rend une barre par entrée, dans l’ordre reçu', () => {
  const root = el('div');
  const bars = createGoalBars(root);
  bars.render([entry('a'), entry('b')]);
  assert.strictEqual(root.children.length, 2);
  assert.strictEqual(text(root.children[0], 'goal-title'), 'Objectif a');
  assert.strictEqual(text(root.children[1], 'goal-title'), 'Objectif b');
});

check('réutilise le même nœud entre deux rendus (transition CSS préservée)', () => {
  const root = el('div');
  const bars = createGoalBars(root);
  bars.render([entry('a')]);
  const first = root.children[0];
  bars.render([entry('a', { current: 150, percent: 75 })]);
  assert.strictEqual(root.children[0].uid, first.uid, 'la barre a été recréée');
  assert.strictEqual(first.querySelector('.goal-fill').style.width, '75%');
  assert.strictEqual(text(first, 'goal-current'), '150');
});

check('retire les barres disparues de la configuration', () => {
  const root = el('div');
  const bars = createGoalBars(root);
  bars.render([entry('a'), entry('b')]);
  const removed = root.children[1];
  bars.render([entry('a')]);
  assert.strictEqual(root.children.length, 1);
  assert.strictEqual(removed.parentNode, null, 'la barre retirée est restée attachée');
});

check('réordonne sans recréer', () => {
  const root = el('div');
  const bars = createGoalBars(root);
  bars.render([entry('a'), entry('b')]);
  const [a, b] = root.children;
  bars.render([entry('b'), entry('a')]);
  assert.strictEqual(root.children[0].uid, b.uid);
  assert.strictEqual(root.children[1].uid, a.uid);
});

check('une source indisponible affiche « – » et masque le pourcentage', () => {
  const root = el('div');
  const bars = createGoalBars(root);
  bars.render([entry('a', { current: null, percent: null })]);
  const bar = root.children[0];
  assert.strictEqual(text(bar, 'goal-current'), '–');
  assert.ok(bar.querySelector('.goal-percent').classList.contains('goal-hide'));
  // Pas de 0 % trompeur, mais la barre reste vide et non débordante.
  assert.strictEqual(bar.querySelector('.goal-fill').style.width, '0%');
});

check('showNumbers / showPercent masquent leurs blocs', () => {
  const root = el('div');
  const bars = createGoalBars(root);
  bars.render([entry('a', { config: { showNumbers: false, showPercent: false } })]);
  const bar = root.children[0];
  assert.ok(bar.querySelector('.goal-numbers').classList.contains('goal-hide'));
  assert.ok(bar.querySelector('.goal-percent').classList.contains('goal-hide'));
});

check('objectif atteint → classe complete', () => {
  const root = el('div');
  const bars = createGoalBars(root);
  bars.render([entry('a', { percent: 100 })]);
  assert.ok(root.children[0].classList.contains('complete'));
  bars.render([entry('a', { percent: 40 })]);
  assert.ok(!root.children[0].classList.contains('complete'), 'complete non retirée');
});

check('l’avertissement Twitch passe par textContent, jamais par du HTML', () => {
  const root = el('div');
  const bars = createGoalBars(root);
  const payload = '<img src=x onerror="alert(1)">';
  bars.render([entry('a', { warning: payload })]);
  const warn = root.children[0].querySelector('.goal-warning');
  assert.strictEqual(warn.textContent, payload, 'le texte doit rester littéral');
  assert.strictEqual(warn.children.length, 0, 'aucun nœud construit depuis la charge utile');
  assert.ok(!warn.classList.contains('goal-hide'));

  bars.render([entry('a')]);
  assert.ok(warn.classList.contains('goal-hide'), 'l’avertissement doit disparaître');
});

check('sans accentColor, la surcharge est retirée (retour à l’accent du thème)', () => {
  const root = el('div');
  const bars = createGoalBars(root);
  bars.render([entry('a', { config: { accentColor: '#ff0000' } })]);
  const bar = root.children[0];
  assert.strictEqual(bar.style.getPropertyValue('--accent'), '#ff0000');
  bars.render([entry('a', { config: { accentColor: '' } })]);
  assert.strictEqual(bar.style.getPropertyValue('--accent'), '');
});

check('deux jeux de barres ne se volent pas leurs nœuds', () => {
  // C'est la propriété qui empêche la carte sortante de se vider en plein fondu.
  const dock = el('div');
  const card = el('div');
  const dockBars = createGoalBars(dock);
  const cardBars = createGoalBars(card);
  dockBars.render([entry('a')]);
  cardBars.render([entry('a')]);
  assert.strictEqual(dock.children.length, 1);
  assert.strictEqual(card.children.length, 1);
  assert.notStrictEqual(dock.children[0].uid, card.children[0].uid);
});

console.log(`\n  ${passed} assertions de rendu OK`);
