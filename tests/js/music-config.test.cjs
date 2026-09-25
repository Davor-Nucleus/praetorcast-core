'use strict';
/**
 * `/music-config`, utilisée comme dock OBS.
 *
 * La mise en page se vérifie à l'œil dans OBS ; ici, ce qui doit rester lisible
 * quand la place manque : l'état des interrupteurs (y compris « inconnu » quand un
 * serveur est coupé), les noms de sons sans extension, leurs raccourcis, et les
 * messages qui remplacent un « Chargement… » éternel.
 */
const fs = require('fs');
const path = require('path');
const assert = require('assert');
const { el, makeDocument } = require('./dom-stub.cjs');

const TPL = path.resolve(__dirname, '../../templates/music_config.html');

function inlineScript() {
  const html = fs.readFileSync(TPL, 'utf8');
  return /<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/.exec(html)[1]
    .replace(/\{\{\s*music_port\s*\}\}/g, '3001')
    .replace(/\{\{\s*soundboard_port\s*\}\}/g, '3002')
    .replace(/\{\{\s*shortcuts_json\|safe\s*\}\}/g, JSON.stringify({ 1: 'alt+1', 2: 'alt+2' }));
}

/** Interrupteur tel que dans le template : pastille, nom, état. */
function toggle() {
  const btn = el('button', 'toggle is-unknown');
  btn.appendChild(el('span', 'toggle-dot'));
  btn.appendChild(el('span'));
  btn.appendChild(el('span', 'toggle-state'));
  return btn;
}

function mount(routes = {}) {
  const ids = {
    normBtn: toggle(), progressBtn: toggle(), obsLimiterBtn: toggle(),
    volume: el('span'), obsThreshold: el('span'),
    currentMusic: el('div', 'now-title is-empty'),
    folderSelect: el('select'), soundboardGrid: el('div', 'sb-grid'),
  };
  const fetchStub = async (url) => {
    for (const [suffix, body] of Object.entries(routes)) {
      if (url.endsWith(suffix)) {
        if (body instanceof Error) throw body;
        return { ok: true, json: async () => body };
      }
    }
    throw new Error('hors ligne');
  };
  const api = new Function(
    'document', 'window', 'fetch', 'console', 'connectMusicWS', 'connectObsLimiterWS',
    inlineScript() + `
    ; return { setToggle, setObsLimiterUI, showTrack, withoutExtension, shortcutFor,
               loadSoundboard, loadFolders, updateVolume, soundboardSounds: () => soundboardSounds };`
  )(
    makeDocument(ids), { addEventListener() {} }, fetchStub, { error() {} },
    () => ({ close() {} }), () => ({ close() {} })
  );
  return { api, ids };
}

const state = (btn) => btn.querySelector('.toggle-state').textContent;

let passed = 0;
const check = async (label, fn) => {
  try { await fn(); passed++; console.log(`  ok    ${label}`); }
  catch (e) { console.log(`  ECHEC ${label}\n        ${e.message}`); process.exitCode = 1; }
};

(async () => {

await check('un interrupteur affiche ON / OFF en toutes lettres, pas seulement en couleur', () => {
  const { api, ids } = mount();
  api.setToggle('normBtn', true);
  assert.strictEqual(state(ids.normBtn), 'ON');
  assert.ok(ids.normBtn.classList.contains('is-on'));
  assert.strictEqual(ids.normBtn.getAttribute('aria-pressed'), 'true');

  api.setToggle('normBtn', false);
  assert.strictEqual(state(ids.normBtn), 'OFF');
  assert.ok(!ids.normBtn.classList.contains('is-on'));
  assert.ok(!ids.normBtn.classList.contains('is-unknown'));
});

await check('OBS coupé : le limiteur dit pourquoi au lieu d\'afficher OFF', () => {
  const { api, ids } = mount();
  api.setObsLimiterUI(null);
  assert.strictEqual(state(ids.obsLimiterBtn), 'OBS absent');
  assert.ok(ids.obsLimiterBtn.classList.contains('is-unknown'));

  api.setObsLimiterUI({ enabled: true, threshold: -6.4 });
  assert.strictEqual(state(ids.obsLimiterBtn), 'ON');
  assert.strictEqual(ids.obsThreshold.textContent, '-6');
  assert.ok(!ids.obsLimiterBtn.classList.contains('is-unknown'));
});

await check('le titre en cours perd son extension, garde le nom complet en infobulle', () => {
  const { api, ids } = mount();
  api.showTrack('Mr. Brightside.mp3');
  assert.strictEqual(ids.currentMusic.textContent, 'Mr. Brightside');
  assert.strictEqual(ids.currentMusic.title, 'Mr. Brightside.mp3');
  assert.ok(!ids.currentMusic.classList.contains('is-empty'));

  api.showTrack(null);
  assert.strictEqual(ids.currentMusic.textContent, 'Rien en lecture');
  assert.ok(ids.currentMusic.classList.contains('is-empty'));
});

await check('les sons affichent leur nom sans extension et leur raccourci', async () => {
  const { api, ids } = mount({ '/api/soundboard/sounds': { sounds: ['FOR THE EMPEROR.mp3', 'applause.wav', 'boom.flac'] } });
  await api.loadSoundboard();
  const buttons = ids.soundboardGrid.children;
  assert.strictEqual(buttons.length, 3);
  assert.strictEqual(buttons[0].querySelector('.sb-name').textContent, 'FOR THE EMPEROR');
  assert.strictEqual(buttons[0].title, 'FOR THE EMPEROR.mp3');
  assert.strictEqual(buttons[0].querySelector('.sb-key').textContent, 'Alt+1');
  assert.ok(buttons[0].classList.contains('has-key'));
  // Troisième son : pas de raccourci configuré, pas de pastille vide.
  assert.strictEqual(buttons[2].querySelector('.sb-key'), null);
  assert.ok(!buttons[2].classList.contains('has-key'));
});

await check('PhonosCore coupé ou dossier vide : un message plutôt qu\'une grille muette', async () => {
  const off = mount();
  await off.api.loadSoundboard();
  assert.match(off.ids.soundboardGrid.textContent, /PhonosCore/);

  const empty = mount({ '/api/soundboard/sounds': { sounds: [] } });
  await empty.api.loadSoundboard();
  assert.match(empty.ids.soundboardGrid.textContent, /Aucun son/);
});

await check('JanusCore coupé : la liste des playlists le dit au lieu de « Chargement… »', async () => {
  const { api, ids } = mount();
  await api.loadFolders();
  assert.match(ids.folderSelect.innerHTML, /JanusCore injoignable/);
});

await check('le volume s\'affiche en pourcentage entier', async () => {
  const { api, ids } = mount({ '/api/volume': { volume: 0.456 } });
  await api.updateVolume('get');
  assert.strictEqual(ids.volume.textContent, '46');
});

console.log(`  ${passed} verification(s) OK`);

})();
