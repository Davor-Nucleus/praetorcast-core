'use strict';
const fs = require('fs');
const path = require('path');
const assert = require('assert');
const { el, makeDocument } = require('./dom-stub.cjs');

const TPL_DIR = path.resolve(__dirname, '../../templates');
const JS_DIR = path.resolve(__dirname, '../../public/js');

function inlineScript(file) {
  const html = fs.readFileSync(`${TPL_DIR}/${file}`, 'utf8');
  return /<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/.exec(html)[1];
}

/**
 * Monte la logique de chat sur le DOM factice.
 *
 * Deux particularités par rapport aux autres suites :
 *
 * 1. L'essentiel du code vit dans `public/js/chat-common.js`, chargé par `src=` —
 *    `inlineScript` l'exclut volontairement. Les deux moitiés doivent être évaluées
 *    dans le **même** `new Function` et dans l'ordre de chargement de la page :
 *    `chat-common.js` lit et réassigne les variables déclarées par le bloc inline
 *    (`messages`, `ws`, `globalBadges`…), ce qui ne résoudrait pas autrement.
 * 2. Le bloc inline contient des balises Askama (`{{ port_ws_youtube_chat }}`), qui
 *    sont une erreur de syntaxe JavaScript : elles sont substituées avant montage.
 */
function mountChat() {
  const chat = el('div', 'chat');
  chat.id = 'chat';
  const document = makeDocument({ chat });

  const loadListeners = new Map();
  const window = {
    addEventListener(type, fn) {
      loadListeners.set(type, fn);
    },
  };

  // Minuteries mises en file plutôt qu'exécutées : `addMessage` arme une expiration
  // à 30 s et `removeMessage` un fondu à 500 ms. Un `setTimeout` immédiat viderait
  // le chat à chaque message ; il faut pouvoir ne faire tourner que le fondu.
  const timers = [];
  const setTimeout = (fn, delay) => {
    timers.push({ fn, delay: Number(delay) || 0 });
    return timers.length;
  };
  const runTimers = (maxDelay) => {
    const due = timers.filter((t) => t.delay <= maxDelay);
    for (const t of due) timers.splice(timers.indexOf(t), 1);
    for (const t of due) t.fn();
  };

  let uuid = 0;

  const source =
    fs.readFileSync(`${JS_DIR}/chat-common.js`, 'utf8') +
    '\n' +
    inlineScript('chat_horizontal.html')
      .replace(/\{\{\s*twitch_channel_name\s*\}\}/g, 'testchannel')
      .replace(/\{\{\s*port_ws_youtube_chat\s*\}\}/g, '3003');

  const api = new Function(
    'document', 'window', 'crypto', 'fetch', 'WebSocket', 'console', 'location',
    'setTimeout', 'clearTimeout', 'setInterval', 'clearInterval',
    source + `
    ; return {
        handleTwitchLine, addYouTubeMessage, renderMessages,
        messages: () => messages,
        setWs: (v) => { ws = v; },
        constants: () => ({ MAX_MESSAGES, TWITCH_CHANNEL_NAME, PORT_WS_YOUTUBE_CHAT }),
    };`
  )(
    document, window,
    { randomUUID: () => 'uuid-' + ++uuid },
    () => { throw new Error('aucun appel réseau ne doit partir pendant le test'); },
    function () { throw new Error('aucune WS ne doit être ouverte pendant le test'); },
    { error() {}, log() {}, warn() {} },
    { protocol: 'http:', host: '127.0.0.1:3000' },
    setTimeout, () => {}, () => 0, () => {}
  );

  return { api, chat, runTimers, timers };
}

// ── Fabriques de lignes IRC ─────────────────────────────────────────────────

function privmsg({ id = 'm1', userId = 'u1', login = 'ronni', name = 'Ronni', text = 'salut' } = {}) {
  return `@badges=;color=;display-name=${name};emotes=;id=${id};user-id=${userId} :${login}!${login}@${login}.tmi.twitch.tv PRIVMSG #testchannel :${text}`;
}

function clearmsg(targetMsgId, login = 'ronni') {
  return `@login=${login};room-id=1;target-msg-id=${targetMsgId} :tmi.twitch.tv CLEARMSG #testchannel :message supprimé`;
}

