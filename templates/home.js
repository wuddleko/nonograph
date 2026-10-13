function savedRelayUrls(storage) {
    try {
        const raw = storage.getItem("nonograph_relays");
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
        if (!root || urls.length === 0) {
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

            const EXTRA_RELAYS_KEY = "nonograph_extra_relays";
            let maxFetchRelays = 6;
            let relaysForPublicPublish = () => [];
            let publicRelayUrl = () => false;
            let nostrLoading;

            function extraRelayLimit() {
                const taken = Math.min(savedRelayUrls(localStorage).length, maxFetchRelays);
                return maxFetchRelays - taken;
            }

            function readExtraRelays() {
                try {
                    const raw = localStorage.getItem(EXTRA_RELAYS_KEY);
                    const parsed = JSON.parse(raw || "[]");
                    if (!Array.isArray(parsed)) {
                        return [];
                    }
                    return parsed.filter((entry) => typeof entry === "string");
                } catch (error) {
                    return [];
                }
            }

            function loadExtraRelays() {
                const stored = readExtraRelays();
                const kept = stored.slice(0, extraRelayLimit());
                if (kept.length !== stored.length) {
                    saveExtraRelays(kept);
                }
                return kept;
            }

            function saveExtraRelays(relays) {
                localStorage.setItem(EXTRA_RELAYS_KEY, JSON.stringify(relays));
            }

            function mergePublishRelays() {
                const saved = savedRelayUrls(localStorage);
                const seen = new Set(saved.map((relay) => relayKey(relay)));
                const merged = [];
                for (const relay of loadExtraRelays()) {
                    const key = relayKey(relay);
                    if (seen.has(key)) {
                        continue;
                    }
                    seen.add(key);
                    merged.push(relay);
                }
                return merged.concat(saved);
            }

            function hasAnyPublishRelay() {
                return mergePublishRelays().length > 0;
            }

            globalThis.nonographPublishPublicNote = async function (fields) {
                const { publishPublicNote } = await loadNostr();
                const timeoutMs = Number(form.dataset.timeout) || 10_000;
                return publishPublicNote({
                    title: fields.title,
                    author: fields.author,
                    content: fields.content,
                    contentMax: Number(form.dataset.sealMax),
                    relays: mergePublishRelays(),
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

                // Auto-resize title textarea
                titleTextarea.addEventListener("input", function () {
                    this.style.height = "auto";
                    this.style.height = this.scrollHeight + "px";
                });
            }

            const contentLimit = Number(editor.getAttribute("maxlength")) || 0;
            const publishButtons = document.querySelectorAll(
                'button[type="submit"][form="publishForm"]',
            );
            const errorEl = form.querySelector(".form-error");
            const circuitByRelay = new Map();
            let circuitsTracked = false;

            function everyRelayHasPath() {
                if (!circuitsTracked) {
                    return true;
                }
                const relays = relaysForPublicPublish(mergePublishRelays());
                return (
                    relays.length > 0 &&
                    relays.every((relay) => {
                        const path = circuitByRelay.get(relayKey(relay));
                        return typeof path === "string" && path.length > 0;
                    })
                );
            }

            function syncNostrButtons() {
                const show = hasAnyPublishRelay();
                const waiting = show && !everyRelayHasPath();
                publishButtons.forEach((button) => {
                    button.hidden = !show;
                    button.classList.toggle("circuits-pending", waiting);
                    button.setAttribute("aria-disabled", waiting ? "true" : "false");
                });
            }

            function relayHost(url) {
                const rest = url.replace(/^wss:\/\//i, "").replace(/\/$/, "");
                const end = rest.search(/[/?#]/);
                return end === -1 ? rest : rest.slice(0, end);
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

            function renderExtraRelayLists() {
                const relays = loadExtraRelays();
                document.querySelectorAll(".relay-list-user").forEach((list) => {
                    list.replaceChildren();
                    for (const relay of relays) {
                        const row = document.createElement("li");
                        const label = document.createElement("span");
                        label.className = "relay-name";
                        label.textContent = relayHost(relay);
                        label.title = relay;
                        row.dataset.relay = relay;
                        const remove = document.createElement("button");
                        remove.type = "button";
                        remove.className = "relay-remove";
                        remove.setAttribute("aria-label", "Remove relay");
                        remove.textContent = "×";
                        remove.dataset.relay = relay;
                        row.append(label, remove);
                        list.append(row);
                    }
                    list.hidden = relays.length === 0;
                });
                paintRelayCircuits();
            }

            async function addRelayFromBlock(block) {
                const input = block.querySelector(".relay-url-input");
                const errorEl = block.querySelector(".relay-add-error");
                if (!input) {
                    return;
                }
                const value = input.value.trim();
                if (!value) {
                    if (errorEl) {
                        errorEl.textContent = "Enter a relay URL.";
                    }
                    return;
                }
                await loadNostr();
                if (!publicRelayUrl(value)) {
                    if (errorEl) {
                        errorEl.textContent = "Use a public wss:// relay URL.";
                    }
                    return;
                }
                const extra = loadExtraRelays();
                const key = relayKey(value);
                if (
                    extra.some((entry) => relayKey(entry) === key) ||
                    savedRelayUrls(localStorage).some((entry) => relayKey(entry) === key)
                ) {
                    if (errorEl) {
                        errorEl.textContent = "Already in your list.";
                    }
                    return;
                }
                const limit = extraRelayLimit();
                if (extra.length >= limit) {
                    if (errorEl) {
                        errorEl.textContent =
                            limit === 0
                                ? "This site already uses every relay slot."
                                : "Remove one first (max " + limit + ").";
                    }
                    return;
                }
                extra.push(key);
                saveExtraRelays(extra);
                input.value = "";
                if (errorEl) {
                    errorEl.textContent = "";
                }
                renderExtraRelayLists();
                void sendRelayList();
            }

            document.querySelectorAll("[data-relay-add]").forEach((block) => {
                const input = block.querySelector(".relay-url-input");
                block.querySelector(".relay-add-btn")?.addEventListener(
                    "click",
                    () => addRelayFromBlock(block),
                );
                input?.addEventListener("keydown", (event) => {
                    if (event.key === "Enter") {
                        event.preventDefault();
                        addRelayFromBlock(block);
                    }
                });
            });

            document.addEventListener("click", (event) => {
                const target = event.target;
                if (!(target instanceof HTMLElement)) {
                    return;
                }
                const remove = target.closest(".relay-remove");
                if (!remove || !remove.dataset.relay) {
                    return;
                }
                const relay = remove.dataset.relay;
                saveExtraRelays(
                    loadExtraRelays().filter((entry) => entry !== relay),
                );
                renderExtraRelayLists();
                void sendRelayList();
            });

            paintSavedRelayLists(document, localStorage);
            renderExtraRelayLists();
            let relayListSending = false;
            let relayListDirty = false;
            async function sendRelayList() {
                relayListDirty = true;
                if (relayListSending) {
                    return;
                }
                relayListSending = true;
                try {
                    while (relayListDirty) {
                        relayListDirty = false;
                        try {
                            await fetch("/tor-circuits", {
                                method: "POST",
                                headers: { "Content-Type": "application/json" },
                                body: JSON.stringify({
                                    relays: mergePublishRelays(),
                                    csrf_token: form.csrf_token
                                        ? form.csrf_token.value
                                        : "",
                                }),
                                signal: AbortSignal.timeout(4000),
                            });
                        } catch (error) {
                            break;
                        }
                    }
                } finally {
                    relayListSending = false;
                }
                if (relayListDirty) {
                    void sendRelayList();
                }
            }
            async function refreshRelayCircuits() {
                await loadNostr();
                await sendRelayList();
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
                paintRelayCircuits();
            }
            function paintRelayCircuits() {
                document.querySelectorAll(".relay-list li[data-relay]").forEach((row) => {
                    const path = circuitsTracked
                        ? circuitByRelay.get(relayKey(row.dataset.relay || ""))
                        : "";
                    let line = row.querySelector(".relay-circuit");
                    if (!line) {
                        line = document.createElement("div");
                        line.className = "relay-circuit";
                        row.append(line);
                    }
                    line.replaceChildren();
                    if (!circuitsTracked) {
                        line.classList.remove("relay-circuit-wait");
                        return;
                    }
                    if (path) {
                        line.classList.remove("relay-circuit-wait");
                        line.textContent = path;
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
            refreshRelayCircuits();
            setInterval(refreshRelayCircuits, 2000);

            let publishing = false;

            function syncPublishButtons() {
                const overLimit = editor.value.length > contentLimit;
                publishButtons.forEach((button) => {
                    button.disabled = publishing || overLimit;
                });
            }

            function setPublishBusy(busy) {
                publishing = busy;
                syncPublishButtons();
            }

            function showPublishError(message) {
                if (errorEl) {
                    errorEl.textContent = message;
                }
            }

            function acceptedRelayHints(relays) {
                const hints = [];
                for (const relay of relays || []) {
                    if (typeof relay !== "string" || !publicRelayUrl(relay)) {
                        continue;
                    }
                    const key = relayKey(relay);
                    if (hints.includes(key)) {
                        continue;
                    }
                    hints.push(key);
                }
                return hints;
            }

            function openedSealHref(result) {
                const query = acceptedRelayHints(result.hints)
                    .map((relay) => "r=" + encodeURIComponent(relay))
                    .join("&");
                return (
                    "/s/" +
                    result.locator +
                    "/" +
                    result.event.id +
                    (query ? "?" + query : "") +
                    "#" +
                    result.key
                );
            }

            async function publishOnNostr() {
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
                let leaveBusy = false;
                try {
                    const result = await nonographPublishPublicNote({
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
                    leaveBusy = true;
                    location.assign(openedSealHref(result));
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
                    if (!leaveBusy) {
                        setPublishBusy(false);
                    }
                }
            }


            function updateCharCount() {
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
            editor.addEventListener("input", updateCharCount);
            editor.addEventListener("md-change", updateCharCount);

            form.addEventListener("submit", function (e) {
                e.preventDefault();
                publishOnNostr();
            });

            updateCharCount();

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
