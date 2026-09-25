'use strict';
/**
 * Dock `/followers-info` : dernier follower, état du live, stats, objectifs,
 * activité.
 *
 * Tout ce qui se calcule se vérifie ici : la fenêtre et les totaux des stats, les
 * durées, l'avertissement sur le jeton, le compte à rebours. La mise en page se
 * juge à l'œil dans OBS.
 */
const fs = require('fs');
const path = require('path');
const assert = require('assert');
const { el, makeDocument } = require('./dom-stub.cjs');

const TPL_DIR = path.resolve(__dirname, '../../templates');

function inlineScript(file) {
  const html = fs.readFileSync(`${TPL_DIR}/${file}`, 'utf8');
  return /<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/.exec(html)[1]
    .replace(/\{\{\s*music_port\s*\}\}/g, '3001');
}

const IDS = [
  'livePill', 'liveText', 'viewers', 'viewersCount', 'twitchState', 'streamTitle', 'tokenWarning',
  'totalFollowers', 'followDelta', 'lastFollower', 'lastFollowerAgo',
  'statsTitle', 'statFollows', 'statSubs', 'statGifts', 'statBits', 'statRaids', 'statRaidViewers', 'statRaidsTile',
  'goalsPanel', 'goalsTitle', 'goals', 'timerRow', 'timerValue', 'timerState',
  'activity', 'musicRow', 'currentMusic',
];

function mount({ storage } = {}) {
  const ids = {};
  for (const id of IDS) ids[id] = el('div');
  const document = makeDocument(ids);
  const partial = new Function(
    'document', inlineScript('partials/_event_card.html') + '\n; return { eventMatches, describeEvent };'
  )(document);
  const window = { addEventListener() {}, localStorage: storage };
  const api = new Function(
    'document', 'window', 'fetch', 'WebSocket', 'location', 'setInterval', 'setTimeout',
    'eventMatches', 'describeEvent',
    inlineScript('followers_info.html') + `
    ; return {
        timeAgo, formatDuration, compact, sessionStats, statsWindow, tokenWarning,
        timerVisible, timerRemaining, goalsLabel, loadCollapsed, saveCollapsed,
        renderStatus, renderFollowers, renderStats, renderGoals, renderActivity, applyEventMessage, showTrack,
        set: (key, value) => { ({ stream: () => { stream = value; }, twitch: () => { twitch = value; },
          timer: () => { timer = value; }, goals: () => { goals = value; }, events: () => { events = value; } })[key](); },
    };`
  )(
    document, window, async () => ({ ok: false }), function () {}, { protocol: 'http:', host: 'x' },
    () => 0, () => 0, partial.eventMatches, partial.describeEvent
  );
  return { api, ids };
}

const MIN = 60 * 1000;
const NOW = Date.parse('2026-09-25T20:00:00Z');
const ev = (kind, userName, amount = 0, minutesAgo = 1, extra = {}) =>
  ({ kind, userName, amount, months: 0, atMs: NOW - minutesAgo * MIN, ...extra });

let passed = 0;
const check = (label, fn) => {
  try { fn(); passed++; console.log(`  ok    ${label}`); }
  catch (e) { console.log(`  ECHEC ${label}\n        ${e.message}`); process.exitCode = 1; }
};

// ── Fonctions pures ─────────────────────────────────────────────────────────

check('« il y a » : instant, minutes, heures, jours', () => {
  const { api } = mount();
  assert.strictEqual(api.timeAgo(NOW - 10 * 1000, NOW), 'à l\'instant');
  assert.strictEqual(api.timeAgo(NOW - 4 * MIN, NOW), '4 min');
  assert.strictEqual(api.timeAgo(NOW - 125 * MIN, NOW), '2 h');
  assert.strictEqual(api.timeAgo(NOW - 3 * 24 * 60 * MIN, NOW), '3 j');
  assert.strictEqual(api.timeAgo(NOW + MIN, NOW), 'à l\'instant', 'une horloge en avance ne donne pas de négatif');
});

