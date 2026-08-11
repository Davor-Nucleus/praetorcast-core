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

/**
 * Monte text.html avec une query string donnée.
 *
 * C'est ici que se joue la particularité de l'overlay : le serveur pousse TOUTES
 * les sections et c'est la page qui retrouve la sienne, à chaque envoi.
 */
function mountOverlay(search, { fetchImpl } = {}) {
  const ids = {
    textStage: el('div', 'text-stage-root'),
    emptyState: el('div', 'empty-state'),
    emptyTitle: el('p', 'empty-title'),
    emptyHint: el('p', 'empty-hint'),
  };

  const document = makeDocument(ids);
  const loadHandlers = [];
  const window = {
    addEventListener(type, fn) { if (type === 'load') loadHandlers.push(fn); },
  };

  const sockets = [];
  function WebSocketStub(url) {
    const listeners = new Map();
    const socket = {
      url,
      addEventListener(type, fn) {
        if (!listeners.has(type)) listeners.set(type, []);
        listeners.get(type).push(fn);
      },
      emit(type, event) { for (const fn of listeners.get(type) || []) fn(event); },
    };
    sockets.push(socket);
    return socket;
  }

  const fetched = [];
  const fetchStub = fetchImpl || (async (url) => {
    fetched.push(url);
    return { ok: true, json: async () => ({ sections: [] }) };
  });

  const createTextRenderer = new Function(
    'document', 'setTimeout', 'clearTimeout',
    inlineScript('partials/_text_render.html') + '\n; return createTextRenderer;'
  )(document, () => 0, () => {});

  const api = new Function(
    'document', 'window', 'location', 'WebSocket', 'fetch', 'console',
    'setTimeout', 'URLSearchParams', 'createTextRenderer',
    inlineScript('text.html') + `
    ; return {
        applyTextConfig, applyAll, findSection, connectWs, loadConfigFallback,
        wanted: () => wanted,
        sections: () => sections,
    };`
  )(
    document, window,
    { protocol: 'http:', host: '127.0.0.1:3000', origin: 'http://127.0.0.1:3000', search },
    WebSocketStub, fetchStub,
    { error() {}, log() {} },
    () => 0,
    URLSearchParams,
    createTextRenderer
  );

  /** Chemin normal : le WS livre une configuration. */
  const push = (payload) => {
    api.applyTextConfig(payload);
    api.applyAll();
  };

  return { api, ids, sockets, fetched, loadHandlers, push };
}

const CONFIG = {
  sections: [
    { name: 'start', label: 'Début', content: 'Le stream commence !', animation: 'fade', effect: 'none', animationMs: 800, effectMs: 2000, scale: 1, align: 'center', verticalAlign: 'middle', color: '', backgroundColor: '' },
    { name: 'brb', label: 'Pause', content: 'Je reviens', animation: 'none', effect: 'none', animationMs: 800, effectMs: 2000, scale: 1, align: 'center', verticalAlign: 'middle', color: '', backgroundColor: '' },
  ],
};

/** Texte réellement peint dans la scène. */
const painted = (ids) => ids.textStage.textContent;

let passed = 0;
const check = async (label, fn) => {
  try { await fn(); passed++; console.log(`  ok    ${label}`); }
  catch (e) { console.log(`  ECHEC ${label}\n        ${e.message}`); process.exitCode = 1; }
};

