'use strict';
const fs = require('fs');
const path = require('path');
const { el, makeDocument } = require('./dom-stub.cjs');

const TPL_DIR = path.resolve(__dirname, '../../templates');

function inlineScript(file) {
  const html = fs.readFileSync(`${TPL_DIR}/${file}`, 'utf8');
  return /<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/.exec(html)[1];
}

/**
 * Monte partials/_text_render.html sur le DOM factice, avec des minuteries
 * pilotées à la main : l'effet continu ne démarre qu'à la fin de l'entrée, et
 * c'est précisément ce décalage qu'on vient vérifier.
 */
function mountRenderer() {
  const timers = [];
  const setTimeout = (fn, ms) => {
    timers.push({ fn, ms, cleared: false });
    return timers.length - 1;
  };
  const clearTimeout = (id) => {
    if (timers[id]) timers[id].cleared = true;
  };
  const runTimers = () => {
    for (const t of timers) {
      if (!t.cleared) {
        t.cleared = true;
        t.fn();
      }
    }
  };

  const document = makeDocument();
  const createTextRenderer = new Function(
    'document', 'setTimeout', 'clearTimeout',
    inlineScript('partials/_text_render.html') + '\n; return createTextRenderer;'
  )(document, setTimeout, clearTimeout);

  const root = el('div', 'text-stage-root');
  return { renderer: createTextRenderer(root), root, timers, runTimers };
}

/** Les quatre niveaux du rendu : scène → bloc → ligne → contenu. */
function parts(root) {
  const stage = root.children[0];
  const block = stage.children[0];
  const line = block.children[0];
  return { stage, block, line, content: line.children[0] };
}

const section = (over = {}) => ({
  name: 'start',
  label: 'Écran de début',
  content: 'Bonjour',
  animation: 'none',
  effect: 'none',
  animationMs: 800,
  effectMs: 2000,
  scale: 1,
  color: '',
  align: 'center',
  verticalAlign: 'middle',
  backgroundColor: '',
  order: 0,
  ...over,
});

let passed = 0;
const check = (label, fn) => {
  try { fn(); passed++; console.log(`  ok    ${label}`); }
  catch (e) { console.log(`  ECHEC ${label}\n        ${e.message}`); process.exitCode = 1; }
};
const assert = require('assert');

// --- Empreinte : le cœur du « ne pas rejouer l'animation » -------------------
// Le WebSocket pousse toute la configuration chaque seconde. Sans empreinte, un
// « Save » sur une autre section relancerait le typewriter en plein direct.

check('un rendu identique ne touche pas au DOM', () => {
  const { renderer, root } = mountRenderer();
  assert.strictEqual(renderer.render(section()), true);
  const stageBefore = root.children[0];
  assert.strictEqual(renderer.render(section()), false, 'le second rendu doit être ignoré');
  assert.strictEqual(root.children[0], stageBefore, 'le nœud doit être le même objet');
});

check('label et order ne provoquent pas de reconstruction', () => {
  const { renderer, root } = mountRenderer();
  renderer.render(section());
  const stageBefore = root.children[0];
  assert.strictEqual(renderer.render(section({ label: 'Autre libellé', order: 7 })), false);
  assert.strictEqual(root.children[0], stageBefore);
});

check('un changement de contenu reconstruit', () => {
  const { renderer, root } = mountRenderer();
  renderer.render(section());
  const stageBefore = root.children[0];
  assert.strictEqual(renderer.render(section({ content: 'Autre chose' })), true);
  assert.notStrictEqual(root.children[0], stageBefore);
});

check('chaque champ visuel entre dans l\'empreinte', () => {
  const { renderer } = mountRenderer();
  const base = section();
  const variations = [
    { animation: 'fade' }, { effect: 'pulse' }, { animationMs: 1200 },
    { effectMs: 3000 }, { scale: 2 }, { color: '#ff0000' },
    { align: 'left' }, { verticalAlign: 'top' }, { backgroundColor: '#000000' },
  ];
  for (const over of variations) {
    assert.notStrictEqual(
      renderer.signatureOf(base),
      renderer.signatureOf(section(over)),
      `le champ ${Object.keys(over)[0]} doit changer l'empreinte`
    );
  }
});