check('durées en m:ss puis h:mm:ss', () => {
  const { api } = mount();
  assert.strictEqual(api.formatDuration(65 * 1000), '1:05');
  assert.strictEqual(api.formatDuration((83 * 60 + 45) * 1000), '1:23:45');
  assert.strictEqual(api.formatDuration(-5), '0:00');
});

check('nombres courts pour les tuiles', () => {
  const { api } = mount();
  assert.strictEqual(api.compact(999), '999');
  assert.strictEqual(api.compact(1500), '1,5k');
  assert.strictEqual(api.compact(2000), '2k');
  assert.strictEqual(api.compact(15400), '15k');
  assert.strictEqual(api.compact(1234567), '1,2M');
});

check('les stats comptent chaque type à sa façon, dons compris', () => {
  const { api } = mount();
  const stats = api.sessionStats([
    ev('follow', 'a'), ev('follow', 'b'),
    ev('sub', 'c', 1000), ev('resub', 'd', 1000), ev('prime', 'e', 1000),
    ev('gift', 'f', 5),
    ev('cheer', 'g', 500), ev('cheer', 'h', 1000),
    ev('raid', 'i', 42),
    ev('channel_points', 'j'),
  ], NOW - 60 * MIN);
  assert.deepStrictEqual(stats, { follows: 2, subs: 3, gifts: 5, bits: 1500, raids: 1, raiders: 42 });
});

check('les stats ignorent les tests et ce qui précède la fenêtre', () => {
  const { api } = mount();
  const stats = api.sessionStats([
    ev('follow', 'dans', 0, 10),
    ev('follow', 'avant', 0, 120),
    ev('cheer', 'TestUser', 100, 5, { test: true }),
  ], NOW - 60 * MIN);
  assert.strictEqual(stats.follows, 1);
  assert.strictEqual(stats.bits, 0);
});

check('fenêtre : le live en cours, sinon les 24 dernières heures', () => {
  const { api } = mount();
  const live = api.statsWindow({ live: true, startedAt: '2026-09-25T18:30:00Z' }, NOW);
  assert.deepStrictEqual(live, { since: Date.parse('2026-09-25T18:30:00Z'), label: 'Ce live' });
  const off = api.statsWindow({ live: false }, NOW);
  assert.deepStrictEqual(off, { since: NOW - 24 * 60 * MIN, label: '24 dernières heures' });
  assert.strictEqual(api.statsWindow(null, NOW).label, '24 dernières heures', 'Twitch injoignable');
});

check('le jeton n\'est signalé qu\'en cas de problème', () => {
  const { api } = mount();
  assert.strictEqual(api.tokenWarning({ valid: true, scopesMissing: [], expiresIn: 30 * 86400 }), null);
  assert.strictEqual(api.tokenWarning(null), null, 'serveur injoignable : pas de fausse alerte');
  assert.match(api.tokenWarning({ valid: true, scopesMissing: ['bits:read', 'user:read:chat'], expiresIn: 1e7 }),
    /Droits manquants : bits:read, user:read:chat/);
  assert.match(api.tokenWarning({ valid: true, scopesMissing: [], expiresIn: 3 * 86400 + 60 }), /expire dans 3 j/);
  assert.match(api.tokenWarning({ valid: true, scopesMissing: [], expiresIn: 5 * 3600 }), /expire dans 5 h/);
  assert.match(api.tokenWarning({ valid: false }), /invalide/);
  assert.match(api.tokenWarning({ hasToken: false }), /Pas de jeton/);
});

