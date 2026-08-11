// Shared globals expected from the template:
//   TWITCH_CHANNEL_NAME
//   PORT_WS_YOUTUBE_CHAT
//   MAX_MESSAGES, messages, messageTimeouts
//   ws, ytWs, pingInterval, youtubeConnected
//   globalBadges, channelBadges
//
// Règle de sécurité de ce fichier : le contenu d'un message de chat est fourni par
// un tiers. Il n'est JAMAIS concaténé dans du HTML — tout passe par des nœuds DOM
// (`textContent` pour le texte, propriétés pour les attributs). Aucun `innerHTML`.

const DEFAULT_TWITCH_COLOR = "#9146FF";
const COLOR_PATTERN = /^#[0-9a-f]{6}$/i;

function parseTags(rawTags) {
    const tags = {};
    rawTags.substring(rawTags.startsWith("@") ? 1 : 0).split(";").forEach(part => {
        const [key, value] = part.split("=");
        tags[key] = value;
    });
    return tags;
}

/// Tags d'une ligne IRC : de l'`@` jusqu'à la première espace.
///
/// Twitch échappe les espaces dans les valeurs de tags (`\s`), donc la première
/// espace marque bien la fin du bloc.
function tagsOf(line) {
    if (!line.startsWith("@")) return {};
    const end = line.indexOf(" ");
    return end === -1 ? {} : parseTags(line.substring(0, end));
}

/// Paramètre final d'une ligne IRC : ce qui suit le « : » placé après la commande.
///
/// Pour un CLEARCHAT c'est le *login* de l'utilisateur visé, et son absence
/// distingue un vidage complet du chat d'un timeout individuel.
function trailingOf(line) {
    const start = line.startsWith("@") ? line.indexOf(" ") + 1 : 0;
    const colon = line.indexOf(" :", start);
    return colon === -1 ? "" : line.substring(colon + 2);
}

/// Une couleur non conforme irait se poser telle quelle dans un attribut de style.
function safeColor(value) {
    return COLOR_PATTERN.test(value || "") ? value : DEFAULT_TWITCH_COLOR;
}

function makeImage(className, url, alt) {
    const img = document.createElement("img");
    img.className = className;
    img.src = url;
    img.alt = alt || "";
    return img;
}

/// Renvoie la liste des badges à afficher, sous forme de données (pas de HTML).
function parseBadges(badgesTag) {
    if (!badgesTag) return [];
    return badgesTag.split(",").map(badge => {
        const [id, version] = badge.split("/");
        const key = `${id}/${version}`;
        const url = channelBadges[key] || globalBadges[key];
        return url ? { url, alt: id } : null;
    }).filter(Boolean);
}

/// Découpe le message en segments texte / emote.
/// Les positions d'emotes de Twitch sont exprimées en POINTS DE CODE : on découpe
/// donc sur `Array.from` et non par `substring`, qui dérive dès qu'un emoji
/// (paire de substitution) apparaît avant une emote.
function parseEmotes(message, emotesTag) {
    if (!emotesTag) return [{ type: "text", value: message }];

    const chars = Array.from(message);
    const ranges = [];

    emotesTag.split("/").forEach(group => {
        const [id, positions] = group.split(":");
        if (!positions) return;
        positions.split(",").forEach(pos => {
            const [start, end] = pos.split("-").map(Number);
            if (Number.isInteger(start) && Number.isInteger(end) && start >= 0 && end >= start) {
                ranges.push({ start, end, id });
            }
        });
    });

    ranges.sort((a, b) => a.start - b.start);

    const segments = [];
    let cursor = 0;

    ranges.forEach(range => {
        if (range.start < cursor) return; // plages qui se chevauchent : on ignore
        if (range.start > cursor) {
            segments.push({ type: "text", value: chars.slice(cursor, range.start).join("") });
        }
        segments.push({
            type: "emote",
            url: `https://static-cdn.jtvnw.net/emoticons/v2/${encodeURIComponent(range.id)}/default/dark/1.0`,
            alt: range.id
        });
        cursor = range.end + 1;
    });

    if (cursor < chars.length) {
        segments.push({ type: "text", value: chars.slice(cursor).join("") });
    }

    return segments;
}

/// Construit l'élément d'un message. Seul point de rendu : rien d'autre ne doit
/// fabriquer de balises pour le chat.
function buildMessageElement(id, badges, username, color, segments) {
    const wrapper = document.createElement("div");
    wrapper.className = "message";
    wrapper.id = `msg-${id}`;

    const meta = document.createElement("div");
    meta.className = "meta";
    badges.forEach(badge => meta.appendChild(makeImage("badge", badge.url, badge.alt)));

    const name = document.createElement("span");
    name.className = "username";
    name.style.color = safeColor(color);
    name.textContent = username;
    meta.appendChild(name);

    const content = document.createElement("span");
    content.className = "message-content";
    segments.forEach(segment => {
        if (segment.type === "emote") {
            content.appendChild(makeImage("emote", segment.url, segment.alt));
        } else {
            content.appendChild(document.createTextNode(segment.value));
        }
    });

    wrapper.appendChild(meta);
    wrapper.appendChild(content);
    return wrapper;
}

