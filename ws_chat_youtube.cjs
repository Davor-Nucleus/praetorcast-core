/**
 * WebSocket server relaying YouTube live chat to clients in real time.
 * Reads configuration from `env.json`.
 * Retries automatically when no live stream is found.
 */
const { LiveChat } = require('youtube-chat');
const WebSocket = require('ws');
const fs = require('fs');

const env = JSON.parse(fs.readFileSync('./env.json', 'utf-8'));

const PORT = env.PORT_WS_YOUTUBE_CHAT;
const CHANNEL_ID = env.YOUTUBE_CHANNEL_ID;
const RETRY_DELAY_MS = env.YOUTUBE_CHAT_RETRY_DELAY_MS ?? 60_000;

// `host` explicite : sans lui, `ws` écoute sur toutes les interfaces et diffuse le chat
// du live à quiconque partage le réseau. Les serveurs Rust de la pile se limitent
// déjà à 127.0.0.1.
const wss = new WebSocket.Server({ port: PORT, host: '127.0.0.1' });

wss.on('connection', () => {
    console.log('Nouveau client WebSocket connecté');
});

function broadcast(message) {
    wss.clients.forEach(client => {
        if (client.readyState === WebSocket.OPEN) {
            client.send(message);
        }
    });
}

// Instance courante et retry en vol.
//
// `youtube-chat` émet `error` depuis un setInterval d'une seconde qu'il ne coupe pas
// lui-même (live-chat.js, `#execute`). L'ancien code planifiait un `startChat()` à
// chaque `error` sans jamais arrêter l'instance fautive : une panne durable survenant
// *après* connexion à un live produisait ~60 erreurs par minute, donc 60 nouvelles
// instances, chacune avec son propre poller qui échouait à son tour — emballement en
// cascade de la mémoire, du CPU et des requêtes sortantes vers YouTube.
//
// `current` rend inertes les handlers d'une instance sortante (ils continuent d'être
// appelés le temps que la boucle interne s'arrête) ; `retryTimer` garantit qu'un seul
// redémarrage est en attente à la fois.
let current    = null;
let retryTimer = null;

function scheduleRetry(reason) {
    if (retryTimer) return;

    const stale = current;
    current = null;   // à faire avant stop() : stop() émet `end`, dont le handler doit être inerte
    stale?.stop();    // coupe le setInterval interne de youtube-chat

    console.log(`${reason} Nouvelle tentative dans ${RETRY_DELAY_MS / 1000}s...`);
    retryTimer = setTimeout(() => { retryTimer = null; startChat(); }, RETRY_DELAY_MS);
}

function startChat() {
    console.log('Recherche d\'un stream YouTube en cours...');

    const liveChat = new LiveChat({ channelId: CHANNEL_ID });
    current = liveChat;

    liveChat.on('start', (liveId) => {
        console.log(`Chat connecté au live ID: ${liveId}`);
    });

    liveChat.on('chat', (chatItem) => {
        if (liveChat !== current) return;
        // On envoie des segments structurés, jamais de HTML : c'est le client qui
        // construit le DOM. Interpoler `part.text` dans une balise permettait à
        // n'importe quel spectateur d'injecter du script dans l'overlay.
        const parts = chatItem.message.map(part =>
            part.url
                ? { type: 'image', url: part.url, alt: part.text ?? '' }
                : { type: 'text', text: part.text ?? '' }
        );

        console.log(`${chatItem.author.name}: ${parts.map(p => p.type === 'image' ? (p.alt || '[emoji]') : p.text).join('')}`);

        broadcast(JSON.stringify({
            user: chatItem.author.name,
            parts,
            platform: 'youtube'
        }));
    });

    liveChat.on('end', () => {
        if (liveChat !== current) return;   // arrêt provoqué par scheduleRetry : déjà traité
        scheduleRetry('Stream terminé.');
    });

    liveChat.on('error', (err) => {
        if (liveChat !== current) return;
        if (err.message && err.message.includes('Live Stream was not found')) {
            scheduleRetry('Aucun stream en cours.');
        } else {
            console.error('Erreur du chat:', err.message ?? err);
            scheduleRetry('Erreur du chat.');
        }
    });

    liveChat.start();
}

console.log(`WebSocket serveur démarré sur ws://localhost:${PORT}`);
startChat();
