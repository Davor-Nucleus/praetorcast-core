/**
 * WebSocket server relaying YouTube live chat to clients in real time.
 * Reads configuration from `env.json`.
 */
const { LiveChat } = require('youtube-chat');
const WebSocket = require('ws');
const fs = require('fs');

// Chargement du fichier de config env.json
const env = JSON.parse(fs.readFileSync('./env.json', 'utf-8'));

// Récupération des paramètres
const PORT = env.PORT_WS_YOUTUBE_CHAT;
const CHANNEL_ID = env.YOUTUBE_CHANNEL_ID;

// Création du WebSocket Server
const wss = new WebSocket.Server({ port: PORT });

wss.on('connection', ws => {
    console.log('🔌 Nouveau client WebSocket connecté');
});

/**
 * Broadcasts a message to all connected WebSocket clients.
 * @param {string} message
 * @returns {void}
 */
function broadcast(message) {
    wss.clients.forEach(client => {
        if (client.readyState === WebSocket.OPEN) {
            client.send(message); // Envoi direct du message formaté
        }
    });
}

// Connexion au live chat
const liveChat = new LiveChat({ channelId: CHANNEL_ID });

liveChat.on('start', (liveId) => {
    console.log(`✅ Lecture du chat commencée pour le live ID: ${liveId}`);
});

liveChat.on('chat', (chatItem) => {
    // Combine les parties du message
    const messageText = chatItem.message.map(part => {
        if (part.url) {
            return `<img src="${part.url}" alt="${part.text}" style="height: 1.5em; vertical-align: middle;" />`;
        }
        return part.text;
    }).join('');

    // Formatage du message dans le format souhaité (pour logs console)
    const formattedMessage = `${chatItem.author.name}: ${messageText}`;
    console.log(formattedMessage);

    // Diffusion à tous les clients WebSocket (Format JSON pour le frontend)
    const payload = JSON.stringify({
        user: chatItem.author.name,
        text: messageText,
        platform: 'youtube'
    });

    broadcast(payload);
});

liveChat.on('error', (err) => {
    console.error('❌ Erreur du chat:', err);
});

liveChat.start();

console.log(`🚀 WebSocket serveur démarré sur ws://localhost:${PORT}`);
