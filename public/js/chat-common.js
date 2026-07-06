// Shared globals expected from the template:
//   TWITCH_CHANNEL_NAME, TWITCH_OAUTH_TOKEN, TWITCH_CLIENT_ID
//   PORT_WS_YOUTUBE_CHAT
//   MAX_MESSAGES, messages, messageTimeouts
//   ws, ytWs, pingInterval, youtubeConnected
//   globalBadges, channelBadges

function parseTags(rawTags) {
    const tags = {};
    rawTags.substring(rawTags.startsWith("@") ? 1 : 0).split(";").forEach(part => {
        const [key, value] = part.split("=");
        tags[key] = value;
    });
    return tags;
}

function parseBadges(badgesTag) {
    if (!badgesTag) return "";
    return badgesTag.split(",").map(badge => {
        const [id, version] = badge.split("/");
        const key = `${id}/${version}`;
        const url = channelBadges[key] || globalBadges[key];
        return url ? `<img class="badge" src="${url}" alt="${id}" />` : "";
    }).join("");
}

function parseEmotes(message, emotesTag) {
    if (!emotesTag) return message;

    const replacements = [];
    emotesTag.split("/").forEach(group => {
        const [id, positions] = group.split(":");
        if (!positions) return;
        positions.split(",").forEach(pos => {
            const [start, end] = pos.split("-").map(Number);
            const url = `https://static-cdn.jtvnw.net/emoticons/v2/${id}/default/dark/1.0`;
            const img = `<img class="emote" src="${url}" alt="${id}" />`;
            replacements.push({ start, end, html: img });
        });
    });

    replacements.sort((a, b) => a.start - b.start);

    let result = "";
    let lastIndex = 0;

    replacements.forEach(r => {
        if (r.start > lastIndex) result += message.substring(lastIndex, r.start);
        result += r.html;
        lastIndex = r.end + 1;
    });

    if (lastIndex < message.length) result += message.substring(lastIndex);

    return result;
}

async function loadBadges(token, clientId, channelName) {
    try {
        const authHeaders = {
            Authorization: `Bearer ${token.replace("oauth:", "")}`,
            "Client-Id": clientId
        };

        const globalRes = await fetch("https://api.twitch.tv/helix/chat/badges/global", { headers: authHeaders });
        if (globalRes.ok) {
            const data = await globalRes.json();
            data.data.forEach(badge => {
                badge.versions.forEach(v => {
                    globalBadges[`${badge.set_id}/${v.id}`] = v.image_url_1x;
                });
            });
        }

        const userRes = await fetch(`https://api.twitch.tv/helix/users?login=${channelName}`, { headers: authHeaders });
        const userData = await userRes.json();
        if (userData.data && userData.data.length > 0) {
            const broadcasterId = userData.data[0].id;
            const chanRes = await fetch(
                `https://api.twitch.tv/helix/chat/badges?broadcaster_id=${broadcasterId}`,
                { headers: authHeaders }
            );
            if (chanRes.ok) {
                const cData = await chanRes.json();
                cData.data.forEach(badge => {
                    badge.versions.forEach(v => {
                        channelBadges[`${badge.set_id}/${v.id}`] = v.image_url_1x;
                    });
                });
            }
        }
    } catch (e) {
        console.error("Error loading badges", e);
    }
}

function parseTwitchMessage(line) {
    const splitIdx = line.indexOf(" :");
    if (splitIdx === -1) return;

    const tags = parseTags(line.substring(0, splitIdx));
    const rest = line.substring(splitIdx + 2);

    const msgContentIdx = rest.indexOf("PRIVMSG");
    if (msgContentIdx === -1) return;

    const afterPrivMsg = rest.substring(msgContentIdx);
    const colonIdx = afterPrivMsg.indexOf(":");
    if (colonIdx === -1) return;

    addTwitchMessage(tags, tags["display-name"] || "User", afterPrivMsg.substring(colonIdx + 1));
}

function addTwitchMessage(tags, username, rawMessage) {
    const badgesHtml = parseBadges(tags["badges"]);
    const contentHtml = parseEmotes(rawMessage, tags["emotes"]);
    const color = tags["color"] || "#9146FF";

    const fullHtml = `
        <div class="meta">
            ${badgesHtml}
            <span class="username" style="color: ${color}">${username}</span>
        </div>
        <span class="message-content">${contentHtml}</span>
    `;

    addMessage({
        id: crypto.randomUUID(),
        platform: "twitch",
        user: username,
        htmlContent: fullHtml,
        color: color,
        timestamp: Date.now()
    });
}

function addYouTubeMessage(msgData) {
    const username = msgData.user;
    const color = "#" + Math.floor(Math.random() * 16777215).toString(16);

    let processedMessage = msgData.text;
    if (window.joypixels) {
        try {
            processedMessage = window.joypixels.shortnameToImage(processedMessage);
            processedMessage = window.joypixels.unicodeToImage(processedMessage);
        } catch (e) {
            console.error("JoyPixels Error:", e);
        }
    }

    const fullHtml = `
        <div class="meta">
            <span class="username" style="color: ${color}">${username} (YT)</span>
        </div>
        <span class="message-content">${processedMessage}</span>
    `;

    addMessage({
        id: crypto.randomUUID(),
        platform: "youtube",
        user: username,
        htmlContent: fullHtml,
        color: color,
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
    document.getElementById("chat").innerHTML = messages.map(msg => `
        <div class="message" id="msg-${msg.id}">
            ${msg.htmlContent}
        </div>
    `).join("");
}

async function connectTwitch() {
    const channel = TWITCH_CHANNEL_NAME;
    const token = TWITCH_OAUTH_TOKEN;
    const clientId = TWITCH_CLIENT_ID;

    if (!channel) return;

    if (token && clientId) await loadBadges(token, clientId, channel);

    ws = new WebSocket("wss://irc-ws.chat.twitch.tv:443");

    ws.onopen = () => {
        console.log("Twitch WS Connected");
        ws.send("CAP REQ :twitch.tv/tags twitch.tv/commands twitch.tv/membership");
        if (token) {
            ws.send(`PASS oauth:${token.replace("oauth:", "")}`);
            ws.send(`NICK ${channel}`);
        } else {
            ws.send("PASS SCHMOOPIIE");
            ws.send("NICK justinfan123");
        }
        ws.send(`JOIN #${channel.toLowerCase()}`);
        pingInterval = setInterval(() => ws.send("PING"), 60000);
    };

    ws.onmessage = (event) => {
        const data = event.data;
        if (data.includes("PING")) { ws.send("PONG :tmi.twitch.tv"); return; }
        if (data.includes("PRIVMSG")) {
            data.split("\r\n").forEach(line => {
                if (line.includes("PRIVMSG")) parseTwitchMessage(line);
            });
        }
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
            if (data.platform === "youtube" && data.user && data.text) addYouTubeMessage(data);
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
