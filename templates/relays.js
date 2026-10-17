const RELAY_LIST_KEY = "nonograph_relays";

function emptyRelayList() {
    return { dealt: [], pinned: [] };
}

function relayUrls(value) {
    if (!Array.isArray(value)) {
        return [];
    }
    return value.filter((entry) => typeof entry === "string");
}

function readRelayList(storage) {
    try {
        const raw = storage.getItem(RELAY_LIST_KEY);
        if (raw == null || raw === "") {
            return emptyRelayList();
        }
        const parsed = JSON.parse(raw);
        if (!parsed || typeof parsed !== "object") {
            return emptyRelayList();
        }
        return {
            dealt: relayUrls(parsed.dealt),
            pinned: relayUrls(parsed.pinned),
        };
    } catch (error) {
        return emptyRelayList();
    }
}

function writeRelayList(storage, list) {
    const saved = {
        dealt: relayUrls(list.dealt),
        pinned: relayUrls(list.pinned),
    };
    storage.setItem(RELAY_LIST_KEY, JSON.stringify(saved));
    return saved;
}

function dealRelays(candidates, open, pinned, removed) {
    if (open === 0) {
        return [];
    }
    const skip = new Set(relayUrls(pinned).concat(relayUrls(removed)));
    const chosen = [];
    for (const url of relayUrls(candidates)) {
        if (chosen.length === open) {
            break;
        }
        if (skip.has(url)) {
            continue;
        }
        chosen.push(url);
    }
    return chosen;
}

function loadRelayList(storage, candidates) {
    if (storage.getItem(RELAY_LIST_KEY) == null) {
        return writeRelayList(storage, {
            dealt: dealRelays(candidates, 3, [], []),
            pinned: [],
        });
    }
    return readRelayList(storage);
}

const RELAY_CAP = 6;

function boundedCount(pinned, count) {
    const floor = pinned.length;
    const requested = Number(count);
    if (!Number.isFinite(requested)) {
        return floor;
    }
    return Math.min(RELAY_CAP, Math.max(floor, Math.floor(requested)));
}

function shuffled(urls) {
    const copy = relayUrls(urls).slice();
    for (let index = copy.length - 1; index > 0; index -= 1) {
        const swap = Math.floor(Math.random() * (index + 1));
        const held = copy[index];
        copy[index] = copy[swap];
        copy[swap] = held;
    }
    return copy;
}

function parseRelay(relay) {
    if (typeof relay !== "string") {
        return null;
    }
    const trimmed = relay.trim();
    if (!trimmed.startsWith("wss://") || trimmed.length > 255 || !/^[\x20-\x7e]+$/.test(trimmed)) {
        return null;
    }
    const rest = trimmed.slice("wss://".length);
    if (!rest || rest.indexOf("@") !== -1) {
        return null;
    }
    let cut = rest.length;
    const slash = rest.indexOf("/");
    const query = rest.indexOf("?");
    if (slash !== -1) {
        cut = slash;
    }
    if (query !== -1 && query < cut) {
        cut = query;
    }
    const hostport = rest.slice(0, cut);
    const tail = rest.slice(cut);
    let host = hostport;
    if (hostport.charAt(0) === "[") {
        const end = hostport.indexOf("]");
        if (end === -1) {
            return null;
        }
        host = hostport.slice(1, end);
        const port = hostport.slice(end + 1);
        if (port && port !== ":443") {
            return null;
        }
    } else {
        const colon = hostport.lastIndexOf(":");
        if (colon !== -1) {
            if (hostport.slice(colon) !== ":443") {
                return null;
            }
            host = hostport.slice(0, colon);
        }
    }
    host = host.replace(/\.$/, "").toLowerCase();
    if (!host || blockedRelayHost(host)) {
        return null;
    }
    return { host: host, tail: tail };
}

function publicRelayUrl(relay) {
    return parseRelay(relay) !== null;
}

function blockedRelayHost(host) {
    if (host === "localhost" || host.endsWith(".localhost")) {
        return true;
    }
    const ipv4 = host.match(/^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/);
    if (ipv4) {
        const octets = ipv4.slice(1).map(Number);
        if (octets.some((octet) => octet > 255)) {
            return true;
        }
        if (octets[0] === 127 || octets[0] === 0 || octets[0] === 10) {
            return true;
        }
        if (octets[0] === 192 && octets[1] === 168) {
            return true;
        }
        if (octets[0] === 172 && octets[1] >= 16 && octets[1] <= 31) {
            return true;
        }
        if (octets[0] === 169 && octets[1] === 254) {
            return true;
        }
        if (octets[0] === 100 && octets[1] >= 64 && octets[1] <= 127) {
            return true;
        }
        return octets[0] >= 224;
    }
    if (host.includes(":")) {
        const ip = host.replace(/^\[|\]$/g, "");
        if (ip === "::1" || ip === "::") {
            return true;
        }
        if (ip.startsWith("fc") || ip.startsWith("fd") || ip.startsWith("fe80") || ip.startsWith("ff")) {
            return true;
        }
        if (ip.startsWith("::ffff:") || ip.startsWith("64:ff9b:")) {
            return true;
        }
    }
    return false;
}

