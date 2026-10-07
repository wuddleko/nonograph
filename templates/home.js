            const editor = document.querySelector('writemark-editor[name="content"]');
            const charCount = document.getElementById("charCount");
            const mobileCharCount = document.getElementById("mobileCharCount");
            const form = document.getElementById("publishForm");

            const EXTRA_RELAYS_KEY = "nonograph_extra_relays";
            const MAX_EXTRA_RELAYS = 8;
            const MAX_FETCH_RELAYS = 6;

            function loadExtraRelays() {
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

            function saveExtraRelays(relays) {
                localStorage.setItem(EXTRA_RELAYS_KEY, JSON.stringify(relays));
            }

            function serverRelays() {
                try {
                    return JSON.parse(form.dataset.relays || "[]");
                } catch (error) {
                    return [];
                }
            }

            function mergePublishRelays() {
                const merged = [...serverRelays()];
                for (const relay of loadExtraRelays()) {
                    if (!merged.includes(relay)) {
                        merged.push(relay);
                    }
                }
                return merged;
            }

            function hasAnyPublishRelay() {
                return mergePublishRelays().length > 0;
            }

            // Same cap as nostr.js relaysForPublicPublish: extras past six can
            // keep a spinner, but they must not hold On Nostr.
            function relaysForPublicPublish(relays) {
                const out = [];
                for (const relay of relays) {
                    if (out.length === MAX_FETCH_RELAYS) {
                        break;
                    }
                    if (!out.includes(relay)) {
                        out.push(relay);
                    }
                }
                return out;
            }

            globalThis.nonographPublishPublicNote = async function (fields) {
                const { publishPublicNote } = await import(
                    new URL(
                        `./nostr.js${new URL(import.meta.url).search}`,
                        import.meta.url,
                    )
                );
                const timeoutMs = Number(form.dataset.timeout) || 10_000;
                return publishPublicNote({
                    title: fields.title,
                    author: fields.author,
                    content: fields.content,
                    contentMax: contentLimit,
                    relays: mergePublishRelays(),
                    timeoutMs,
                    csrfToken: form.csrf_token ? form.csrf_token.value : "",
                });
            };
            const progressCircle = document.getElementById("progressCircle");
            const mobileProgressCircle = document.getElementById(
                "mobileProgressCircle",
            );

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

            function updateProgressCircle(percentage, circleElement) {
                if (!circleElement) return;

                const progress = circleElement.querySelector(".progress");
                const circumference = 2 * Math.PI * 6; // radius = 6
                const offset =
                    circumference - (percentage / 100) * circumference;

                progress.style.strokeDashoffset = offset;

                if (percentage >= 100) {
                    progress.style.stroke = "#e74c3c"; // Red
                } else if (percentage >= 75) {
                    progress.style.stroke = "#f39c12"; // Yellow/Orange
                } else {
                    progress.style.stroke = "#666"; // Dark grey
                }
            }

            const contentLimit = Number(editor.getAttribute("maxlength")) || 0;
            const publishButtons = document.querySelectorAll(
                'button[type="submit"], .nostr-publish',
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
                document.querySelectorAll(".nostr-publish").forEach((button) => {
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
                const host = match[1].replace(/\.+$/, "").toLowerCase();
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
                const { publicRelayUrl } = await import(
                    new URL(
                        `./nostr.js${new URL(import.meta.url).search}`,
                        import.meta.url,
                    )
                );
                if (!publicRelayUrl(value)) {
                    if (errorEl) {
                        errorEl.textContent = "Use a public wss:// relay URL.";
                    }
                    return;
                }
                const extra = loadExtraRelays();
                if (extra.includes(value)) {
                    if (errorEl) {
                        errorEl.textContent = "Already in your list.";
                    }
                    return;
                }
                if (serverRelays().includes(value)) {
                    if (errorEl) {
                        errorEl.textContent = "That relay is already configured on this site.";
                    }
                    return;
                }
                if (extra.length >= MAX_EXTRA_RELAYS) {
                    if (errorEl) {
                        errorEl.textContent =
                            "Remove one first (max " + MAX_EXTRA_RELAYS + ").";
                    }
                    return;
                }
                extra.push(value);
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
                        !result.accepted ||
                        !result.accepted.length
                    ) {
                        showPublishError("Publishing failed. Try again.");
                        return;
                    }
                    leaveBusy = true;
                    location.assign("/s/" + result.locator + "#" + result.key);
                } catch (error) {
                    console.error(error);
                    showPublishError("Publishing failed. Try again.");
                } finally {
                    if (!leaveBusy) {
                        setPublishBusy(false);
                    }
                }
            }

            document.querySelectorAll(".nostr-publish").forEach((button) => {
                button.addEventListener("click", publishOnNostr);
            });

            function updateCharCount() {
                const count = editor.value.length;
                const countText =
                    count.toLocaleString() +
                    " / " +
                    contentLimit.toLocaleString();
                const percentage = (count / contentLimit) * 100;

                // Update desktop character count
                if (charCount) {
                    const span = charCount.querySelector("span");
                    if (span) span.textContent = countText;
                    updateProgressCircle(percentage, progressCircle);
                }

                // Update mobile character count
                if (mobileCharCount) {
                    const span = mobileCharCount.querySelector("span");
                    if (span) span.textContent = countText;
                    updateProgressCircle(percentage, mobileProgressCircle);
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
                if (publishing) {
                    e.preventDefault();
                    return false;
                }
                if (editor.value.length > contentLimit) {
                    e.preventDefault();
                    alert(
                        "Content exceeds " +
                            contentLimit.toLocaleString() +
                            " character limit.",
                    );
                    return false;
                }
                setPublishBusy(true);
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
