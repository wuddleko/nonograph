const { Point, utils, etc, CURVE } = await import(
    new URL(`./secp256k1.js${new URL(import.meta.url).search}`, import.meta.url)
);

export const KIND_LONG_FORM = 30023;
export const MAX_FETCH_RELAYS = 6;

export function publicRelayUrl(relay) {
    if (typeof relay !== "string" || !relay.startsWith("wss://")) {
        return false;
    }
    if (!relay.length || relay.length > 255 || !/^[\x20-\x7e]+$/.test(relay)) {
        return false;
    }
    let url;
    try {
        url = new URL(relay);
    } catch (error) {
        return false;
    }
    if (url.protocol !== "wss:") {
        return false;
    }
    if (url.username || url.password) {
        return false;
    }
    if (url.port && url.port !== "443") {
        return false;
    }
    const host = url.hostname.replace(/\.$/, "").toLowerCase();
    if (!host || blockedRelayHost(host)) {
        return false;
    }
    return true;
}

export function relaysForPublicPublish(relays) {
    const out = [];
    for (const relay of relays || []) {
        if (out.length === MAX_FETCH_RELAYS) {
            break;
        }
        if (!publicRelayUrl(relay) || out.includes(relay)) {
            continue;
        }
        out.push(relay);
    }
    return out;
}

export async function publishPublicNote({
    title,
    author,
    content,
    relays,
    timeoutMs,
    createdAt,
} = {}) {
    const signed = await signLongForm({
        title,
        author,
        content,
        createdAt,
    });
    const accepted = await sendToRelays(
        relaysForPublicPublish(relays),
        signed,
        timeoutMs,
    );
    const nevent = accepted.length
        ? encodeNevent(signed.id, signed.pubkey, KIND_LONG_FORM, accepted)
        : "";
    return { event: signed, accepted, nevent };
}

export function encodeNevent(idHex, pubkeyHex, kind, relays) {
    const id = hexToBytes(idHex);
    const pubkey = hexToBytes(pubkeyHex);
    if (id.length !== 32 || pubkey.length !== 32) {
        return "";
    }
    const data = [];
    pushTlv(data, 0, id);
    for (const relay of relaysForPublicPublish(relays)) {
        const bytes = new TextEncoder().encode(relay);
        if (bytes.length > 255) {
            continue;
        }
        pushTlv(data, 1, bytes);
    }
    pushTlv(data, 2, pubkey);
    const kindBytes = new Uint8Array(4);
    const value = Number(kind) >>> 0;
    kindBytes[0] = (value >>> 24) & 0xff;
    kindBytes[1] = (value >>> 16) & 0xff;
    kindBytes[2] = (value >>> 8) & 0xff;
    kindBytes[3] = value & 0xff;
    pushTlv(data, 3, kindBytes);
    return encodeBech32("nevent", data);
}

function pushTlv(out, tag, value) {
    out.push(tag, value.length);
    for (const byte of value) {
        out.push(byte);
    }
}

function encodeBech32(hrp, data) {
    const values = convertBits(data, 8, 5, true);
    const checksum = bech32Checksum(hrp, values);
    let out = hrp + "1";
    const charset = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";
    for (const value of values.concat(checksum)) {
        out += charset[value];
    }
    return out;
}

function convertBits(data, from, to, pad) {
    let acc = 0;
    let bits = 0;
    const maxv = (1 << to) - 1;
    const out = [];
    for (const value of data) {
        acc = (acc << from) | value;
        bits += from;
        while (bits >= to) {
            bits -= to;
            out.push((acc >> bits) & maxv);
        }
    }
    if (pad && bits > 0) {
        out.push((acc << (to - bits)) & maxv);
    }
    return out;
}

function bech32Checksum(hrp, data) {
    const values = hrpExpand(hrp).concat(data, [0, 0, 0, 0, 0, 0]);
    const mod = bech32Polymod(values) ^ 1;
    const ret = [];
    for (let i = 0; i < 6; i++) {
        ret.push((mod >>> (5 * (5 - i))) & 31);
    }
    return ret;
}

function hrpExpand(hrp) {
    const ret = [];
    for (let i = 0; i < hrp.length; i++) {
        ret.push(hrp.charCodeAt(i) >>> 5);
    }
    ret.push(0);
    for (let i = 0; i < hrp.length; i++) {
        ret.push(hrp.charCodeAt(i) & 31);
    }
    return ret;
}