check('clear() oublie l\'empreinte, le rendu suivant reconstruit', () => {
  const { renderer, root } = mountRenderer();
  renderer.render(section());
  renderer.clear();
  assert.strictEqual(root.children.length, 0);
  assert.strictEqual(renderer.render(section()), true, 'après clear, le même contenu se rejoue');
});

// --- Échappement -------------------------------------------------------------
// Le contenu vient d'un champ de saisie libre.

check('un contenu balisé ressort en texte, pas en balise', () => {
  const { renderer, root } = mountRenderer();
  renderer.render(section({ content: '<script>alert(1)</script>' }));
  const { content } = parts(root);
  assert.strictEqual(content.textContent, '<script>alert(1)</script>');
  assert.ok(!content.innerHTML.includes('<script>'), 'aucune balise ne doit être produite');
  assert.ok(content.innerHTML.includes('&lt;script&gt;'));
});

check('un contenu balisé reste du texte même découpé par lettre', () => {
  const { renderer, root } = mountRenderer();
  renderer.render(section({ content: '<b>x', animation: 'typewriter' }));
  const { content } = parts(root);
  assert.strictEqual(content.textContent, '<b>x');
  assert.ok(!content.innerHTML.includes('<b>'));
});

check('le glitch passe le texte par un attribut, jamais par du balisage', () => {
  const { renderer, root } = mountRenderer();
  renderer.render(section({ effect: 'glitch', content: '"><img>' }));
  const { content } = parts(root);
  assert.strictEqual(content.getAttribute('data-text'), '"><img>');
});

// --- Découpage par lettre ----------------------------------------------------

check('typewriter découpe en lettres et indexe pour les délais CSS', () => {
  const { renderer, root } = mountRenderer();
  renderer.render(section({ content: 'abc', animation: 'typewriter' }));
  const { content } = parts(root);
  assert.strictEqual(content.children.length, 3);
  assert.strictEqual(content.children[2].style.getPropertyValue('--char-index'), '2');
  assert.strictEqual(content.style.getPropertyValue('--char-count'), '3');
});

check('wave découpe aussi, même sans animation d\'entrée', () => {
  const { renderer, root } = mountRenderer();
  renderer.render(section({ content: 'abc', effect: 'wave' }));
  assert.strictEqual(parts(root).content.children.length, 3);
});

check('cascade découpe aussi : chaque lettre porte sa propre animation', () => {
  const { renderer, root } = mountRenderer();
  renderer.render(section({ content: 'abc', animation: 'cascade' }));
  const { content } = parts(root);
  assert.strictEqual(content.children.length, 3);
  assert.strictEqual(content.style.getPropertyValue('--char-count'), '3');
});

check('scatter découpe aussi : chaque lettre vient de sa propre direction', () => {
  const { renderer, root } = mountRenderer();
  renderer.render(section({ content: 'abc', animation: 'scatter' }));
  const { content } = parts(root);
  assert.strictEqual(content.children.length, 3);
  assert.strictEqual(content.style.getPropertyValue('--char-count'), '3');
});

check('sans typewriter, cascade ni wave, le contenu reste un nœud texte', () => {
  const { renderer, root } = mountRenderer();
  renderer.render(section({ content: 'abc', animation: 'fade', effect: 'pulse' }));
  assert.strictEqual(parts(root).content.children.length, 0);

  // Les entrées ajoutées après coup ne découpent pas : `bounce` anime la ligne
  // entière, un découpage inutile empêcherait la coupure en fin de ligne.
  const other = mountRenderer();
  other.renderer.render(section({ content: 'abc', animation: 'bounce', effect: 'neon' }));
  assert.strictEqual(parts(other.root).content.children.length, 0);
});

check('typewriter ajoute un curseur, les autres non', () => {
  const withCaret = mountRenderer();
  withCaret.renderer.render(section({ animation: 'typewriter' }));
  assert.ok(parts(withCaret.root).line.querySelector('.text-cursor'));

  const without = mountRenderer();
  without.renderer.render(section({ animation: 'fade' }));
  assert.strictEqual(parts(without.root).line.querySelector('.text-cursor'), null);

  // `cascade` et `scatter` dévoilent lettre à lettre comme le typewriter, mais
  // sans curseur : les lettres arrivent en mouvement, pas sous une frappe.
  for (const animation of ['cascade', 'scatter']) {
    const perChar = mountRenderer();
    perChar.renderer.render(section({ animation }));
    assert.strictEqual(
      parts(perChar.root).line.querySelector('.text-cursor'), null,
      `${animation} ne doit pas afficher de curseur`
    );
  }
});