function canonicalRelay(relay) {
    const parsed = parseRelay(relay);
    if (!parsed) {
        return "";
    }
    let path = parsed.tail;
    let query = "";
    const queryAt = path.indexOf("?");
    if (queryAt !== -1) {
        query = path.slice(queryAt);
        path = path.slice(0, queryAt);
    }
    path = path.replace(/\/+$/, "");
    if (path === "/") {
        path = "";
    }
    return "wss://" + parsed.host + path + query;
}

function setRelayCount(storage, candidates, count) {
    const saved = readRelayList(storage);
    const total = boundedCount(saved.pinned, count);
    const open = total - saved.pinned.length;
    let dealt = saved.dealt.slice(0, open);
    if (dealt.length < open) {
        const more = dealRelays(candidates, open - dealt.length, saved.pinned.concat(dealt), []);
        dealt = dealt.concat(more);
    }
    return writeRelayList(storage, { dealt, pinned: saved.pinned });
}

function addRelay(storage, url) {
    const canonical = canonicalRelay(url);
    const saved = readRelayList(storage);
    if (!canonical || saved.pinned.includes(canonical) || saved.pinned.length >= RELAY_CAP) {
        return saved;
    }
    const pinned = saved.pinned.concat([canonical]);
    let dealt = saved.dealt.filter((entry) => entry !== canonical);
    while (dealt.length + pinned.length > RELAY_CAP) {
        dealt.pop();
    }
    return writeRelayList(storage, { dealt, pinned });
}

function addRelayResult(storage, url) {
    const canonical = canonicalRelay(url);
    const saved = readRelayList(storage);
    if (!canonical) {
        return {
            list: saved,
            error: url.trim() ? "Use a public wss:// relay URL." : "Enter a relay URL.",
            clear: false,
        };
    }
    if (saved.pinned.includes(canonical)) {
        return { list: saved, error: "", clear: true };
    }
    if (saved.pinned.length >= RELAY_CAP) {
        return { list: saved, error: "Six relays are already pinned.", clear: false };
    }
    return { list: addRelay(storage, url), error: "", clear: true };
}

function removeRelay(storage, url) {
    const saved = readRelayList(storage);
    return writeRelayList(storage, {
        dealt: saved.dealt.filter((entry) => entry !== url),
        pinned: saved.pinned.filter((entry) => entry !== url),
    });
}

function deleteRelays(storage) {
    return writeRelayList(storage, { dealt: [], pinned: [] });
}

function reshuffleRelays(storage, candidates, count) {
    const saved = readRelayList(storage);
    const total = boundedCount(
        saved.pinned,
        count == null ? saved.dealt.length + saved.pinned.length : count,
    );
    const dealt = dealRelays(
        shuffled(candidates),
        total - saved.pinned.length,
        saved.pinned,
        saved.dealt,
    );
    return writeRelayList(storage, { dealt, pinned: saved.pinned });
}

function pinExtraRelays(storage) {
    const raw = storage.getItem("nonograph_extra_relays");
    if (raw == null) {
        return readRelayList(storage);
    }
    let extras = [];
    try {
        const parsed = JSON.parse(raw);
        if (Array.isArray(parsed)) {
            extras = parsed.filter(function (entry) {
                return typeof entry === "string";
            });
        }
    } catch (error) {
        extras = [];
    }
    let list = readRelayList(storage);
    for (let index = 0; index < extras.length; index += 1) {
        list = addRelay(storage, extras[index]);
    }
    if (typeof storage.removeItem === "function") {
        storage.removeItem("nonograph_extra_relays");
    }
    return list;
}