(async () => {

await check('le script s\'évalue et enregistre son écouteur "load"', () => {
  const { loadHandlers } = mountOverlay('?name=start');
  assert.strictEqual(loadHandlers.length, 1);
});

// --- Sélection par l'URL -----------------------------------------------------

await check('?name= sélectionne la bonne section', () => {
  const { ids, push } = mountOverlay('?name=brb');
  push(CONFIG);
  assert.strictEqual(painted(ids), 'Je reviens');
  assert.ok(ids.emptyState.classList.contains('hide'), 'l\'invite doit disparaître');
});

await check('la comparaison ignore la casse et les espaces de l\'URL', () => {
  const { ids, push } = mountOverlay('?name=%20START%20');
  push(CONFIG);
  assert.strictEqual(painted(ids), 'Le stream commence !');
});

await check('la comparaison ignore aussi la casse du nom enregistré', () => {
  const { ids, push } = mountOverlay('?name=start');
  push({ sections: [{ ...CONFIG.sections[0], name: 'Start' }] });
  assert.strictEqual(painted(ids), 'Le stream commence !');
});

await check('un nom inconnu affiche l\'invite et liste les sections disponibles', () => {
  const { ids, push } = mountOverlay('?name=inconnu');
  push(CONFIG);
  assert.ok(!ids.emptyState.classList.contains('hide'));
  assert.ok(ids.emptyTitle.textContent.includes('inconnu'));
  assert.ok(ids.emptyHint.textContent.includes('start'));
  assert.ok(ids.emptyHint.textContent.includes('brb'));
  assert.strictEqual(painted(ids), '', 'la scène doit être vide');
});

await check('sans paramètre, l\'invite explique quoi ajouter', () => {
  const { ids, push } = mountOverlay('');
  push(CONFIG);
  assert.ok(!ids.emptyState.classList.contains('hide'));
  assert.ok(ids.emptyTitle.textContent.includes('?name='));
  assert.ok(ids.emptyHint.textContent.includes('start'));
});

await check('sans aucune section, l\'invite renvoie vers /text-config', () => {
  const { ids, push } = mountOverlay('?name=start');
  push({ sections: [] });
  assert.ok(ids.emptyHint.textContent.includes('/text-config'));
});

await check('un nom de section n\'est jamais injecté dans l\'invite', () => {
  const { ids, push } = mountOverlay('?name=inconnu');
  push({ sections: [{ name: '<img src=x onerror=alert(1)>' }] });
  // textContent, pas innerHTML : le nom ressort tel quel, sans devenir du balisage.
  assert.ok(ids.emptyHint.textContent.includes('<img src=x onerror=alert(1)>'));
  assert.ok(!ids.emptyHint.innerHTML.includes('<img src=x'));
});

// --- Mises à jour poussées par le WebSocket ----------------------------------

await check('un envoi qui ne touche pas notre section ne rejoue pas l\'animation', () => {
  const { ids, push } = mountOverlay('?name=start');
  push(CONFIG);
  const stageBefore = ids.textStage.children[0];

  // Le configurateur a modifié « brb » : notre section est inchangée.
  push({ sections: [CONFIG.sections[0], { ...CONFIG.sections[1], content: 'Autre chose' }] });

  assert.strictEqual(
    ids.textStage.children[0], stageBefore,
    'le nœud doit être le même objet — sinon le typewriter repart en plein direct'
  );
});

await check('modifier notre section met l\'affichage à jour', () => {
  const { ids, push } = mountOverlay('?name=start');
  push(CONFIG);
  push({ sections: [{ ...CONFIG.sections[0], content: 'Nouveau texte' }] });
  assert.strictEqual(painted(ids), 'Nouveau texte');
});

await check('renommer notre section fait basculer sur l\'invite', () => {
  const { ids, push } = mountOverlay('?name=start');
  push(CONFIG);
  assert.ok(ids.emptyState.classList.contains('hide'));

  push({ sections: [{ ...CONFIG.sections[0], name: 'demarrage' }] });
  assert.ok(!ids.emptyState.classList.contains('hide'), 'la section n\'existe plus sous ce nom');
  assert.strictEqual(painted(ids), '');
});

await check('la section réapparue est réaffichée', () => {
  const { ids, push } = mountOverlay('?name=start');
  push({ sections: [] });
  assert.strictEqual(painted(ids), '');
  push(CONFIG);
  assert.strictEqual(painted(ids), 'Le stream commence !');
});

await check('une charge utile absurde ne casse pas la page', () => {
  const { ids, push } = mountOverlay('?name=start');
  push(null);
  push('nope');
  push({ sections: 'pas un tableau' });
  assert.strictEqual(painted(ids), '');
  assert.ok(!ids.emptyState.classList.contains('hide'));
});

// --- Transport ---------------------------------------------------------------

await check('connectWs ouvre /api/text_ws et applique le message reçu', () => {
  const { api, ids, sockets } = mountOverlay('?name=start');
  api.connectWs();
  assert.strictEqual(sockets.length, 1);
  assert.strictEqual(sockets[0].url, 'ws://127.0.0.1:3000/api/text_ws');

  sockets[0].emit('message', { data: JSON.stringify(CONFIG) });
  assert.strictEqual(painted(ids), 'Le stream commence !');
});

await check('un message illisible n\'interrompt pas l\'affichage en cours', () => {
  const { api, ids, sockets } = mountOverlay('?name=start');
  api.connectWs();
  sockets[0].emit('message', { data: JSON.stringify(CONFIG) });
  sockets[0].emit('message', { data: '{ pas du json' });
  assert.strictEqual(painted(ids), 'Le stream commence !');
});

await check('le repli HTTP ne sert que si le WS n\'a jamais rien livré', async () => {
  const fetched = [];
  const { api, sockets } = mountOverlay('?name=start', {
    fetchImpl: async (url) => {
      fetched.push(url);
      return { ok: true, json: async () => CONFIG };
    },
  });

  api.connectWs();
  sockets[0].emit('close', {});
  await new Promise((r) => setImmediate(r));
  assert.deepStrictEqual(fetched, ['/api/text-config'], 'une fermeture à vide doit déclencher le repli');

  // Deuxième socket : cette fois le WS a livré, la fermeture ne doit rien refetch.
  api.connectWs();
  sockets[1].emit('message', { data: JSON.stringify(CONFIG) });
  sockets[1].emit('close', {});
  await new Promise((r) => setImmediate(r));
  assert.strictEqual(fetched.length, 1, 'aucun fetch après une livraison réussie');
});

await check('un fetch en échec laisse la page sur son invite', async () => {
  const { api, ids } = mountOverlay('?name=start', {
    fetchImpl: async () => { throw new Error('réseau coupé'); },
  });
  await api.loadConfigFallback();
  assert.ok(!ids.emptyState.classList.contains('hide'));
});

console.log(`  ${passed} verification(s) OK`);

})();