// --- Enchaînement entrée → effet --------------------------------------------

check('l\'effet attend la fin de l\'entrée', () => {
  const { renderer, root, timers, runTimers } = mountRenderer();
  renderer.render(section({ animation: 'fade', effect: 'pulse', animationMs: 900 }));
  const { line, content } = parts(root);

  assert.ok(line.classList.contains('anim-fade'));
  assert.ok(!content.classList.contains('fx-pulse'), 'l\'effet ne démarre pas pendant l\'entrée');
  assert.strictEqual(timers.filter((t) => !t.cleared)[0].ms, 900, 'le délai suit animationMs');

  runTimers();
  assert.ok(content.classList.contains('fx-pulse'));
  assert.ok(
    !line.classList.contains('anim-fade'),
    'la classe d\'entrée doit partir : à spécificité égale elle disputerait l\'animation à l\'effet'
  );
});

check('sans animation d\'entrée, l\'effet est posé immédiatement', () => {
  const { renderer, root, timers } = mountRenderer();
  renderer.render(section({ animation: 'none', effect: 'gradient' }));
  assert.ok(parts(root).content.classList.contains('fx-gradient'));
  assert.strictEqual(timers.filter((t) => !t.cleared).length, 0, 'aucune minuterie en attente');
});

check('sans effet, aucune minuterie n\'est armée', () => {
  const { renderer, timers } = mountRenderer();
  renderer.render(section({ animation: 'fade', effect: 'none' }));
  assert.strictEqual(timers.length, 0);
});

check('une reconstruction annule la minuterie de la version précédente', () => {
  const { renderer, root, timers, runTimers } = mountRenderer();
  renderer.render(section({ animation: 'fade', effect: 'pulse' }));
  const staleLine = parts(root).line;

  renderer.render(section({ content: 'Autre', animation: 'fade', effect: 'wave' }));
  runTimers();

  assert.ok(!staleLine.children[0].classList.contains('fx-pulse'), 'l\'ancien nœud reste intact');
  assert.ok(parts(root).content.classList.contains('fx-wave'));
});

check('clear() annule la minuterie en attente', () => {
  const { renderer, root, runTimers } = mountRenderer();
  renderer.render(section({ animation: 'fade', effect: 'pulse' }));
  const stale = parts(root).content;
  renderer.clear();
  runTimers();
  assert.ok(!stale.classList.contains('fx-pulse'));
});

// --- Géométrie et valeurs hors domaine ---------------------------------------

check('align, verticalAlign et marquee posent leurs classes', () => {
  const { renderer, root } = mountRenderer();
  renderer.render(section({ align: 'left', verticalAlign: 'bottom', effect: 'marquee' }));
  const { stage } = parts(root);
  assert.ok(stage.classList.contains('align-left'));
  assert.ok(stage.classList.contains('valign-bottom'));
  assert.ok(stage.classList.contains('has-marquee'), 'le marquee élargit le bloc à toute la scène');
});

/**
 * Le maillon que ne couvre pas text-config.test.cjs.
 *
 * Ce dernier vérifie qu'une variante Rust figure bien dans les deux JS ; il ne
 * dit rien de la feuille de style. Une entrée acceptée par `oneOf` mais sans
 * règle CSS poserait sa classe et ne bougerait pas — un texte inerte, sans la
 * moindre erreur en console. On remonte donc jusqu'aux règles.
 */