function bech32Polymod(values) {
    const gen = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];
    let chk = 1;
    for (const value of values) {
        const top = chk >>> 25;
        chk = ((chk & 0x1ffffff) << 5) ^ value;
        for (let i = 0; i < 5; i++) {
            if ((top >>> i) & 1) {
                chk ^= gen[i];
            }
        }
    }
    return chk;
}

export async function signLongForm({
    title,
    author,
    content,
    createdAt,
    secret,
    aux,
    identifier,
} = {}) {
    await ensureBip340();
    const created_at = createdAt || Math.floor(Date.now() / 1000);
    const key = to32Bytes(secret) || utils.randomPrivateKey();
    const tags = longFormTags(title || "", author || "", created_at, identifier);
    const kind = KIND_LONG_FORM;
    const body = content || "";
    const pubkey = xOnlyPubkey(key);
    const id = await eventId(pubkey, created_at, kind, tags, body);
    const sig = await schnorrSign(key, hexToBytes(id), to32Bytes(aux));
    return {
        id,
        pubkey,
        created_at,
        kind,
        tags,
        content: body,
        sig: bytesToHex(sig),
    };
}

async function sendToRelays(relays, event, timeoutMs) {
    const configured = Number(timeoutMs);
    const wait = Math.max(
        10_000,
        Number.isFinite(configured) ? configured : 0,
    );
    const results = await Promise.all(
        relays.map((relay) =>
            sendEvent(relay, event, wait)
                .then(() => relay)
                .catch((error) => {
                    console.warn(relay, error && error.message);
                    return null;
                }),
        ),
    );
    return results.filter(Boolean);
}

function sendEvent(relay, event, timeoutMs) {
    return new Promise((resolve, reject) => {
        let done = false;
        const socket = new WebSocket(relay);
        const payload = JSON.stringify(["EVENT", event]);
        const finish = (error, abort) => {
            if (done) {
                return;
            }
            done = true;
            clearTimeout(timer);
            if (
                abort &&
                (socket.readyState === WebSocket.CONNECTING ||
                    socket.readyState === WebSocket.OPEN)
            ) {
                socket.close();
            }
            if (error) {
                reject(error);
            } else {
                resolve();
            }
        };
        const timer = setTimeout(() => {
            finish(new Error("timed out waiting for the relay"), true);
        }, timeoutMs);
        socket.addEventListener("open", () => {
            socket.send(payload);
        });
        socket.addEventListener("message", (message) => {
            let data;
            try {
                data = JSON.parse(message.data);
            } catch (error) {
                return;
            }
            if (!Array.isArray(data) || data[0] !== "OK" || data[1] !== event.id) {
                return;
            }
            if (data[2]) {
                finish(undefined, true);
            } else {
                finish(new Error(data[3] || "relay rejected the note"), true);
            }
        });
        socket.addEventListener("error", () => {
            finish(new Error("relay closed the connection"), false);
        });
        socket.addEventListener("close", () => {
            finish(new Error("relay closed the connection"), false);
        });
    });
}

function longFormTags(title, author, createdAt, identifier) {
    const tags = [
        ["d", identifier || randomHex(16)],
        ["title", title],
        ["published_at", String(createdAt)],
    ];
    if (author) {
        tags.push(["author", author]);
    }
    return tags;
}

function xOnlyPubkey(secret) {
    const point = Point.fromPrivateKey(secret);
    return bytesToHex(etc.numberToBytesBE(point.x));
}

