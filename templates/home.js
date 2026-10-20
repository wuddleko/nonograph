function savedRelayUrls(storage) {
    try {
        const raw = storage.getItem("nostorgraph_relays");
        if (raw == null || raw === "") {
            return [];
        }
        const parsed = JSON.parse(raw);
        if (!parsed || typeof parsed !== "object") {
            return [];
        }
        const pinned = Array.isArray(parsed.pinned) ? parsed.pinned : [];
        const dealt = Array.isArray(parsed.dealt) ? parsed.dealt : [];
        const urls = [];
        for (let index = 0; index < pinned.length; index += 1) {
            if (typeof pinned[index] === "string" && pinned[index]) {
                urls.push(pinned[index]);
            }
        }
        for (let index = 0; index < dealt.length; index += 1) {
            if (typeof dealt[index] === "string" && dealt[index]) {
                urls.push(dealt[index]);
            }
        }
        return urls;
    } catch (error) {
        return [];
    }
}

function paintSavedRelayLists(doc, storage) {
    const urls = savedRelayUrls(storage);
    const roots = [doc.querySelector("#sidebar-relays"), doc.querySelector("#relays")];
    for (let index = 0; index < roots.length; index += 1) {
        const root = roots[index];
        if (!root || root.querySelector(".relay-deal-controls")) {
            continue;
        }
        let list = null;
        const lists = root.querySelectorAll(".relay-list");
        for (let item = 0; item < lists.length; item += 1) {
            if (!lists[item].classList.contains("relay-list-user")) {
                list = lists[item];
                break;
            }
        }
        if (urls.length === 0) {
            if (list) {
                list.remove();
            }
            if (!root.querySelector(".relay-empty")) {
                const message = doc.createElement("p");
                message.className = "relay-empty";
                message.textContent = "No relays.";
                root.append(message);
            }
            continue;
        }
        const empty = root.querySelector(".relay-empty");
        if (empty) {
            empty.remove();
        }
        if (!list) {
            list = doc.createElement("ul");
            list.className = "relay-list";
            const anchor = root.querySelector(".relay-add");
            if (anchor) {
                root.insertBefore(list, anchor);
            } else {
                root.append(list);
            }
        }
        list.replaceChildren();
        for (let item = 0; item < urls.length; item += 1) {
            const row = doc.createElement("li");
            row.dataset.relay = urls[item];
            const name = doc.createElement("span");
            name.className = "relay-name";
            name.textContent = urls[item].replace(/^wss:\/\//, "");
            row.append(name);
            list.append(row);
        }
    }
}

            const editor = document.querySelector('writemark-editor[name="content"]');
            const charCount = document.getElementById("charCount");
            const mobileCharCount = document.getElementById("mobileCharCount");
            const form = document.getElementById("publishForm");

            if (editor && editor.shadowRoot && window.CSS && CSS.supports("field-sizing", "content")) {
                const source = editor.shadowRoot.querySelector("textarea");
                if (source) source.rows = 1;
            }

            let maxFetchRelays = 6;
            let relaysForPublicPublish = () => [];
            let publicRelayUrl = () => false;
            let nostrLoading;

            function hasAnyPublishRelay() {
                return savedRelayUrls(localStorage).length > 0;
            }

            globalThis.nostorgraphPublishPublicNote = async function (fields) {
                const { publishPublicNote } = await loadNostr();
                const timeoutMs = Number(form.dataset.timeout) || 10_000;
                return publishPublicNote({
                    title: fields.title,
                    author: fields.author,
                    content: fields.content,
                    contentMax: Number(form.dataset.sealMax),
                    relays: savedRelayUrls(localStorage),
                    timeoutMs,
                    csrfToken: form.csrf_token ? form.csrf_token.value : "",
                });
            };

            async function loadNostr() {
                if (!nostrLoading) {
                    nostrLoading = (async () => {
                        const nostr = await import(
                            new URL(
                                `./nostr.js${new URL(import.meta.url).search}`,
                                import.meta.url,
                            )
                        );
                        relaysForPublicPublish = nostr.relaysForPublicPublish;
                        publicRelayUrl = nostr.publicRelayUrl;
                        maxFetchRelays = nostr.MAX_FETCH_RELAYS;
                        return nostr;
                    })();
                }
                return nostrLoading;
            }

            // Check if device is mobile
            function isMobile() {
                return window.innerWidth <= 480;
            }

            // Prevent Enter key in title textarea
            const titleTextarea = document.querySelector('textarea[name="title"]');
            if (titleTextarea) {
                titleTextarea.addEventListener("keydown", function (e) {
                    if (e.key === "Enter") {
                        e.preventDefault();
                        this.blur();
                        return false;
                    }
                });

                if (!(window.CSS && CSS.supports("field-sizing", "content"))) {
                    titleTextarea.addEventListener("input", function () {
                        this.style.height = "auto";
                        this.style.height = this.scrollHeight + "px";
                    });
                }
            }

            const contentLimit = Number(editor.getAttribute("maxlength")) || 0;
            const publishButtons = document.querySelectorAll(
                'button[type="submit"][form="publishForm"]',
            );
            const errorEl = form.querySelector(".form-error");
            const circuitByRelay = new Map();
            let circuitsTracked = false;
            let storeReady = false;

            function everyRelayHasPath() {
                if (!circuitsTracked) {
                    return true;
                }
                const relays = relaysForPublicPublish(savedRelayUrls(localStorage));
                return (
                    relays.length > 0 &&
                    relays.every((relay) => {
                        const path = circuitByRelay.get(relayKey(relay));
                        return typeof path === "string" && path.length > 0;
                    })
                );
            }

            function syncNostrButtons() {
                const blocked = !hasAnyPublishRelay();
                const waiting = !blocked && !everyRelayHasPath();
                publishButtons.forEach((button) => {
                    button.classList.toggle("circuits-pending", waiting);
                    button.classList.toggle("no-relays", blocked);
                    button.setAttribute("aria-disabled", blocked || waiting ? "true" : "false");
                });
                syncPublishButtons();
            }

            function relayKey(url) {
                const match = /^wss:\/\/([^/?#]+)(.*)$/i.exec(url);
                if (!match) {
                    return url;
                }
                let host = match[1];
                if (host.endsWith(":443")) {
                    host = host.slice(0, -4);
                }
                host = host.replace(/\.+$/, "").toLowerCase();
                let tail = match[2];
                if (tail === "/") {
                    tail = "";
                } else if (tail.endsWith("/")) {
                    tail = tail.slice(0, -1);
                }
                return "wss://" + host + tail;
            }

            globalThis.paintSavedRelayLists = paintSavedRelayLists;
            globalThis.refreshRelayCircuits = function () {};
            const relaySidebar = document.querySelector("#sidebar-relays");
            if (relaySidebar && typeof globalThis.bindRelayActions === "function") {
                globalThis.bindRelayActions(localStorage, relaySidebar);
            } else {
                paintSavedRelayLists(document, localStorage);
            }
            let relayListSending = false;
            let relayListDirty = false;
            let relayListSend = null;
            const CIRCUIT_POLL_MS = 8000;
            const CIRCUIT_QUIET_MS = 400;
            let lastTypingAt = 0;
            let circuitRefreshGen = 0;
            let circuitPollTimer = 0;
            function noteTyping() {
                lastTypingAt = Date.now();
            }
            function circuitPollPaused() {
                if (document.hidden) {
                    return true;
                }
                return Date.now() - lastTypingAt < CIRCUIT_QUIET_MS;
            }
            function scheduleCircuitPoll() {
                if (circuitPollTimer || document.hidden) {
                    return;
                }
                const delay = Math.max(CIRCUIT_QUIET_MS - (Date.now() - lastTypingAt), 50);
                circuitPollTimer = setTimeout(function () {
                    circuitPollTimer = 0;
                    pollRelayCircuits();
                }, delay);
            }
            function pollRelayCircuits() {
                if (circuitPollPaused()) {
                    scheduleCircuitPoll();
                    return;
                }
                void refreshRelayCircuits();
            }
            async function sendRelayList() {
                relayListDirty = true;
                if (!relayListSend) {
                    relayListSend = deliverRelayList();
                }
                return relayListSend;
            }
            async function deliverRelayList() {
                relayListSending = true;
                try {
                    while (relayListDirty) {
                        relayListDirty = false;
                        const relays = savedRelayUrls(localStorage);
                        try {
                            const response = await fetch("/tor-circuits", {
                                method: "POST",
                                headers: { "Content-Type": "application/json" },
                                body: JSON.stringify({
                                    relays: relays,
                                    csrf_token: form.csrf_token
                                        ? form.csrf_token.value
                                        : "",
                                }),
                                signal: AbortSignal.timeout(4000),
                            });
                            if (!response.ok && !relayListDirty) {
                                break;
                            }
                        } catch (error) {
                            if (!relayListDirty) {
                                break;
                            }
                        }
                    }
                } finally {
                    relayListSending = false;
                    relayListSend = null;
                }
                if (relayListDirty) {
                    return sendRelayList();
                }
            }
            function applyCircuitPayload(payload) {
                circuitsTracked = payload.tracking === true;
                const live = new Set();
                if (circuitsTracked) {
                    for (const [relay, path] of Object.entries(payload.relays || {})) {
                        if (typeof path !== "string" || !path) {
                            continue;
                        }
                        const key = relayKey(relay);
                        live.add(key);
                        circuitByRelay.set(key, path);
                    }
                }
                for (const key of [...circuitByRelay.keys()]) {
                    if (!live.has(key)) {
                        circuitByRelay.delete(key);
                    }
                }
            }
            async function refreshRelayCircuits(options) {
                const force = !!(options && options.force);
                const gen = ++circuitRefreshGen;
                await loadNostr();
                if (gen !== circuitRefreshGen) {
                    return;
                }
                await sendRelayList();
                if (gen !== circuitRefreshGen) {
                    return;
                }
                let payload;
                try {
                    const response = await fetch("/tor-circuits", { cache: "no-store" });
                    if (!response.ok) {
                        return;
                    }
                    payload = await response.json();
                } catch (error) {
                    return;
                }
                if (gen !== circuitRefreshGen) {
                    return;
                }
                if (!force && circuitPollPaused()) {
                    scheduleCircuitPoll();
                    return;
                }
                applyCircuitPayload(payload);
                paintRelayCircuits();
            }
            function relayGuardIcon() {
                const svg = "http://www.w3.org/2000/svg";
                const icon = document.createElementNS(svg, "svg");
                icon.setAttribute("class", "relay-guard-icon");
                icon.setAttribute("viewBox", "0 0 16 16");
                icon.setAttribute("width", "12");
                icon.setAttribute("height", "12");
                icon.setAttribute("aria-hidden", "true");
                icon.setAttribute("focusable", "false");
                const shape = document.createElementNS(svg, "path");
                shape.setAttribute("fill", "none");
                shape.setAttribute("stroke", "currentColor");
                shape.setAttribute("stroke-width", "1.6");
                shape.setAttribute("stroke-linejoin", "round");
                shape.setAttribute(
                    "d",
                    "M8 1.75 13 3.6v4.1c0 3.1-2.1 5.2-5 6.55C5.1 12.9 3 10.8 3 7.7V3.6Z",
                );
                icon.append(shape);
                return icon;
            }

            function circuitToken(path) {
                if (!circuitsTracked) {
                    return "0";
                }
                if (!path) {
                    return "1";
                }
                return "2:" + path;
            }
            function paintRelayCircuits() {
                document.querySelectorAll(".relay-list li[data-relay]").forEach((row) => {
                    const path = circuitsTracked
                        ? circuitByRelay.get(relayKey(row.dataset.relay || ""))
                        : "";
                    const token = circuitToken(path);
                    let line = row.querySelector(".relay-circuit");
                    if (line && line.dataset.circuit === token) {
                        return;
                    }
                    if (!line) {
                        line = document.createElement("div");
                        line.className = "relay-circuit";
                        row.append(line);
                    }
                    line.dataset.circuit = token;
                    line.replaceChildren();
                    if (!circuitsTracked) {
                        line.classList.remove("relay-circuit-wait");
                        return;
                    }
                    if (path) {
                        line.classList.remove("relay-circuit-wait");
                        const guard = document.createElement("span");
                        guard.className = "relay-guard";
                        guard.title = "Entry guard";
                        const label = document.createElement("span");
                        label.className = "relay-guard-label";
                        label.textContent = "guard";
                        guard.append(relayGuardIcon(), label);
                        line.append(guard);
                        for (const name of path.split(" → ")) {
                            if (!name) {
                                continue;
                            }
                            const arrow = document.createElement("span");
                            arrow.className = "relay-circuit-arrow";
                            arrow.setAttribute("aria-hidden", "true");
                            arrow.textContent = "→";
                            const hop = document.createElement("span");
                            hop.className = "relay-circuit-hop";
                            hop.textContent = name;
                            line.append(arrow, hop);
                        }
                        return;
                    }
                    line.classList.add("relay-circuit-wait");
                    const spinner = document.createElement("span");
                    spinner.className = "relay-spinner";
                    spinner.setAttribute("role", "status");
                    spinner.setAttribute("aria-label", "Opening a circuit");
                    line.append(spinner);
                });
                syncNostrButtons();
            }
            globalThis.syncNostrButtons = syncNostrButtons;
            globalThis.refreshRelayCircuits = refreshRelayCircuits;
            refreshRelayCircuits();
            setInterval(pollRelayCircuits, CIRCUIT_POLL_MS);
            document.addEventListener("keydown", noteTyping, true);
            document.addEventListener("beforeinput", noteTyping, true);
            document.addEventListener("compositionstart", noteTyping, true);
            document.addEventListener("compositionupdate", noteTyping, true);
            editor.addEventListener("md-input", noteTyping);
            document.addEventListener("visibilitychange", function () {
                if (!document.hidden) {
                    pollRelayCircuits();
                }
            });

            let publishing = false;

            let noteLocked = false;

            function syncPublishButtons() {
                const overLimit = editor.value.length > contentLimit;
                const blocked = !hasAnyPublishRelay();
                publishButtons.forEach((button) => {
                    button.disabled =
                        noteLocked || !storeReady || publishing || overLimit || blocked;
                });
            }
            syncNostrButtons();

            function setPublishBusy(busy) {
                publishing = busy;
                syncPublishButtons();
            }

            function showPublishError(message) {
                if (errorEl) {
                    errorEl.textContent = message;
                }
            }

            function sealHref(locator, eventId, key, hints) {
                let href = "/" + locator;
                if (eventId) {
                    href += "/" + eventId;
                }
                const query = hints
                    .map((relay) => "r=" + encodeURIComponent(relay))
                    .join("&");
                return href + (query ? "?" + query : "") + "#" + key;
            }

            function longSealHref(result) {
                const hints = Array.isArray(result.hints) ? result.hints : [];
                const eventId = result && result.event && result.event.id;
                return sealHref(result.locator, eventId, result.key, hints);
            }

            function shortSealHref(result) {
                return sealHref(result.locator, null, result.key, []);
            }

            function pageHref(path) {
                if (typeof location === "undefined" || !location.origin) {
                    return path;
                }
                return new URL(path, location.origin).href;
            }

            function pinLongLink(path) {
                if (
                    typeof history === "undefined" ||
                    typeof history.pushState !== "function"
                ) {
                    return;
                }
                try {
                    history.pushState(null, "", path);
                } catch (error) {
                    return;
                }
            }

            async function copyShortLink(result, csrfToken) {
                const event = result && result.event;
                const eventId = event && event.id;
                if (!result || !result.locator || !eventId || !result.claim) {
                    return { ok: false, href: null };
                }
                let response;
                try {
                    response = await fetch("/seal-row", {
                        method: "POST",
                        headers: { "Content-Type": "application/json" },
                        body: JSON.stringify({
                            locator: result.locator,
                            event_id: eventId,
                            relays: Array.isArray(result.accepted) ? result.accepted : [],
                            claim: result.claim,
                            csrf_token: csrfToken || "",
                        }),
                    });
                } catch (error) {
                    return { ok: false, href: null };
                }
                if (!response || !response.ok) {
                    return { ok: false, href: null };
                }
                const href = pageHref(shortSealHref(result));
                paintShareUrl(
                    document.getElementById("share-short-url"),
                    "share-short-full",
                    href,
                );
                return { ok: true, href: await writeClipboard(href) };
            }

            async function copyTitleLink(result, csrfToken) {
                const event = result && result.event;
                const eventId = event && event.id;
                const title = result && typeof result.title === "string" ? result.title : "";
                const origin = typeof location !== "undefined" && location.origin;
                if (
                    !result ||
                    !result.locator ||
                    !eventId ||
                    !title.trim() ||
                    !result.key ||
                    !result.claim ||
                    !origin
                ) {
                    return { ok: false, href: null };
                }
                let response;
                try {
                    response = await fetch("/title-link", {
                        method: "POST",
                        headers: { "Content-Type": "application/json" },
                        body: JSON.stringify({
                            title: title,
                            locator: result.locator,
                            event_id: eventId,
                            relays: Array.isArray(result.accepted) ? result.accepted : [],
                            claim: result.claim,
                            csrf_token: csrfToken || "",
                        }),
                    });
                } catch (error) {
                    return { ok: false, href: null };
                }
                if (!response || !response.ok) {
                    return { ok: false, href: null };
                }
                let payload;
                try {
                    payload = await response.json();
                } catch (error) {
                    return { ok: false, href: null };
                }
                const slug = payload && typeof payload.slug === "string" ? payload.slug : "";
                if (!slug) {
                    return { ok: false, href: null };
                }
                const href = new URL(
                    "/" + encodeURIComponent(slug) + "#" + result.key,
                    origin,
                ).href;
                paintShareUrl(
                    document.getElementById("share-title-url"),
                    "share-title-full",
                    href,
                );
                return { ok: true, href: await writeClipboard(href) };
            }

            async function writeClipboard(href) {
                const clipboard =
                    typeof navigator !== "undefined" && navigator.clipboard;
                if (!clipboard || typeof clipboard.writeText !== "function") {
                    return null;
                }
                try {
                    await clipboard.writeText(href);
                } catch (error) {
                    return null;
                }
                return href;
            }

            async function copyLongLink(result) {
                if (!result || !result.locator || !result.key) {
                    return null;
                }
                const href = pageHref(longSealHref(result));
                paintShareUrl(
                    document.getElementById("share-long-url"),
                    "share-long-full",
                    href,
                );
                return writeClipboard(href);
            }

            async function publishOnNostr() {
                if (noteLocked || !hasAnyPublishRelay()) {
                    return;
                }
                if (publishing || !everyRelayHasPath()) {
                    return;
                }
                if (editor.value.length > contentLimit) {
                    alert(
                        "Content exceeds " +
                            contentLimit.toLocaleString() +
                            " character limit.",
                    );
                    return;
                }
                if (!form.reportValidity()) {
                    return;
                }
                const title = form.title.value.trim();
                if (!title) {
                    showPublishError("A title is required.");
                    return;
                }
                const content = editor.value;
                if (!content.trim()) {
                    showPublishError("Write something before publishing.");
                    return;
                }
                showPublishError("");
                setPublishBusy(true);
                try {
                    const result = await nostorgraphPublishPublicNote({
                        title,
                        author: form.alias.value.trim(),
                        content,
                    });
                    if (
                        !result.locator ||
                        !result.key ||
                        !result.event ||
                        !/^[0-9a-f]{64}$/.test(result.event.id) ||
                        !result.accepted ||
                        !result.accepted.length
                    ) {
                        showPublishError("Publishing failed. Try again.");
                        return;
                    }
                    showShareLinks(result, title);
                } catch (error) {
                    console.error(error);
                    const tooLong =
                        error instanceof Error &&
                        error.message === "The sealed note is too long.";
                    showPublishError(
                        tooLong
                            ? "The post is too long."
                            : "Publishing failed. Try again.",
                    );
                } finally {
                    setPublishBusy(false);
                }
            }


            let sharedResult = null;

            function paintShareUrl(node, fullId, href) {
                if (node) {
                    node.textContent = href;
                }
                const full = document.getElementById(fullId);
                if (full) {
                    full.textContent = href;
                }
            }

            function shareTitleSlug(title) {
                const maxBytes = 250;
                let slug = "";
                let dash = false;
                for (const ch of String(title || "").trim()) {
                    if ([...slug].length >= 128) {
                        break;
                    }
                    const code = ch.codePointAt(0);
                    const blocked =
                        code <= 0x1f ||
                        (code >= 0x7f && code <= 0x9f) ||
                        "/\\?#%:*\"<>|".includes(ch);
                    if (blocked || /\s/u.test(ch)) {
                        if (slug) {
                            dash = true;
                        }
                        continue;
                    }
                    const extra = new TextEncoder().encode(ch).length + (dash ? 1 : 0);
                    if (new TextEncoder().encode(slug).length + extra > maxBytes) {
                        break;
                    }
                    if (dash) {
                        slug += "-";
                        dash = false;
                    }
                    slug += ch;
                }
                while (slug.endsWith("-")) {
                    slug = slug.slice(0, -1);
                }
                if (!slug || slug === "." || slug === "..") {
                    return "";
                }
                const reserved =
                    /^[0-9a-f]{20}$/.test(slug) ||
                    [
                        "about",
                        "api",
                        "legal",
                        "markup",
                        "nojs",
                        "relays",
                        "tor-circuits",
                        "editor-mode.js",
                        "home.css",
                        "home.js",
                        "home-noscript.css",
                        "logo.png",
                        "notice.css",
                        "nostr.js",
                        "post.css",
                        "post.js",
                        "post-noscript.css",
                        "relays.js",
                        "secp256k1.js",
                        "theme.js",
                        "writemark.css",
                        "writemark.js",
                        "writemark-live.js",
                        "robots.txt",
                    ].includes(slug);
                if (!reserved) {
                    return slug;
                }
                return slug + "-2";
            }

            function titleShareHref(result, slug) {
                const origin = typeof location !== "undefined" && location.origin;
                if (!slug || !origin || !result || !result.key) {
                    return "";
                }
                return new URL(
                    "/" + encodeURIComponent(slug) + "#" + result.key,
                    origin,
                ).href;
            }

            function showShareLinks(result, title) {
                sharedResult = Object.assign({}, result, {
                    title: title || "",
                });
                noteLocked = true;
                pinLongLink(longSealHref(sharedResult));
                syncPublishButtons();
                const panel = document.getElementById("share-links");
                if (!panel) {
                    return;
                }
                const longHref = pageHref(longSealHref(sharedResult));
                const shortHref = pageHref(shortSealHref(sharedResult));
                const titleHref = titleShareHref(
                    sharedResult,
                    shareTitleSlug(sharedResult.title),
                );
                const longUrl = document.getElementById("share-long-url");
                const shortUrl = document.getElementById("share-short-url");
                const titleUrl = document.getElementById("share-title-url");
                paintShareUrl(longUrl, "share-long-full", longHref);
                paintShareUrl(shortUrl, "share-short-full", shortHref);
                paintShareUrl(titleUrl, "share-title-full", titleHref);
                const status = document.getElementById("share-status");
                if (status) {
                    status.textContent = "";
                }
                panel.hidden = false;
                document.body.classList.add("is-stored");
                if (form.title) {
                    form.title.readOnly = true;
                    form.title.removeAttribute("title");
                }
                if (form.alias) {
                    form.alias.readOnly = true;
                    form.alias.removeAttribute("title");
                }
                editor.readonly = true;
                editor.removeAttribute("title");
                syncPublishButtons();
                if (!panel.dataset.ready) {
                    panel.dataset.ready = "1";
                    bindShareCopies(panel);
                    bindShareAddresses(panel);
                }
                panel.scrollIntoView({ block: "nearest" });
            }

            function bindShareAddresses(panel) {
                panel.querySelectorAll(".share-url").forEach((url) => {
                    url.addEventListener("click", () => {
                        const wrap = url.closest(".share-url-wrap");
                        if (!wrap) {
                            return;
                        }
                        const open = wrap.classList.contains("is-open");
                        panel.querySelectorAll(".share-url-wrap.is-open").forEach((node) => {
                            node.classList.remove("is-open");
                        });
                        if (!open) {
                            wrap.classList.add("is-open");
                        }
                        url.setAttribute("aria-expanded", open ? "false" : "true");
                    });
                });
            }

            function bindShareCopies(panel) {
                const csrf = form.csrf_token ? form.csrf_token.value : "";
                const status = document.getElementById("share-status");
                const finish = (button) => {
                    if (status) {
                        status.textContent = "";
                    }
                    button.classList.add("is-copied");
                    setTimeout(() => {
                        button.classList.remove("is-copied");
                    }, 1200);
                };
                const markCopied = (button, href) => {
                    if (!href) {
                        if (status) {
                            status.textContent = "That link was not copied.";
                        }
                        return;
                    }
                    finish(button);
                };
                const markSaved = (button, outcome) => {
                    if (!outcome || !outcome.ok) {
                        if (status) {
                            status.textContent = "That link was not saved.";
                        }
                        return;
                    }
                    if (!outcome.href) {
                        if (status) {
                            status.textContent = "Saved, but the link was not copied.";
                        }
                        return;
                    }
                    finish(button);
                };
                const longButton = document.getElementById("share-long");
                const shortButton = document.getElementById("share-short");
                const titleButton = document.getElementById("share-title");
                if (longButton) {
                    longButton.addEventListener("click", () => {
                        if (!sharedResult) {
                            return;
                        }
                        copyLongLink(sharedResult).then((href) =>
                            markCopied(longButton, href),
                        );
                    });
                }
                if (shortButton) {
                    shortButton.addEventListener("click", () => {
                        if (!sharedResult) {
                            return;
                        }
                        copyShortLink(sharedResult, csrf).then((outcome) =>
                            markSaved(shortButton, outcome),
                        );
                    });
                }
                if (titleButton) {
                    titleButton.addEventListener("click", () => {
                        if (!sharedResult) {
                            return;
                        }
                        copyTitleLink(sharedResult, csrf).then((outcome) =>
                            markSaved(titleButton, outcome),
                        );
                    });
                }
                panel.dataset.bound = "1";
            }

            let charCountFrame = 0;
            function updateCharCount() {
                if (charCountFrame) return;
                charCountFrame = requestAnimationFrame(paintCharCount);
            }
            function paintCharCount() {
                charCountFrame = 0;
                const count = editor.value.length;
                const countText =
                    count.toLocaleString() +
                    " / " +
                    contentLimit.toLocaleString();

                for (const root of [charCount, mobileCharCount]) {
                    if (!root) continue;
                    const span = root.querySelector(".live-count");
                    if (span) span.textContent = countText;
                }
                if (charCount && contentLimit > 0) {
                    const fill = Math.min(count / contentLimit, 1);
                    charCount.style.setProperty("--fill", String(fill));
                }

                // Update button state and character count styling
                if (count > contentLimit) {
                    if (charCount) charCount.classList.add("over-limit");
                    if (mobileCharCount)
                        mobileCharCount.classList.add("over-limit");
                } else {
                    if (charCount) charCount.classList.remove("over-limit");
                    if (mobileCharCount)
                        mobileCharCount.classList.remove("over-limit");
                }
                syncPublishButtons();
            }

            // The writemark editor emits input events as the document changes.
            editor.addEventListener("md-input", updateCharCount);
            editor.addEventListener("md-change", updateCharCount);

            form.addEventListener("submit", function (e) {
                e.preventDefault();
                publishOnNostr();
            });
            storeReady = true;

            paintCharCount();

            document
                .getElementById("aliasRand")
                .addEventListener("click", function () {
                    const pools = [
                        ["aarav","arjun","asha","ayaan","devi","jaya","kali","priya","riya","rohan","rupa","sana","tara","uma","vara","veda","vikram","yara"],
                        ["bao","bo","cai","chen","cheng","dao","fang","feng","gang","hao","hu","hui","jian","jing","jun","kai","lang","lei","li","liang","lin","ling","liu","long","mei","ming","na","ning","peng","ping","qian","qing","quan","rui","shan","sheng","tao","wei","wen","xia","xin","xing","xu","yan","yang","yi","ying","yu","yuan","yun","zhen","zheng","zhi","zhong","zhou","zhu"],
                        ["andile","dayo","fatou","jomo","kofi","mali","nala","nia","olu","osei","zuri","amara","dara","leya","kwame","abena","esi","yaw"],
                        ["amir","amira","bashir","cyrus","elif","emre","farrukh","idris","nour","omar","pari","rafi","rami","rana","reem","tariq","yael","zara"],
                        ["ana","diego","finn","ines","isla","lars","lena","luca","luna","maia","maren","nils","ona","orla","paz","rhea","rio","rosa","sion","sol","thea","tomás","wren"],
                        ["cleo","dani","dean","ezra","ivan","jade","ira","lior","mia","milo","mira","nadia","neo","noa","quinn","sasha","sera","shay","sia","zoe"],
                    ];
                    const pick = (a) => a[(Math.random() * a.length) | 0];
                    const pool = pick(pools);
                    const w = pool;
                    const rnd = (n) => (Math.random() * n) | 0;
                    const cap = (s) => s[0].toUpperCase() + s.slice(1);
                    const sep = () => pick(["","","","_",".","-"," "]);
                    const num = () =>
                        pick([
                            () => rnd(9) + 1,
                            () => rnd(90) + 10,
                            () => rnd(900) + 100,
                            () => rnd(9000) + 1000,
                            () => pick(["x","v","ii","iii","iv"]),
                        ]);
                    const maybe = (fn) => (Math.random() < 0.5 ? fn() : "");
                    const a = pick(w), b = pick(w), s = sep();
                    const formats = [
                        () => a + s + b,
                        () => a + s + b + num()(),
                        () => a + num()(),
                        () => cap(a) + b,
                        () => cap(a) + s + b + num()(),
                        () => a + s + cap(b),
                        () => cap(a) + cap(b),
                        () => cap(a) + cap(b) + num()(),
                        () => a + num()() + b,
                        () => a + b + maybe(() => num()()),
                        () => cap(a) + " " + cap(b),
                        () => cap(a) + " " + cap(b) + " " + cap(pick(w)),
                    ];
                    const name = pick(formats)();
                    document.querySelector("input[name=alias]").value = name.slice(
                        0,
                        32,
                    );
                });

            const saveRoot = document.querySelector(".save-note");
            if (saveRoot) {
                const opener = saveRoot.querySelector(".save-open");
                const formats = saveRoot.querySelector(".save-formats");
                const saveStatus = saveRoot.querySelector(".save-status");

                function noteFields() {
                    const title = document.querySelector('textarea[name="title"]');
                    const alias = document.querySelector('input[name="alias"]');
                    return {
                        title: title ? title.value : "",
                        alias: alias ? alias.value : "",
                        content: editor.value,
                    };
                }

                function noteIsEmpty(fields) {
                    return !fields.title.trim() && !fields.alias.trim() && !fields.content.trim();
                }

                function saveFilename(title, extension) {
                    const cleaned = title
                        .trim()
                        .replace(/[\u0000-\u001f\\/:*?"<>|]/g, " ")
                        .replace(/\s+/g, " ")
                        .replace(/[. ]+$/g, "")
                        .trim();
                    return (cleaned || "note") + extension;
                }

                function downloadFile(filename, body, type) {
                    const blob = new Blob([body], { type });
                    const url = URL.createObjectURL(blob);
                    const link = document.createElement("a");
                    link.href = url;
                    link.download = filename;
                    document.body.appendChild(link);
                    link.click();
                    link.remove();
                    setTimeout(function () {
                        URL.revokeObjectURL(url);
                    }, 1000);
                }

                function showSaveError(message) {
                    if (!saveStatus) return;
                    saveStatus.hidden = false;
                    saveStatus.textContent = message;
                }

                function clearSaveError() {
                    if (!saveStatus) return;
                    saveStatus.textContent = "";
                    saveStatus.hidden = true;
                }

                function saveMarkdown(fields) {
                    downloadFile(
                        saveFilename(fields.title, ".md"),
                        fields.content,
                        "text/markdown;charset=utf-8",
                    );
                }

                let pdfModule = null;

                function loadPdfModule() {
                    if (pdfModule) return Promise.resolve(pdfModule);
                    if (!loadPdfModule.pending) {
                        const js = new URL("./pdf/nostorgraph_pdf.js", import.meta.url);
                        const wasm = new URL("./pdf/nostorgraph_pdf_bg.wasm", import.meta.url);
                        loadPdfModule.pending = import(js.href)
                            .then((module) =>
                                module.default({ module_or_path: wasm }).then(() => module),
                            )
                            .then((module) => {
                                pdfModule = module;
                                return module;
                            })
                            .catch((error) => {
                                loadPdfModule.pending = null;
                                throw error;
                            });
                    }
                    return loadPdfModule.pending;
                }

                async function savePdf(fields) {
                    const page = editor.readyPage ? editor.readyPage() : null;
                    if (!page || typeof page.render_markdown !== "function") {
                        showSaveError("PDF is not ready.");
                        return;
                    }
                    try {
                        const html = page.render_markdown(
                            fields.content,
                            editor.syntaxTheme,
                            editor.maxUrlLength,
                            editor.externalLinks,
                        );
                        const pdf = await loadPdfModule();
                        if (typeof pdf.note_pdf !== "function") {
                            showSaveError("PDF is not ready.");
                            return;
                        }
                        const bytes = pdf.note_pdf(fields.title.trim(), fields.alias.trim(), html);
                        downloadFile(
                            saveFilename(fields.title, ".pdf"),
                            bytes,
                            "application/pdf",
                        );
                        clearSaveError();
                    } catch (error) {
                        showSaveError("Could not save the PDF.");
                    }
                }

                function setSaveOpen(open) {
                    if (!formats || !opener) return;
                    formats.hidden = !open;
                    opener.setAttribute("aria-expanded", open ? "true" : "false");
                }

                if (opener) {
                    opener.addEventListener("click", function () {
                        if (formats && !formats.hidden) {
                            setSaveOpen(false);
                            clearSaveError();
                            return;
                        }
                        if (noteIsEmpty(noteFields())) return;
                        setSaveOpen(true);
                    });
                }

                saveRoot.addEventListener("click", function (event) {
                    const choice = event.target && event.target.closest
                        ? event.target.closest("[data-save]")
                        : null;
                    if (!choice || !saveRoot.contains(choice)) return;
                    const fields = noteFields();
                    if (noteIsEmpty(fields)) return;
                    const kind = choice.getAttribute("data-save");
                    if (kind === "markdown") {
                        clearSaveError();
                        setSaveOpen(false);
                        saveMarkdown(fields);
                        return;
                    }
                    if (kind !== "pdf") return;
                    clearSaveError();
                    setSaveOpen(false);
                    savePdf(fields);
                });
            }