function clearchat(targetUserId, login) {
  const tags = targetUserId ? `@room-id=1;target-user-id=${targetUserId}` : '@room-id=1';
  return `${tags} :tmi.twitch.tv CLEARCHAT #testchannel${login ? ' :' + login : ''}`;
}

// ── Harnais ─────────────────────────────────────────────────────────────────

let passed = 0;
const check = (label, fn) => {
  try { fn(); passed++; console.log(`  ok    ${label}`); }
  catch (e) { console.log(`  ECHEC ${label}\n        ${e.message}`); process.exitCode = 1; }
};

// ── Réception d'un message ──────────────────────────────────────────────────

check('un PRIVMSG s’affiche et conserve les identifiants Twitch', () => {
  const { api, chat } = mountChat();
  api.handleTwitchLine(privmsg({ id: 'abc', userId: '42', login: 'ronni', name: 'Ronni' }));

  const messages = api.messages();
  assert.strictEqual(messages.length, 1);
  assert.strictEqual(messages[0].user, 'Ronni');
  // Sans ces trois champs, aucune suppression n'est possible : c'est le prérequis
  // de toute la modération.
  assert.strictEqual(messages[0].msgId, 'abc');
  assert.strictEqual(messages[0].userId, '42');
  assert.strictEqual(messages[0].login, 'ronni');
  assert.strictEqual(chat.children.length, 1);
});

check('le texte du message arrive intact dans le DOM', () => {
  const { api, chat } = mountChat();
  api.handleTwitchLine(privmsg({ text: 'bonjour à tous' }));
  assert.ok(chat.textContent.includes('bonjour à tous'), chat.textContent);
});

check('un pseudo localisé garde son login distinct du nom affiché', () => {
  // C'est la raison d'être du champ `login` : CLEARCHAT ne donne que le login, et
  // un `display-name` ne permettrait pas de retrouver l'utilisateur.
  const { api } = mountChat();
  api.handleTwitchLine(privmsg({ login: 'kenji', name: '健二' }));
  assert.strictEqual(api.messages()[0].user, '健二');
  assert.strictEqual(api.messages()[0].login, 'kenji');
});

// ── CLEARMSG : suppression d'un message ─────────────────────────────────────

check('CLEARMSG retire le message visé et laisse les autres', () => {
  const { api, runTimers, chat } = mountChat();
  api.handleTwitchLine(privmsg({ id: 'garde', text: 'innocent' }));
  api.handleTwitchLine(privmsg({ id: 'vise', text: 'insulte' }));
  assert.strictEqual(api.messages().length, 2);

  api.handleTwitchLine(clearmsg('vise'));
  // Le retrait passe par le fondu de 0,5 s ; l'expiration de 30 s ne doit pas
  // tourner, sinon tout partirait et le test ne prouverait rien.
  runTimers(1000);

  const remaining = api.messages();
  assert.strictEqual(remaining.length, 1);
  assert.strictEqual(remaining[0].msgId, 'garde');
  assert.strictEqual(chat.children.length, 1);
});

check('une trame ne contenant qu’un CLEARMSG est bien traitée', () => {
  // Régression : l'ancien dispatcher ne découpait la trame en lignes que si elle
  // contenait « PRIVMSG », donc un CLEARMSG seul n'était même pas examiné.
  const { api, runTimers } = mountChat();
  api.handleTwitchLine(privmsg({ id: 'vise' }));

  const frame = clearmsg('vise') + '\r\n';
  frame.split('\r\n').forEach(api.handleTwitchLine);
  runTimers(1000);
  assert.strictEqual(api.messages().length, 0);
});

check('un target-msg-id inconnu ne fait rien', () => {
  const { api, runTimers } = mountChat();
  api.handleTwitchLine(privmsg({ id: 'garde' }));
  api.handleTwitchLine(clearmsg('jamais-vu'));
  runTimers(1000);
  assert.strictEqual(api.messages().length, 1);
});

check('un CLEARMSG sans target-msg-id ne purge rien', () => {
  const { api, runTimers } = mountChat();
  api.handleTwitchLine(privmsg({ id: 'garde' }));
  api.handleTwitchLine('@login=ronni;room-id=1 :tmi.twitch.tv CLEARMSG #testchannel :texte');
  runTimers(1000);
  assert.strictEqual(api.messages().length, 1);
});

// ── CLEARCHAT : timeout, ban, vidage ────────────────────────────────────────