async function schnorrSign(secret, message, aux) {
    let d = etc.bytesToNumberBE(secret);
    if (d === 0n || d >= CURVE.n) {
        throw new Error("private key invalid");
    }
    let P = Point.BASE.multiply(d);
    if (!hasEvenY(P)) {
        d = CURVE.n - d;
        P = Point.BASE.multiply(d);
    }
    const dBytes = etc.numberToBytesBE(d);
    const px = etc.numberToBytesBE(P.x);
    const auxBytes = aux || utils.randomPrivateKey();
    const t = xorBytes(dBytes, await taggedHash("BIP0340/aux", auxBytes));
    const rand = await taggedHash("BIP0340/nonce", t, px, message);
    let k = etc.mod(etc.bytesToNumberBE(rand), CURVE.n);
    if (k === 0n) {
        throw new Error("nonce invalid");
    }
    let R = Point.BASE.multiply(k);
    if (!hasEvenY(R)) {
        k = CURVE.n - k;
    }
    const rx = etc.numberToBytesBE(R.x);
    const e = etc.mod(
        etc.bytesToNumberBE(
            await taggedHash("BIP0340/challenge", rx, px, message),
        ),
        CURVE.n,
    );
    const s = etc.mod(k + e * d, CURVE.n);
    return etc.concatBytes(rx, etc.numberToBytesBE(s));
}

async function eventId(pubkey, createdAt, kind, tags, content) {
    const canonical = canonicalEvent(pubkey, createdAt, kind, tags, content);
    const digest = await sha256(new TextEncoder().encode(canonical));
    return bytesToHex(digest);
}

function canonicalEvent(pubkey, createdAt, kind, tags, content) {
    return (
        '[0,' +
        jsonString(pubkey) +
        "," +
        createdAt +
        "," +
        kind +
        "," +
        jsonTags(tags) +
        "," +
        jsonString(content) +
        "]"
    );
}

function jsonTags(tags) {
    let out = "[";
    tags.forEach((tag, tagIndex) => {
        if (tagIndex > 0) {
            out += ",";
        }
        out += "[";
        tag.forEach((item, itemIndex) => {
            if (itemIndex > 0) {
                out += ",";
            }
            out += jsonString(item);
        });
        out += "]";
    });
    return out + "]";
}

function jsonString(value) {
    let out = '"';
    for (const ch of String(value)) {
        const code = ch.codePointAt(0);
        if (ch === "\n") {
            out += "\\n";
        } else if (ch === '"') {
            out += '\\"';
        } else if (ch === "\\") {
            out += "\\\\";
        } else if (ch === "\r") {
            out += "\\r";
        } else if (ch === "\t") {
            out += "\\t";
        } else if (code === 0x08) {
            out += "\\b";
        } else if (code === 0x0c) {
            out += "\\f";
        } else if (code < 0x20) {
            out += "\\u" + code.toString(16).padStart(4, "0");
        } else {
            out += ch;
        }
    }
    return out + '"';
}

async function taggedHash(tag, ...parts) {
    const tagHash = await sha256(new TextEncoder().encode(tag));
    return sha256(etc.concatBytes(tagHash, tagHash, ...parts));
}

async function sha256(bytes) {
    const subtle = globalThis.crypto && globalThis.crypto.subtle;
    if (subtle && typeof subtle.digest === "function") {
        try {
            return new Uint8Array(await subtle.digest("SHA-256", bytes));
        } catch (error) {
            // http onion / LAN: SubtleCrypto is missing or refuses.
        }
    }
    return sha256Sync(bytes);
}

