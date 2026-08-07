'use strict';

/**
 * Tests des overlays — `node tests/js/run.cjs`
 *
 * `cargo test` couvre les modèles Rust, mais l'essentiel du comportement des
 * overlays vit dans le JavaScript des templates : réutilisation des barres entre
 * deux trames, empreinte qui empêche la rotation de repartir à chaque follower
 * gagné, échappement des messages venus de l'API Twitch. Rien de tout ça n'est
 * visible d'un test Rust.
 *
 * Aucune dépendance : `tests/js/dom-stub.cjs` fournit le minimum de DOM utilisé
 * par les templates, et chaque script est chargé depuis son fichier `.html` —
 * c'est donc bien le code servi qui est testé, pas une copie.
 */

const { spawnSync } = require('child_process');
const path = require('path');
const fs = require('fs');

const DIR = __dirname;
const suites = fs.readdirSync(DIR).filter((f) => f.endsWith('.test.cjs')).sort();

let failed = 0;

for (const suite of suites) {
  console.log(`\n${suite}`);
  const result = spawnSync(process.execPath, [path.join(DIR, suite)], {
    stdio: 'inherit',
    windowsHide: true,
  });
  if (result.status !== 0) failed++;
}

console.log(
  failed === 0
    ? `\nToutes les suites passent (${suites.length}).`
    : `\n${failed} suite(s) en echec sur ${suites.length}.`
);
process.exit(failed === 0 ? 0 : 1);