check('CLEARCHAT purge tous les messages de l’utilisateur visé', () => {
  const { api, runTimers } = mountChat();
  api.handleTwitchLine(privmsg({ id: 'a', userId: '42', login: 'ronni' }));
  api.handleTwitchLine(privmsg({ id: 'b', userId: '42', login: 'ronni' }));
  api.handleTwitchLine(privmsg({ id: 'c', userId: '99', login: 'autre' }));

  api.handleTwitchLine(clearchat('42', 'ronni'));
  runTimers(1000);

  const remaining = api.messages();
  assert.strictEqual(remaining.length, 1);
  assert.strictEqual(remaining[0].userId, '99');
});

check('CLEARCHAT retombe sur le login quand l’user-id manque', () => {
  // Un message reçu sans tag `user-id` doit rester purgeable : le login du
  // paramètre final est le repli.
  const { api, runTimers } = mountChat();
  api.handleTwitchLine(
    '@badges=;display-name=Ronni;emotes=;id=a :ronni!ronni@ronni.tmi.twitch.tv PRIVMSG #testchannel :coucou'
  );
  assert.strictEqual(api.messages()[0].userId, '');

  api.handleTwitchLine(clearchat('42', 'ronni'));
  runTimers(1000);
  assert.strictEqual(api.messages().length, 0);
});

check('CLEARCHAT sans cible vide tout le chat Twitch', () => {
  const { api, runTimers } = mountChat();
  api.handleTwitchLine(privmsg({ id: 'a', userId: '1' }));
  api.handleTwitchLine(privmsg({ id: 'b', userId: '2' }));

  api.handleTwitchLine(clearchat(null, null));
  runTimers(1000);
  assert.strictEqual(api.messages().length, 0);
});

check('un message YouTube survit à un CLEARCHAT Twitch', () => {
  // Un modérateur Twitch n'a pas autorité sur le chat YouTube, et un message
  // YouTube ne porte de toute façon ni msgId ni userId.
  const { api, runTimers } = mountChat();
  api.addYouTubeMessage({ user: 'Spectateur', text: 'salut' });
  api.handleTwitchLine(privmsg({ id: 'a', userId: '1' }));

  api.handleTwitchLine(clearchat(null, null));
  runTimers(1000);

  const remaining = api.messages();
  assert.strictEqual(remaining.length, 1);
  assert.strictEqual(remaining[0].platform, 'youtube');
});

// ── Routage des lignes ──────────────────────────────────────────────────────

check('un message dont le texte contient CLEARCHAT s’affiche au lieu de purger', () => {
  // Le routage teste PRIVMSG en premier : sinon n'importe qui viderait l'overlay
  // en tapant le mot dans le chat.
  const { api, runTimers } = mountChat();
  api.handleTwitchLine(privmsg({ id: 'a', text: 'regarde ce CLEARCHAT rigolo' }));
  runTimers(1000);
  assert.strictEqual(api.messages().length, 1);
});

check('un PING dans un message n’emporte pas la trame', () => {
  // Régression : le PING était testé sur la trame entière, donc un « PING » tapé
  // dans le chat faisait perdre tous les messages qui l'accompagnaient.
  const { api } = mountChat();
  const sent = [];
  api.setWs({ send: (v) => sent.push(v) });

  api.handleTwitchLine(privmsg({ id: 'a', text: 'PING les amis' }));
  assert.strictEqual(api.messages().length, 1);
  assert.deepStrictEqual(sent, [], 'un PONG a été émis pour un message de chat');
});

check('un vrai PING serveur reçoit un PONG', () => {
  const { api } = mountChat();
  const sent = [];
  api.setWs({ send: (v) => sent.push(v) });

  api.handleTwitchLine('PING :tmi.twitch.tv');
  assert.deepStrictEqual(sent, ['PONG :tmi.twitch.tv']);
  assert.strictEqual(api.messages().length, 0);
});

check('les lignes de service sont ignorées sans lever', () => {
  const { api } = mountChat();
  [
    '',
    ':tmi.twitch.tv 001 justinfan123 :Welcome, GLHF!',
    ':justinfan123!justinfan123@justinfan123.tmi.twitch.tv JOIN #testchannel',
    '@msg-id=slow_off :tmi.twitch.tv NOTICE #testchannel :Slow mode disabled.',
  ].forEach(api.handleTwitchLine);
  assert.strictEqual(api.messages().length, 0);
});

console.log(`\n  ${passed} assertions de chat OK`);