function sha256Sync(bytes) {
    const input = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    const extra = (64 - ((input.length + 9) % 64)) % 64;
    const padded = new Uint8Array(input.length + 1 + extra + 8);
    padded.set(input);
    padded[input.length] = 0x80;
    const bitLen = input.length * 8;
    const view = new DataView(padded.buffer);
    view.setUint32(padded.length - 8, Math.floor(bitLen / 0x100000000));
    view.setUint32(padded.length - 4, bitLen >>> 0);
    const K = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1,
        0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
        0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
        0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
        0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
        0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
        0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let h0 = 0x6a09e667,
        h1 = 0xbb67ae85,
        h2 = 0x3c6ef372,
        h3 = 0xa54ff53a,
        h4 = 0x510e527f,
        h5 = 0x9b05688c,
        h6 = 0x1f83d9ab,
        h7 = 0x5be0cd19;
    const w = new Uint32Array(64);
    const rotr = (x, n) => (x >>> n) | (x << (32 - n));
    for (let off = 0; off < padded.length; off += 64) {
        for (let i = 0; i < 16; i++) {
            w[i] = view.getUint32(off + i * 4);
        }
        for (let i = 16; i < 64; i++) {
            const s0 =
                rotr(w[i - 15], 7) ^ rotr(w[i - 15], 18) ^ (w[i - 15] >>> 3);
            const s1 =
                rotr(w[i - 2], 17) ^ rotr(w[i - 2], 19) ^ (w[i - 2] >>> 10);
            w[i] = (w[i - 16] + s0 + w[i - 7] + s1) >>> 0;
        }
        let a = h0,
            b = h1,
            c = h2,
            d = h3,
            e = h4,
            f = h5,
            g = h6,
            h = h7;
        for (let i = 0; i < 64; i++) {
            const S1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25);
            const ch = (e & f) ^ (~e & g);
            const t1 = (h + S1 + ch + K[i] + w[i]) >>> 0;
            const S0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22);
            const maj = (a & b) ^ (a & c) ^ (b & c);
            const t2 = (S0 + maj) >>> 0;
            h = g;
            g = f;
            f = e;
            e = (d + t1) >>> 0;
            d = c;
            c = b;
            b = a;
            a = (t1 + t2) >>> 0;
        }
        h0 = (h0 + a) >>> 0;
        h1 = (h1 + b) >>> 0;
        h2 = (h2 + c) >>> 0;
        h3 = (h3 + d) >>> 0;
        h4 = (h4 + e) >>> 0;
        h5 = (h5 + f) >>> 0;
        h6 = (h6 + g) >>> 0;
        h7 = (h7 + h) >>> 0;
    }
    const out = new Uint8Array(32);
    const outView = new DataView(out.buffer);
    outView.setUint32(0, h0);
    outView.setUint32(4, h1);
    outView.setUint32(8, h2);
    outView.setUint32(12, h3);
    outView.setUint32(16, h4);
    outView.setUint32(20, h5);
    outView.setUint32(24, h6);
    outView.setUint32(28, h7);
    return out;
}

let bip340Ready;

async function ensureBip340() {
    if (!bip340Ready) {
        bip340Ready = (async () => {
            const empty = bytesToHex(sha256Sync(new Uint8Array()));
            const abc = bytesToHex(sha256Sync(new TextEncoder().encode("abc")));
            if (
                empty !==
                    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" ||
                abc !==
                    "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
            ) {
                throw new Error("sha256 fallback is wrong");
            }
            const secret = hexToBytes(
                "0000000000000000000000000000000000000000000000000000000000000003",
            );
            const sig = bytesToHex(
                await schnorrSign(secret, new Uint8Array(32), new Uint8Array(32)),
            );
            if (
                sig !==
                "e907831f80848d1069a5371b402410364bdf1c5f8307b0084c55f1ce2dca821525f66a4a85ea8b71e482a74f382d2ce5ebeee8fdb2172f477df4900d310536c0"
            ) {
                throw new Error("schnorr signer is wrong");
            }
        })();
    }
    return bip340Ready;
}

function to32Bytes(value) {
    if (value instanceof Uint8Array) {
        if (value.length !== 32) {
            throw new Error("expected 32 bytes");
        }
        return value;
    }
    if (typeof value === "string" && value.length === 64) {
        return hexToBytes(value);
    }
    return null;
}

function hasEvenY(point) {
    return (point.y & 1n) === 0n;
}

function xorBytes(left, right) {
    const out = new Uint8Array(left.length);
    for (let i = 0; i < left.length; i++) {
        out[i] = left[i] ^ right[i];
    }
    return out;
}

function randomHex(bytes) {
    const raw = new Uint8Array(bytes);
    crypto.getRandomValues(raw);
    return bytesToHex(raw);
}

function bytesToHex(bytes) {
    return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join(
        "",
    );
}

function hexToBytes(hex) {
    const out = new Uint8Array(hex.length / 2);
    for (let i = 0; i < out.length; i++) {
        out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
    }
    return out;
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
        if (octets[0] >= 224) {
            return true;
        }
        return false;
    }
    if (host.includes(":")) {
        const ip = host.replace(/^\[|\]$/g, "");
        if (ip === "::1" || ip === "::") {
            return true;
        }
        if (ip.startsWith("fc") || ip.startsWith("fd")) {
            return true;
        }
        if (ip.startsWith("fe80") || ip.startsWith("ff")) {
            return true;
        }
        if (ip.startsWith("::ffff:") || ip.startsWith("64:ff9b:")) {
            return true;
        }
    }
    return false;
}