check('chaque animation proposée pose sa classe ET a sa règle CSS', () => {
  const partial = fs.readFileSync(`${TPL_DIR}/partials/_text_render.html`, 'utf8');
  const config = fs.readFileSync(`${TPL_DIR}/text_config.html`, 'utf8');

  const listed = (name) =>
    new RegExp(`const ${name} = \\[([^\\]]*)\\]`).exec(partial)[1]
      .split(',')
      .map((v) => v.trim().replace(/^'|'$/g, ''))
      .filter(Boolean);

  // Le configurateur liste des paires `['clé', 'Libellé']` : seule la clé compte.
  const offered = (name) =>
    Array.from(
      new RegExp(`const ${name} = \\[([^\\]]*\\])[^;]*`).exec(config)[0].matchAll(/\['([a-z]+)'/g),
      (m) => m[1]
    ).filter((v) => v !== 'none');

  // Garde-fou : sans lui, une regex qui cesserait de capturer ferait passer ce
  // test à vide — il ne vérifierait plus rien tout en restant vert.
  assert.ok(listed('ENTRANCES').length >= 16, 'liste des entrées non capturée');
  assert.ok(listed('EFFECTS').length >= 15, 'liste des effets non capturée');

  // Le configurateur ne doit ni cacher une animation existante, ni en proposer
  // une que le rendu ignorerait (`oneOf` la ramènerait au défaut, sans erreur).
  assert.deepStrictEqual(offered('ENTRANCES'), listed('ENTRANCES'));
  assert.deepStrictEqual(offered('EFFECTS'), listed('EFFECTS'));

  for (const animation of listed('ENTRANCES')) {
    const { renderer, root } = mountRenderer();
    renderer.render(section({ animation }));
    assert.ok(
      parts(root).line.classList.contains(`anim-${animation}`),
      `${animation} devrait poser anim-${animation}`
    );
    assert.ok(
      new RegExp(`\\.anim-${animation}[\\s.{]`).test(partial),
      `anim-${animation} n'a aucune règle CSS : le texte resterait immobile`
    );
  }

  for (const effect of listed('EFFECTS')) {
    // `runTimers` : l'effet n'est posé qu'à la fin de l'entrée, et `section()`
    // part sans entrée — le minuteur est donc déjà écoulé, mais on le force pour
    // ne pas dépendre de ce détail.
    const { renderer, root, runTimers } = mountRenderer();
    renderer.render(section({ effect }));
    runTimers();
    assert.ok(
      parts(root).content.classList.contains(`fx-${effect}`),
      `${effect} devrait poser fx-${effect}`
    );
    assert.ok(
      new RegExp(`\\.fx-${effect}[\\s.,{+]`).test(partial),
      `fx-${effect} n'a aucune règle CSS : le texte resterait immobile`
    );
  }
});

check('une valeur inconnue retombe sur le défaut', () => {
  const { renderer, root } = mountRenderer();
  renderer.render(section({ animation: 'wat', effect: 'nope', align: 'x', verticalAlign: 'y' }));
  const { stage, line, content } = parts(root);
  assert.ok(stage.classList.contains('align-center'));
  assert.ok(stage.classList.contains('valign-middle'));
  assert.strictEqual(line.className, 'text-line', 'aucune classe anim-* parasite');
  assert.strictEqual(content.className, 'text-content');
});

check('une échelle ou une durée absurde retombe sur le défaut', () => {
  const { renderer, root } = mountRenderer();
  renderer.render(section({ scale: 0, animationMs: -5, effectMs: 'x' }));
  const { stage } = parts(root);
  assert.strictEqual(stage.style.getPropertyValue('--text-scale'), '1');
  assert.strictEqual(stage.style.getPropertyValue('--text-anim-ms'), '800ms');
  assert.strictEqual(stage.style.getPropertyValue('--text-effect-ms'), '2000ms');
});

check('couleur et fond ne sont posés que s\'ils sont renseignés', () => {
  const bare = mountRenderer();
  bare.renderer.render(section());
  assert.strictEqual(parts(bare.root).stage.style.getPropertyValue('--text-color'), '');
  assert.strictEqual(parts(bare.root).stage.style.getPropertyValue('--text-bg'), '');

  const painted = mountRenderer();
  painted.renderer.render(section({ color: '#9146FF', backgroundColor: '#00000080' }));
  const { stage } = parts(painted.root);
  assert.strictEqual(stage.style.getPropertyValue('--text-color'), '#9146FF');
  assert.strictEqual(stage.style.getPropertyValue('--text-bg'), '#00000080');
});

check('render(null) vide le conteneur sans planter', () => {
  const { renderer, root } = mountRenderer();
  renderer.render(section());
  assert.strictEqual(renderer.render(null), true);
  assert.strictEqual(root.children.length, 0);
});

check('un contenu vide ne casse pas le découpage par lettre', () => {
  const { renderer, root } = mountRenderer();
  renderer.render(section({ content: '', animation: 'typewriter' }));
  const { content } = parts(root);
  assert.strictEqual(content.children.length, 0);
  // Le pas du typewriter divise par --char-count : jamais 0.
  assert.strictEqual(content.style.getPropertyValue('--char-count'), '1');
});

console.log(`  ${passed} verification(s) OK`);
