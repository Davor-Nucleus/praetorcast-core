/**
 * Recharge `/theme.css` dès que le thème change.
 *
 * Une Browser Source OBS ne se rafraîchit pas toute seule : sans ce petit client,
 * chaque essai de couleur imposerait d'aller cliquer « Actualiser » sur chaque
 * source dans OBS. Le serveur n'envoie qu'un signal — c'est le navigateur qui
 * rejoue la feuille, ce qui évite d'injecter du CSS reçu par WebSocket.
 */
(function () {
    const LINK_ID = "pc-theme";
    const RETRY_MS = 3000;

    function reloadStylesheet() {
        const link = document.getElementById(LINK_ID);
        if (!link) return;
        // Le cache-buster est indispensable : `Cache-Control: no-store` empêche la
        // mise en cache, mais réassigner un href identique ne relance aucune requête.
        link.setAttribute("href", "/theme.css?v=" + Date.now());
    }

    function connect() {
        const socket = new WebSocket("ws://" + location.host + "/api/theme_ws");
        socket.onmessage = reloadStylesheet;
        socket.onerror = () => socket.close();
        socket.onclose = () => setTimeout(connect, RETRY_MS);
    }

    connect();
})();