check('le compte à rebours n\'apparaît que s\'il sert', () => {
  const { api } = mount();
  const idle = { running: false, remainingMs: 300000, durationMs: 300000, subathon: { enabled: false } };
  assert.ok(!api.timerVisible(idle), 'au repos');
  assert.ok(!api.timerVisible({ ...idle, remainingMs: 0 }), 'terminé');
  assert.ok(api.timerVisible({ ...idle, running: true }), 'en cours');
  assert.ok(api.timerVisible({ ...idle, remainingMs: 120000 }), 'en pause, entamé');
  assert.ok(api.timerVisible({ ...idle, subathon: { enabled: true } }), 'subathon');
  assert.ok(!api.timerVisible(null));
});

check('le temps restant s\'extrapole seulement quand le compte tourne', () => {
  const { api } = mount();
  const t = { running: true, remainingMs: 60000, receivedAt: NOW };
  assert.strictEqual(api.timerRemaining(t, NOW + 15000), 45000);
  assert.strictEqual(api.timerRemaining({ ...t, running: false }, NOW + 15000), 60000);
  assert.strictEqual(api.timerRemaining(t, NOW + 90000), 0);
});

check('le titre de la section suit son contenu', () => {
  const { api } = mount();
  assert.strictEqual(api.goalsLabel(2, false), 'Objectifs');
  assert.strictEqual(api.goalsLabel(0, true), 'Compte à rebours');
  assert.strictEqual(api.goalsLabel(1, true), 'Objectifs · timer');
});

check('sections repliées : un stockage refusé ou corrompu ne casse rien', () => {
  const throwing = { getItem() { throw new Error('refusé'); }, setItem() { throw new Error('plein'); } };
  const { api } = mount({ storage: throwing });
  assert.deepStrictEqual(api.loadCollapsed(), []);
  api.saveCollapsed(['stats']); // ne doit pas lever

  const store = new Map();
  const memory = { getItem: (k) => store.get(k) ?? null, setItem: (k, v) => store.set(k, v) };
  const ok = mount({ storage: memory });
  ok.api.saveCollapsed(['activity', 'goals']);
  assert.deepStrictEqual(ok.api.loadCollapsed(), ['activity', 'goals']);
  store.set('praetorcast.followersInfo.collapsed', '{pas du json');
  assert.deepStrictEqual(ok.api.loadCollapsed(), []);
});

// ── Rendu ───────────────────────────────────────────────────────────────────

check('en live : durée, spectateurs, catégorie et titre', () => {
  const { api, ids } = mount();
  api.set('stream', { live: true, startedAt: new Date(NOW - 83 * MIN).toISOString(), viewers: 1520, title: 'Pour l\'Empereur !', game: 'Space Marine 2' });
  api.set('twitch', { connected: true });
  api.renderStatus(NOW);
  assert.strictEqual(ids.liveText.textContent, 'LIVE 1:23:00');
  assert.ok(ids.livePill.classList.contains('is-live'));
  assert.strictEqual(ids.viewers.hidden, false);
  assert.strictEqual(ids.viewersCount.textContent, '1,5k');
  assert.strictEqual(ids.streamTitle.textContent, 'Space Marine 2 · Pour l\'Empereur !');
  assert.strictEqual(ids.twitchState.textContent, 'Twitch ✓');
});

check('hors ligne ou inconnu, sans spectateurs affichés', () => {
  const { api, ids } = mount();
  api.set('stream', { live: false });
  api.set('twitch', { connected: false });
  api.renderStatus(NOW);
  assert.strictEqual(ids.liveText.textContent, 'Hors ligne');
  assert.strictEqual(ids.viewers.hidden, true);
  assert.strictEqual(ids.twitchState.textContent, 'Twitch ✕');

  api.set('stream', null);
  api.renderStatus(NOW);
  assert.strictEqual(ids.liveText.textContent, 'Live ?');
});