function paintRelayList(list, root) {
    const scope = root && root.querySelector ? root : document;
    const controls = scope.querySelector(".relay-deal-controls");
    if (!controls) {
        return;
    }
    const urls = list.pinned.concat(list.dealt);
    let rows = scope.querySelector(".relay-list");
    const empty = scope.querySelector(".relay-empty");
    if (urls.length === 0) {
        if (rows) {
            rows.remove();
        }
        if (!scope.querySelector(".relay-empty")) {
            const message = document.createElement("p");
            message.className = "relay-empty";
            message.textContent = "No relays.";
            controls.parentNode.insertBefore(message, controls);
        }
    } else {
        if (empty) {
            empty.remove();
        }
        if (!rows) {
            rows = document.createElement("ul");
            rows.className = "relay-list relay-list-user";
            controls.parentNode.insertBefore(rows, controls);
        }
        rows.replaceChildren();
        for (const url of urls) {
            const row = document.createElement("li");
            row.dataset.relay = url;
            const name = document.createElement("span");
            name.className = "relay-name";
            name.textContent = url.replace(/^wss:\/\//, "");
            const remove = document.createElement("button");
            remove.type = "button";
            remove.className = "relay-remove";
            remove.setAttribute("aria-label", "Remove relay");
            remove.textContent = "×";
            row.append(name, remove);
            rows.append(row);
        }
    }
    const total = list.pinned.length + list.dealt.length;
    const count = scope.querySelector(".relay-count");
    if (count) {
        count.textContent = total + "/" + RELAY_CAP;
    }
    const up = scope.querySelector(".relay-count-up");
    const down = scope.querySelector(".relay-count-down");
    if (up) {
        up.disabled = total >= RELAY_CAP;
    }
    if (down) {
        down.disabled = total <= list.pinned.length;
    }
    const reshuffle = scope.querySelector(".relay-reshuffle");
    if (reshuffle) {
        reshuffle.classList.toggle("relay-reshuffle-quiet", total === 0);
    }
    if (typeof globalThis.syncNostrButtons === "function") {
        globalThis.syncNostrButtons();
    }
}

function postRelayList(list) {
    const token = document.querySelector('input[name="csrf_token"]');
    fetch("/tor-circuits", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
            relays: list.pinned.concat(list.dealt),
            csrf_token: token ? token.value : "",
        }),
    }).catch(function () {});
}

function sameRelayList(current, next) {
    return (
        current.pinned.length === next.pinned.length &&
        current.dealt.length === next.dealt.length &&
        current.pinned.every(function (url, index) {
            return url === next.pinned[index];
        }) &&
        current.dealt.every(function (url, index) {
            return url === next.dealt[index];
        })
    );
}

function rememberRelayList(current, next) {
    if (
        !sameRelayList(current, next) &&
        typeof globalThis.refreshRelayCircuits !== "function"
    ) {
        postRelayList(next);
    }
    return next;
}

function bindRelayActions(storage, root) {
    const scope = root && root.querySelector ? root : document;
    let node = null;
    if (typeof scope.getElementById === "function") {
        node = scope.getElementById("relay-candidates");
    }
    if (!node && typeof scope.querySelector === "function") {
        node = scope.querySelector("#relay-candidates");
    }
    let candidates = [];
    if (node) {
        try {
            const parsed = JSON.parse(node.textContent);
            candidates = Array.isArray(parsed) ? parsed : [];
        } catch (error) {
            candidates = [];
        }
    }
    let list = loadRelayList(storage, candidates);
    function show(next) {
        const previous = list;
        list = rememberRelayList(previous, next);
        paintRelayList(list, scope);
        if (typeof globalThis.paintSavedRelayLists === "function") {
            globalThis.paintSavedRelayLists(document, storage);
        }
        if (
            !sameRelayList(previous, list) &&
            typeof globalThis.refreshRelayCircuits === "function"
        ) {
            void globalThis.refreshRelayCircuits({ force: true });
        }
    }
    show(pinExtraRelays(storage));
    const reshuffle = scope.querySelector(".relay-reshuffle");
    if (reshuffle) {
        reshuffle.addEventListener("click", () => {
            const saved = readRelayList(storage);
            show(reshuffleRelays(storage, candidates, saved.pinned.length + saved.dealt.length));
        });
    }
    const up = scope.querySelector(".relay-count-up");
    const down = scope.querySelector(".relay-count-down");
    if (up) {
        up.addEventListener("click", () => {
            const saved = readRelayList(storage);
            show(
                setRelayCount(
                    storage,
                    candidates,
                    saved.pinned.length + saved.dealt.length + 1,
                ),
            );
        });
    }
    if (down) {
        down.addEventListener("click", () => {
            const saved = readRelayList(storage);
            show(
                setRelayCount(
                    storage,
                    candidates,
                    saved.pinned.length + saved.dealt.length - 1,
                ),
            );
        });
    }
    const add = scope.querySelector(".relay-add-btn");
    const input = scope.querySelector(".relay-url-input");
    const error = scope.querySelector(".relay-add-error");
    if (add) {
        add.addEventListener("click", () => {
            const typed = input ? input.value : "";
            const result = addRelayResult(storage, typed);
            show(result.list);
            if (result.clear) {
                if (input) {
                    input.value = "";
                }
                if (error) {
                    error.textContent = "";
                }
            } else if (error) {
                error.textContent = result.error;
            }
        });
    }
    scope.addEventListener("click", (event) => {
        const target = event.target;
        if (!target || !target.closest) {
            return;
        }
        const button = target.closest(".relay-remove");
        if (!button) {
            return;
        }
        const row = button.closest("[data-relay]");
        if (!row) {
            return;
        }
        show(removeRelay(storage, row.dataset.relay));
    });
}

if (typeof document !== "undefined" && document.querySelector(".relays-page")) {
    bindRelayActions(localStorage);
}
