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

const wss = new WebSocket.Server({ port: PORT });

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

function startChat() {
    console.log('Recherche d\'un stream YouTube en cours...');

    const liveChat = new LiveChat({ channelId: CHANNEL_ID });

    liveChat.on('start', (liveId) => {
        console.log(`Chat connecté au live ID: ${liveId}`);
    });

    liveChat.on('chat', (chatItem) => {
        const messageText = chatItem.message.map(part => {
            if (part.url) {
                return `<img src="${part.url}" alt="${part.text}" style="height: 1.5em; vertical-align: middle;" />`;
            }
            return part.text;
        }).join('');

        console.log(`${chatItem.author.name}: ${messageText}`);

        broadcast(JSON.stringify({
            user: chatItem.author.name,
            text: messageText,
            platform: 'youtube'
        }));
    });

    liveChat.on('end', () => {
        console.log(`Stream terminé. Nouvelle tentative dans ${RETRY_DELAY_MS / 1000}s...`);
        setTimeout(startChat, RETRY_DELAY_MS);
    });

    liveChat.on('error', (err) => {
        if (err.message && err.message.includes('Live Stream was not found')) {
            console.log(`Aucun stream en cours. Nouvelle tentative dans ${RETRY_DELAY_MS / 1000}s...`);
        } else {
            console.error('Erreur du chat:', err.message ?? err);
        }
        setTimeout(startChat, RETRY_DELAY_MS);
    });

    liveChat.start();
}

console.log(`WebSocket serveur démarré sur ws://localhost:${PORT}`);
startChat();