check('dernier follower avec son ancienneté, et le gain du live', () => {
  const { api, ids } = mount();
  api.set('twitch', { total_followers: 1234, last_follower: 'Ronni', lastFollowerAt: new Date(NOW - 4 * MIN).toISOString() });
  api.set('stream', { live: true, startedAt: new Date(NOW - 60 * MIN).toISOString() });
  api.set('events', [ev('follow', 'Ronni', 0, 4), ev('follow', 'Bob', 0, 30)]);
  api.renderFollowers(NOW);
  api.renderStats(NOW);
  assert.strictEqual(ids.lastFollower.textContent, 'Ronni');
  assert.strictEqual(ids.lastFollowerAgo.textContent, '4 min');
  assert.strictEqual(ids.followDelta.textContent, '+2 ce live');
  assert.strictEqual(ids.statsTitle.textContent, 'Ce live');
});

check('l\'activité liste les derniers événements, sans les tests, pseudos en texte', () => {
  const { api, ids } = mount();
  api.set('events', [
    ev('cheer', '<b>gras</b>', 500, 2),
    ev('raid', 'Raider', 42, 9, {}),
    ev('follow', 'TestUser', 0, 1, { test: true }),
    ev('channel_points', 'Pointu', 0, 12),
  ]);
  api.renderActivity(NOW);
  const rows = ids.activity.children;
  assert.strictEqual(rows.length, 3);
  assert.strictEqual(rows[0].querySelector('.who').textContent, '<b>gras</b>');
  assert.strictEqual(rows[0].querySelector('.what').textContent, '500 bits');
  assert.strictEqual(rows[1].querySelector('.what').textContent, '42 spectateurs');
  assert.strictEqual(rows[1].querySelector('.when').textContent, '9 min');
  assert.strictEqual(rows[2].querySelector('.what').textContent, 'récompense');
});

check('un nouvel événement de test n\'entre ni dans l\'activité ni dans les stats', () => {
  const { api, ids } = mount();
  api.applyEventMessage({ type: 'snapshot', events: [] });
  api.applyEventMessage({ type: 'event', event: ev('raid', 'TestUser', 10, 0, { test: true }) });
  assert.match(ids.activity.textContent, /Aucun événement/);
  assert.strictEqual(ids.statRaids.textContent, '0');
});

check('objectifs visibles seulement, section masquée sans objectif ni timer', () => {
  const { api, ids } = mount();
  api.set('goals', [
    { config: { id: 'a', title: 'Followers', target: 200, visible: true, accentColor: '#9146FF' }, current: 150, percent: 75 },
    { config: { id: 'b', title: 'Caché', target: 10, visible: false }, current: 1, percent: 10 },
  ]);
  api.renderGoals(NOW);
  assert.strictEqual(ids.goalsPanel.hidden, false);
  assert.strictEqual(ids.goals.children.length, 1);
  assert.strictEqual(ids.goals.children[0].querySelector('.goal-value').textContent, '150/200 · 75 %');

  api.set('goals', []);
  api.renderGoals(NOW);
  assert.strictEqual(ids.goalsPanel.hidden, true);
});

check('un objectif de followers se lit au follower près, un gros objectif en court', () => {
  const { api, ids } = mount();
  api.set('goals', [
    { config: { id: 'a', title: 'Followers', target: 1300, visible: true }, current: 1234, percent: 94.9 },
    { config: { id: 'b', title: 'Bits', target: 100000, visible: true }, current: 15400, percent: 15.4 },
  ]);
  api.renderGoals(NOW);
  // `toLocaleString('fr-FR')` sépare les milliers par une espace fine insécable.
  assert.match(ids.goals.children[0].querySelector('.goal-value').textContent, /^1\s234\/1\s300 · 95 %$/);
  assert.strictEqual(ids.goals.children[1].querySelector('.goal-value').textContent, '15k/100k · 15 %');
});

check('la musique perd son extension et signale la pause', () => {
  const { api, ids } = mount();
  api.showTrack({ current_music: 'Zoltraak.mp3', paused: true });
  assert.strictEqual(ids.currentMusic.textContent, 'Zoltraak ⏸');
  api.showTrack({ current_music: null });
  assert.strictEqual(ids.currentMusic.textContent, 'rien en lecture');
});

console.log(`  ${passed} verification(s) OK`);