async function loadBadges() {
    // Les appels Helix sont faits côté serveur : le token Twitch ne descend jamais
    // dans la page (voir twitch_controller::badges).
    try {
        const response = await fetch("/api/twitch/badges");
        if (!response.ok) return;
        const data = await response.json();
        globalBadges = data.global || {};
        channelBadges = data.channel || {};
    } catch (e) {
        console.error("Error loading badges", e);
    }
}

function parseTwitchMessage(line) {
    const splitIdx = line.indexOf(" :");
    if (splitIdx === -1) return;

    const tags = tagsOf(line);
    const rest = line.substring(splitIdx + 2);

    const msgContentIdx = rest.indexOf("PRIVMSG");
    if (msgContentIdx === -1) return;

    // Le préfixe `nick!nick@nick.tmi.twitch.tv` porte le *login*, seul identifiant que
    // CLEARCHAT donne en clair. Le `display-name` ne peut pas le remplacer : pour un
    // pseudo localisé (CJK, accents) les deux n'ont rien à voir.
    const prefix = rest.substring(0, msgContentIdx);
    const bang = prefix.indexOf("!");
    const login = bang > 0 ? prefix.substring(0, bang) : "";

    const afterPrivMsg = rest.substring(msgContentIdx);
    const colonIdx = afterPrivMsg.indexOf(":");
    if (colonIdx === -1) return;

    addTwitchMessage(tags, tags["display-name"] || "User", afterPrivMsg.substring(colonIdx + 1), login);
}

function addTwitchMessage(tags, username, rawMessage, login) {
    const id = crypto.randomUUID();
    addMessage({
        id,
        platform: "twitch",
        user: username,
        // Identifiants Twitch conservés pour la modération : `msgId` est la cible d'un
        // CLEARMSG, `userId` celle d'un CLEARCHAT. L'`id` local reste la clé du DOM.
        msgId: tags["id"] || "",
        userId: tags["user-id"] || "",
        login: login || "",
        node: buildMessageElement(
            id,
            parseBadges(tags["badges"]),
            username,
            tags["color"],
            parseEmotes(rawMessage, tags["emotes"])
        ),
        timestamp: Date.now()
    });
}

// --- Modération ------------------------------------------------------------
// Twitch pousse déjà CLEARMSG et CLEARCHAT sur cette socket anonyme : ils sont
// couverts par le `twitch.tv/commands` du CAP REQ. Sans ces gestionnaires, un
// message supprimé par un modérateur restait affiché à l'écran du stream.

/// CLEARMSG — un message précis a été supprimé.
///
/// `@login=x;target-msg-id=<uuid> :tmi.twitch.tv CLEARMSG #chan :texte`
function purgeMessage(line) {
    const target = tagsOf(line)["target-msg-id"];
    if (!target) return;
    // Un identifiant qu'on n'a pas (message déjà expiré, ou reçu avant l'ouverture
    // de l'overlay) est un non-événement, pas une erreur.
    messages
        .filter(msg => msg.platform === "twitch" && msg.msgId === target)
        .forEach(msg => removeMessage(msg.id));
}

/// CLEARCHAT — timeout, bannissement, ou vidage complet du chat.
///
/// `@target-user-id=123 :tmi.twitch.tv CLEARCHAT #chan :login` cible un utilisateur ;
/// sans paramètre final, c'est tout le chat qui est vidé.
function purgeUser(line) {
    const targetId = tagsOf(line)["target-user-id"];
    const login = trailingOf(line);

    messages
        .filter(msg => {
            // Un modérateur Twitch n'a pas autorité sur le chat YouTube, et un
            // message YouTube ne porte de toute façon aucun de ces identifiants.
            if (msg.platform !== "twitch") return false;
            if (!targetId && !login) return true;
            // `user-id` d'abord : c'est stable, là où un login peut être changé.
            if (targetId && msg.userId) return msg.userId === targetId;
            return login !== "" && msg.login === login;
        })
        .forEach(msg => removeMessage(msg.id));
}

/// JoyPixels : on convertit les raccourcis en caractères Unicode (texte) et non en
/// balises `<img>`, pour que le résultat puisse rester dans un nœud texte.
function shortnamesToText(text) {
    if (window.joypixels && typeof window.joypixels.shortnameToUnicode === "function") {
        try {
            return window.joypixels.shortnameToUnicode(text);
        } catch (e) {
            console.error("JoyPixels Error:", e);
        }
    }
    return text;
}

