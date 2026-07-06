function connectMusicWS(port, onUpdate) {
    const wsUrl = `ws://localhost:${port}/api/current_music_ws`;
    let ws = null;
    let reconnectTimer = null;

    function connect() {
        try {
            ws = new WebSocket(wsUrl);
        } catch (e) {
            reconnectTimer = setTimeout(connect, 5000);
            return;
        }

        ws.addEventListener("message", (evt) => {
            try {
                const data = JSON.parse(evt.data);
                onUpdate(data.current_music || null);
            } catch (e) {}
        });

        ws.addEventListener("close", () => {
            reconnectTimer = setTimeout(connect, 5000);
        });

        ws.addEventListener("error", () => {
            try { ws?.close(); } catch (e) {}
        });
    }

    connect();

    return {
        close() {
            if (reconnectTimer) clearTimeout(reconnectTimer);
            try { ws?.close(); } catch (e) {}
        }
    };
}

// Flux d'état du limiteur OBS. Contrairement à connectMusicWS (serveur musique
// sur MUSIC_PORT), le limiteur est servi par praetorcast-core lui-même : on se
// connecte donc en same-origin. onUpdate reçoit l'objet complet {enabled, threshold}.
function connectObsLimiterWS(onUpdate) {
    const scheme = location.protocol === "https:" ? "wss" : "ws";
    const wsUrl = `${scheme}://${location.host}/api/obs/limiter_ws`;
    let ws = null;
    let reconnectTimer = null;

    function connect() {
        try {
            ws = new WebSocket(wsUrl);
        } catch (e) {
            reconnectTimer = setTimeout(connect, 5000);
            return;
        }

        ws.addEventListener("message", (evt) => {
            try {
                onUpdate(JSON.parse(evt.data));
            } catch (e) {}
        });

        ws.addEventListener("close", () => {
            reconnectTimer = setTimeout(connect, 5000);
        });

        ws.addEventListener("error", () => {
            try { ws?.close(); } catch (e) {}
        });
    }

    connect();

    return {
        close() {
            if (reconnectTimer) clearTimeout(reconnectTimer);
            try { ws?.close(); } catch (e) {}
        }
    };
}