/// Le pont YouTube envoie des segments structurés (`parts`). L'ancien format à plat
/// (`text`) reste accepté au cas où un pont non mis à jour tourne encore.
function youtubeSegments(msgData) {
    if (Array.isArray(msgData.parts)) {
        return msgData.parts.map(part => {
            if (part && part.type === "image" && part.url) {
                return { type: "emote", url: part.url, alt: part.alt || "" };
            }
            return { type: "text", value: shortnamesToText(String((part && part.text) || "")) };
        });
    }
    return [{ type: "text", value: shortnamesToText(String(msgData.text || "")) }];
}

function addYouTubeMessage(msgData) {
    const id = crypto.randomUUID();
    const username = String(msgData.user || "");
    const color = "#" + Math.floor(Math.random() * 16777216).toString(16).padStart(6, "0");

    addMessage({
        id,
        platform: "youtube",
        user: username,
        node: buildMessageElement(id, [], `${username} (YT)`, color, youtubeSegments(msgData)),
        timestamp: Date.now()
    });
}

function addMessage(msg) {
    messages = [msg, ...messages].slice(0, MAX_MESSAGES);
    renderMessages();

    const timeout = setTimeout(() => removeMessage(msg.id), 30000);
    messageTimeouts.add(timeout);
}

function removeMessage(id) {
    const msgElement = document.getElementById(`msg-${id}`);
    if (msgElement) {
        msgElement.classList.add("fade-out");
        setTimeout(() => {
            messages = messages.filter(m => m.id !== id);
            renderMessages();
        }, 500);
    } else {
        messages = messages.filter(m => m.id !== id);
        renderMessages();
    }
}

function renderMessages() {
    const chat = document.getElementById("chat");
    if (!chat) return;
    chat.replaceChildren(...messages.map(msg => msg.node));
}

/// Route une ligne IRC vers son gestionnaire.
///
/// Les commandes sont testées entourées d'espaces et `PRIVMSG` passe en premier : un
/// message dont le *texte* contient « CLEARCHAT » doit s'afficher, pas vider le chat.
function handleTwitchLine(line) {
    if (!line) return;
    if (line.startsWith("PING")) {
        ws.send("PONG :tmi.twitch.tv");
        return;
    }
    if (line.includes(" PRIVMSG ")) {
        parseTwitchMessage(line);
        return;
    }
    if (line.includes(" CLEARMSG ")) {
        purgeMessage(line);
        return;
    }
    if (line.includes(" CLEARCHAT ")) {
        purgeUser(line);
    }
}

async function connectTwitch() {
    const channel = TWITCH_CHANNEL_NAME;
    if (!channel) return;

    await loadBadges();

    ws = new WebSocket("wss://irc-ws.chat.twitch.tv:443");

    ws.onopen = () => {
        console.log("Twitch WS Connected");
        ws.send("CAP REQ :twitch.tv/tags twitch.tv/commands twitch.tv/membership");
        // Connexion anonyme en lecture seule : les tags (emotes, badges,
        // display-name, color) sont servis sans authentification, et aucun jeton
        // n'a besoin d'exister dans la page.
        ws.send("PASS SCHMOOPIIE");
        ws.send("NICK justinfan123");
        ws.send(`JOIN #${channel.toLowerCase()}`);
        pingInterval = setInterval(() => ws.send("PING"), 60000);
    };

    // Une trame peut contenir plusieurs lignes : on découpe systématiquement, puis on
    // route ligne par ligne. L'ancien filtre portait sur la trame entière et ne la
    // découpait que si elle contenait « PRIVMSG » — une trame ne portant qu'un
    // CLEARMSG n'était donc même pas examinée. Le PING est aussi traité par ligne :
    // testé sur la trame, un simple « PING » tapé dans le chat faisait perdre tous
    // les messages qui l'accompagnaient.
    ws.onmessage = (event) => {
        String(event.data).split("\r\n").forEach(handleTwitchLine);
    };

    ws.onclose = () => {
        console.log("Twitch WS Closed, retrying...");
        clearInterval(pingInterval);
        setTimeout(connectTwitch, 3000);
    };
}

function connectYouTubeSSE() {
    console.log(`Connecting to YouTube WebSocket on port ${PORT_WS_YOUTUBE_CHAT}...`);

    ytWs = new WebSocket(`ws://localhost:${PORT_WS_YOUTUBE_CHAT}`);

    ytWs.onopen = () => {
        console.log("YouTube WebSocket Connected");
        youtubeConnected = true;
    };

    ytWs.onmessage = (event) => {
        try {
            const data = JSON.parse(event.data);
            if (data.platform === "youtube" && data.user) addYouTubeMessage(data);
        } catch (e) {
            console.error("Error parsing YouTube WS message", e);
        }
    };

    ytWs.onerror = (err) => {
        console.error("YouTube WebSocket Error:", err);
        youtubeConnected = false;
    };

    ytWs.onclose = () => {
        console.log("YouTube WebSocket Closed, retrying...");
        youtubeConnected = false;
        setTimeout(connectYouTubeSSE, 5000);
    };
}
