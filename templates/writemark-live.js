const TAG_NAME = "writemark-editor";
const LEGACY_TAG_NAME = "md-live-editor";

const DEFAULTS = Object.freeze({
  mode: "live",
  markdownFlavor: "gfm",
  tabBehavior: "editor-first",
  indentString: "  ",
  placeholder: "Write markdown...",
  renderDebounceMs: 100,
  smallDocChars: 20_000,
  largeDocChars: 100_000,
  linkTarget: "_self",
  allowRawHtml: false,
  sanitize: true,
  emptyRequiredTrim: true,
});

const REFLECTED_ATTRIBUTES = [
  "name",
  "value",
  "label",
  "placeholder",
  "mode",
  "markdown-flavor",
  "tab-behavior",
  "indent-string",
  "required",
  "disabled",
  "readonly",
  "spellcheck",
  "maxlength",
  "minlength",
  "aria-label",
  "aria-labelledby",
  "dir",
];

const LANGUAGES = [
  "python", "javascript", "typescript", "tsx", "jsx", "html", "css", "json", "bash", "shell", "sh",
  "sql", "yaml", "toml", "xml", "markdown", "text", "go", "rust", "java", "c", "cpp", "csharp",
  "php", "ruby", "swift", "kotlin", "r", "scala", "dockerfile", "nginx", "graphql", "regex"
];

const ALIASES = new Map([
  ["py", "python"],
  ["js", "javascript"],
  ["ts", "typescript"],
  ["yml", "yaml"],
  ["md", "markdown"],
  ["rb", "ruby"],
  ["rs", "rust"],
  ["cs", "csharp"],
  ["kt", "kotlin"],
]);

const LIVE_ARROW_KEYS = new Set(["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"]);
const NAVIGATION_KEYS = new Set(["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End", "PageUp", "PageDown"]);
const ESCAPABLE_PUNCTUATION = "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";

// Nostorgraph-flavored inline markup. Unlike CommonMark emphasis, Nostorgraph uses
// literal paired same-delimiter wrapping processed left-to-right in a fixed
// order (mirrors safe_replace in src/parser.rs). Each entry maps a delimiter to
// the HTML element the server produces.
const NOSTORGRAPH_INLINE_RULES = [
  { marker: "**", tag: "strong" },
  { marker: "*", tag: "em" },
  { marker: "_", tag: "u" },
  { marker: "~", tag: "del" },
  { marker: "^", tag: "sup" },
  { marker: "==", tag: "mark" },
  { marker: "#", tag: "span", className: "secret" },
];

const MARKER_PAIRS = [
  { marker: "**" },
  {
    marker: "*",
    lineStartSpaceBreaksPair: true,
    divider: { whenLineIs: "**", insert: "***" },
  },
  { marker: "`", literalAtLineStart: true },
];

// Replicate one safe_replace pass over `source`, returning the wrapped ranges as
// { openStart, openEnd, closeStart, closeEnd } in source offsets. Semantics: find
// the first delimiter, then the next matching delimiter; if the enclosed content
// is non-empty and single-line, it is a pair; otherwise the opener is literal and
// scanning continues past it. Regions already claimed by earlier (higher-priority)
// passes are skipped so markers inside them are treated as literal text, matching
// the server which never re-wraps inside a produced tag's markers.
function nostorgraphPairsForMarker(source, marker, claimed, allowEmpty = false) {
  const pairs = [];
  const len = source.length;
  const isClaimed = pos => {
    for (const [from, to] of claimed) if (pos >= from && pos < to) return true;
    return false;
  };
  let i = 0;
  while (i <= len - marker.length) {
    if (isClaimed(i) || source.startsWith(marker, i) === false) { i += 1; continue; }
    const openStart = i;
    const contentStart = i + marker.length;
    // Find next matching marker after the opener that is not inside a claimed region.
    let j = contentStart;
    let closeStart = -1;
    while (j <= len - marker.length) {
      if (!isClaimed(j) && source.startsWith(marker, j)) { closeStart = j; break; }
      j += 1;
    }
    if (closeStart === -1) { i = contentStart; continue; }
    const content = source.slice(contentStart, closeStart);
    if (content.includes("\n") || (content.length === 0 && !allowEmpty)) { i = contentStart; continue; }
    const closeEnd = closeStart + marker.length;
    pairs.push({ openStart, openEnd: contentStart, closeStart, closeEnd });
    i = closeEnd;
  }
  return pairs;
}

// Compute all Nostorgraph inline emphasis spans for a single line of source text,
// in the server's processing order. Returns a flat list of { from, to, html }
// insertion points suitable for interleaving with the escaped source, where each
// span carries its opening/closing HTML and (for live mode) the retained marker.
function nostorgraphEmphasisSpans(source, markerHtml = () => "") {
  const claimed = [];
  const spans = [];
  for (const rule of NOSTORGRAPH_INLINE_RULES) {
    const pairs = nostorgraphPairsForMarker(source, rule.marker, claimed);
    for (const pair of pairs) {
      const openTag = rule.className
        ? `<${rule.tag} class="${rule.className}">`
        : `<${rule.tag}>`;
      const closeTag = `</${rule.tag}>`;
      spans.push({ from: pair.openStart, to: pair.openEnd, html: `${markerHtml(rule.marker)}${openTag}` });
      spans.push({ from: pair.closeStart, to: pair.closeEnd, html: `${closeTag}${markerHtml(rule.marker)}` });
      // Claim the entire wrapped region (markers included) so later, lower-priority
      // passes treat any delimiters inside as literal text.
      claimed.push([pair.openStart, pair.closeEnd]);
    }
  }
  return spans;
}

// Render `source` with Nostorgraph inline emphasis. Mirrors renderEmphasisMarkdown's
// output contract: escapes text, interleaves span HTML, and (via markerHtml) can
// retain the raw delimiters for live editing.
function renderNostorgraphEmphasis(source, markerHtml = () => "") {
  const spans = nostorgraphEmphasisSpans(source, markerHtml);
  spans.sort((a, b) => a.from - b.from || b.to - a.to);
  let html = "";
  let cursor = 0;
  for (const span of spans) {
    if (span.from < cursor) continue;
    html += escapeHtml(source.slice(cursor, span.from));
    html += span.html;
    cursor = span.to;
  }
  html += escapeHtml(source.slice(cursor));
  return html;
}

function lineRangeAt(value, offset) {
  const start = value.lastIndexOf("\n", Math.max(0, offset - 1)) + 1;
  const endIndex = value.indexOf("\n", offset);
  const end = endIndex === -1 ? value.length : endIndex;
  return { start, end, text: value.slice(start, end) };
}

function offsetInFence(value, offset) {
  const lines = value.split("\n");
  let open = null;
  let pos = 0;
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    const lineStart = pos;
    const lineEnd = pos + line.length;
    const fence = /^(\s{0,3})(`{3,}|~{3,})(.*)$/.exec(line);
    const marker = fence?.[2] ?? "";
    const info = fence ? fence[3].trim() : "";
    const fenceOk = Boolean(fence) && !(marker[0] === "`" && info.includes("`"));
    if (open) {
      const closes = fenceOk && marker[0] === open.char && marker.length >= open.length && info === "";
      if (offset >= lineStart && offset <= lineEnd) return true;
      if (closes) open = null;
    } else if (fenceOk && lineEnd < value.length) {
      open = { char: marker[0], length: marker.length };
      if (offset >= lineStart && offset <= lineEnd) return true;
    }
    pos = lineEnd + 1;
  }
  return false;
}

function offsetInInlineCode(value, offset) {
  const line = lineRangeAt(value, offset);
  const before = line.text.slice(0, Math.max(0, offset - line.start));
  return ((before.match(/(?<!\\)`/g) || []).length % 2) === 1;
}

function atLineStart(value, offset) {
  const line = lineRangeAt(value, offset);
  return /^[ \t]{0,3}$/.test(line.text.slice(0, Math.max(0, offset - line.start)));
}

function canAutoCloseBefore(ch) {
  return ch == null || !/[A-Za-z0-9_]/.test(ch);
}

function pairMarker(marker) {
  return MARKER_PAIRS.find(entry => entry.marker === marker)?.marker ?? "";
}

function isPairKey(key) {
  return MARKER_PAIRS.some(entry => entry.marker.includes(key));
}

function emptyMarkerPair(value, caret) {
  for (const entry of MARKER_PAIRS) {
    const marker = entry.marker;
    if (caret < marker.length || !value.startsWith(marker, caret) || !value.startsWith(marker, caret - marker.length)) continue;
    if (value[caret - marker.length - 1] === "\\") continue;
    return marker;
  }
  return "";
}

function lineMarkerPairs(value, offset, allowEmpty = false) {
  const line = lineRangeAt(value, offset);
  const claimed = [];
  const pairs = [];
  const remember = (pair, marker) => {
    pairs.push({
      marker,
      openStart: line.start + pair.openStart,
      openEnd: line.start + pair.openEnd,
      closeStart: line.start + pair.closeStart,
      closeEnd: line.start + pair.closeEnd,
    });
    claimed.push([pair.openStart, pair.closeEnd]);
  };
  let i = 0;
  const text = line.text;
  while (i < text.length) {
    if (text[i] === "\\") { i += 2; continue; }
    if (text[i] !== "`") { i += 1; continue; }
    let j = i + 1;
    while (j < text.length && text[j] !== "`") j += 1;
    if (j >= text.length) break;
    if (j === i + 1) { i += 1; continue; }
    remember({ openStart: i, openEnd: i + 1, closeStart: j, closeEnd: j + 1 }, "`");
    i = j + 1;
  }
  for (const entry of MARKER_PAIRS) {
    if (entry.marker === "`") continue;
    for (const pair of nostorgraphPairsForMarker(text, entry.marker, claimed, allowEmpty)) remember(pair, entry.marker);
  }
  return pairs;
}

function lineTrimIs(value, offset, text) {
  return lineRangeAt(value, offset).text.trim() === text;
}
function enclosingMarkerExit(value, caret) {
  if (offsetInFence(value, caret)) return null;
  let pairs = lineMarkerPairs(value, caret, true);
  if (offsetInInlineCode(value, caret)) pairs = pairs.filter(pair => pair.marker === "`");
  const enclosing = pairs.filter(pair => caret >= pair.openEnd && caret <= pair.closeStart);
  if (!enclosing.length) return null;
  enclosing.sort((a, b) => (a.closeEnd - a.openStart) - (b.closeEnd - b.openStart) || b.openStart - a.openStart);
  const pair = enclosing[0];
  return { from: caret, to: caret, insert: "", start: pair.closeEnd, end: pair.closeEnd };
}
function markerPairEdit(value, start, end, key, action) {
  const source = String(value ?? "");
  const from = Math.max(0, Math.min(start, end));
  const to = Math.max(0, Math.max(start, end));
  if (offsetInFence(source, from)) return null;
  if (action === "tab") return from === to ? enclosingMarkerExit(source, from) : null;
  if (action === "backspace" || action === "delete") {
    if (from !== to) return null;
    const marker = emptyMarkerPair(source, from);
    if (!marker) return null;
    return { from: from - marker.length, to: from + marker.length, insert: "", start: from - marker.length, end: from - marker.length };
  }
  if (action === "space") {
    if (from !== to || offsetInInlineCode(source, from)) return null;
    const line = lineRangeAt(source, from);
    const before = line.text.slice(0, from - line.start);
    const after = line.text.slice(from - line.start);
    for (const entry of MARKER_PAIRS) {
      if (!entry.lineStartSpaceBreaksPair) continue;
      const marker = entry.marker;
      if (after !== marker || !before.endsWith(marker)) continue;
      if (!/^[ \t]{0,3}$/.test(before.slice(0, -marker.length))) continue;
      return { from, to: from + marker.length, insert: " ", start: from + 1, end: from + 1 };
    }
    return null;
  }
  if (action !== "insert" || String(key ?? "").length !== 1 || !isPairKey(key)) return null;
  const inCode = offsetInInlineCode(source, from);
  if (from !== to) {
    if (inCode || source.slice(from, to).includes("\n")) return null;
    const before = source[from - 1];
    const after = source[to];
    const longer = pairMarker(key + key);
    if (before === key && after === key && source[from - 2] !== key && source[to + 1] !== key && longer) {
      const inner = source.slice(from, to);
      return { from: from - 1, to: to + 1, insert: `${longer}${inner}${longer}`, start: from + key.length, end: to + key.length };
    }
    if (before === key && after === key) return null;
    if (!pairMarker(key)) return null;
    return { from, to, insert: `${key}${source.slice(from, to)}${key}`, start: from + 1, end: to + 1 };
  }
  if (inCode && key !== "`") return null;
  const around = emptyMarkerPair(source, from);
  const longer = pairMarker(key + key);
  if (around === key && longer) {
    const divider = MARKER_PAIRS.find(entry => entry.marker === key)?.divider;
    if (divider && lineTrimIs(source, from, divider.whenLineIs)) {
      const at = from - around.length;
      const end = at + divider.insert.length;
      return { from: at, to: from + around.length, insert: divider.insert, start: end, end };
    }
    return { from: from - around.length, to: from + around.length, insert: longer + longer, start: from + key.length, end: from + key.length };
  }
  if (around && around[0] === key) return { from, to: from, insert: "", start: from + around.length, end: from + around.length };
  const closer = lineMarkerPairs(source, from).find(pair => pair.closeStart === from && pair.marker[0] === key);
  if (closer) return { from, to: from, insert: "", start: closer.closeEnd, end: closer.closeEnd };
  if (inCode) return null;
  if (!pairMarker(key) || source[from] === key) return null;
  if (source[from - 1] === "\\") return null;
  if (MARKER_PAIRS.find(entry => entry.marker === key)?.literalAtLineStart && atLineStart(source, from)) return null;
  if (!canAutoCloseBefore(source[from])) return null;
  return { from, to: from, insert: key + key, start: from + 1, end: from + 1 };
}

function now() { return Date.now(); }
function clamp(n, min, max) { return Math.max(min, Math.min(max, n)); }
function normalizeLineEndings(value) { return String(value ?? "").replace(/\r\n?/g, "\n"); }
function parseLengthConstraint(value) {
  if (value == null || !/^\d+$/.test(String(value).trim())) return null;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed <= 2_147_483_647 ? parsed : null;
}
function escapeHtml(value) {
  return String(value ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}
function escapeAttribute(value) { return escapeHtml(value).replace(/`/g, "&#96;"); }
function stripHtml(value) { return String(value ?? "").replace(/<[^>]*>/g, ""); }
function isProbablyUrl(text) { return /^(https?:\/\/|mailto:|tel:|\/|#|\.\/|\.\.\/)[^\s]+$/i.test(String(text ?? "").trim()); }
function isSafeUrl(url, { allowDataImage = false } = {}) {
  const raw = String(url ?? "").trim();
  if (!raw) return false;
  const compact = raw.replace(/[\u0000-\u001F\u007F\s]+/g, "").toLowerCase();
  if (compact.startsWith("javascript:") || compact.startsWith("vbscript:") || compact.startsWith("file:")) return false;
  if (compact.startsWith("data:")) return allowDataImage && /^data:image\/(png|gif|jpe?g|webp);/i.test(compact);
  if (/^[a-z][a-z0-9+.-]*:/i.test(raw)) return /^(https?:|mailto:|tel:)/i.test(raw);
  return true;
}
function safeHref(url, opts = {}) { const raw = String(url ?? "").trim(); return raw === "" ? "" : isSafeUrl(raw, opts) ? raw : "#"; }
function headingSlug(value) {
  const text = String(value ?? "")
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/\\([\\`*_[\]{}()#+\-.!])/g, "$1")
    .replace(/[`*_~]/g, "")
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s_-]/gu, "")
    .trim()
    .replace(/\s+/g, "-");
  return text || "section";
}
function assignHeadingIds(blocks) {
  const seen = new Map();
  for (const block of blocks) {
    if (block.type !== "heading" || !block.heading) continue;
    const base = headingSlug(block.heading.content);
    const count = seen.get(base) ?? 0;
    seen.set(base, count + 1);
    block.heading.id = count === 0 ? base : `${base}-${count}`;
  }
  return blocks;
}
function isEscapablePunctuation(char) { return Boolean(char) && ESCAPABLE_PUNCTUATION.includes(char); }
function isBackslashEscaped(source, index) {
  let slashes = 0;
  for (let i = index - 1; i >= 0 && source[i] === "\\"; i -= 1) slashes += 1;
  return slashes % 2 === 1;
}
function parseCodeSpanAt(source, index) {
  if (source[index] !== "`" || source[index - 1] === "`" || isBackslashEscaped(source, index)) return null;
  let markerLength = 1;
  while (source[index + markerLength] === "`") markerLength += 1;
  let cursor = index + markerLength;
  while (cursor < source.length) {
    if (source[cursor] !== "`") { cursor += 1; continue; }
    let closingLength = 1;
    while (source[cursor + closingLength] === "`") closingLength += 1;
    if (closingLength === markerLength) {
      return {
        from: index,
        to: cursor + closingLength,
        marker: source.slice(index, index + markerLength),
        content: source.slice(index + markerLength, cursor),
        contentStart: index + markerLength,
        contentEnd: cursor,
      };
    }
    cursor += closingLength;
  }
  return null;
}
function normalizeCodeSpanContent(content) {
  let value = String(content ?? "").replace(/\n/g, " ");
  if (/^\s[\s\S]*\s$/.test(value) && /\S/.test(value)) value = value.slice(1, -1);
  return value;
}

function splitLinkDestinationAndTitle(raw) {
  const value = String(raw ?? "").trim();
  if (!value) return { url: "", title: "" };
  const angle = /^<([^<>\n]*)>(?:\s+(["'])(.*?)\2)?\s*$/.exec(value);
  if (angle) return { url: angle[1], title: angle[3] || "" };
  const quoted = /^(.*?)\s+(["'])(.*?)\2\s*$/.exec(value);
  if (quoted && quoted[1].trim()) return { url: quoted[1].trim(), title: quoted[3] };
  return { url: value, title: "" };
}

function parseInlineLinkAt(text, start = 0) {
  const source = String(text ?? "");
  if (isBackslashEscaped(source, start)) return null;
  const bang = source[start] === "!" ? "!" : "";
  let i = start + bang.length;
  if (source[i] !== "[" || isBackslashEscaped(source, i)) return null;
  let escaped = false;
  let labelEnd = -1;
  let labelDepth = 1;
  for (let j = i + 1; j < source.length; j += 1) {
    const ch = source[j];
    if (escaped) { escaped = false; continue; }
    if (ch === "\\") { escaped = true; continue; }
    if (ch === "[") { labelDepth += 1; continue; }
    if (ch === "]") {
      labelDepth -= 1;
      if (labelDepth === 0) { labelEnd = j; break; }
    }
  }
  if (labelEnd === -1 || source[labelEnd + 1] !== "(") return null;
  const label = source.slice(i + 1, labelEnd);
  const destStart = labelEnd + 2;
  let depth = 0;
  let quote = "";
  escaped = false;
  for (let j = destStart; j < source.length; j += 1) {
    const ch = source[j];
    if (escaped) { escaped = false; continue; }
    if (ch === "\\") { escaped = true; continue; }
    if (quote) {
      if (ch === quote) quote = "";
      continue;
    }
    if (ch === "\"" || ch === "'") { quote = ch; continue; }
    if (ch === "(") { depth += 1; continue; }
    if (ch === ")") {
      if (depth > 0) { depth -= 1; continue; }
      const destination = splitLinkDestinationAndTitle(source.slice(destStart, j));
      return {
        bang,
        label,
        url: destination.url,
        title: destination.title,
        from: start,
        to: j + 1,
        labelStart: i + 1,
        labelEnd,
        full: source.slice(start, j + 1),
      };
    }
  }
  return null;
}

// Nostorgraph bare bracket link: [https://example.com] renders as a link whose
// text is the URL itself. Mirrors process_links in src/parser.rs: only fires
// when the bracket content starts with "http" and contains no line break, and
// is not an image (`![...]`) or a `[label](url)`/`[label][ref]` link.
function parseBareLinkAt(text, start = 0) {
  const source = String(text ?? "");
  if (source[start] !== "[" || isBackslashEscaped(source, start)) return null;
  if (source[start - 1] === "!") return null;
  let escaped = false;
  let end = -1;
  for (let j = start + 1; j < source.length; j += 1) {
    const ch = source[j];
    if (escaped) { escaped = false; continue; }
    if (ch === "\\") { escaped = true; continue; }
    if (ch === "\n") return null;
    if (ch === "]") { end = j; break; }
  }
  if (end === -1) return null;
  // A following "(" means this is a [label](url) link; defer to that parser.
  if (source[end + 1] === "(") return null;
  const url = source.slice(start + 1, end);
  if (!/^https?:\/\//i.test(url)) return null;
  return {
    url,
    from: start,
    to: end + 1,
    labelStart: start + 1,
    labelEnd: end,
    full: source.slice(start, end + 1),
  };
}

function normalizeReferenceLabel(label) {
  return String(label ?? "")
    .replace(/\\([!-/:-@[-`{-~])/g, "$1")
    .trim()
    .replace(/\s+/g, " ")
    .toLowerCase();
}

function parseReferenceDefinition(line) {
  const match = /^ {0,3}\[([^\]\n]+)\]:[ \t]*(.+?)\s*$/.exec(String(line ?? ""));
  if (!match) return null;
  const label = normalizeReferenceLabel(match[1]);
  if (!label) return null;
  const destination = splitLinkDestinationAndTitle(match[2]);
  if (!destination.url) return null;
  return { label, ...destination };
}

function extractReferenceDefinitions(markdown, inherited = null) {
  const references = new Map(inherited instanceof Map ? inherited : []);
  const lines = normalizeLineEndings(markdown).split("\n");
  const output = lines.map(line => {
    const definition = parseReferenceDefinition(line);
    if (!definition) return line;
    if (!references.has(definition.label)) references.set(definition.label, definition);
    return "";
  });
  return { markdown: output.join("\n"), references };
}

function parseReferenceLinkAt(text, references, start = 0) {
  if (!(references instanceof Map) || references.size === 0) return null;
  const source = String(text ?? "");
  if (isBackslashEscaped(source, start)) return null;
  const bang = source[start] === "!" ? "!" : "";
  const labelStartMarker = start + bang.length;
  if (source[labelStartMarker] !== "[" || isBackslashEscaped(source, labelStartMarker)) return null;
  let escaped = false;
  let depth = 1;
  let labelEnd = -1;
  for (let i = labelStartMarker + 1; i < source.length; i += 1) {
    const char = source[i];
    if (escaped) { escaped = false; continue; }
    if (char === "\\") { escaped = true; continue; }
    if (char === "[") { depth += 1; continue; }
    if (char !== "]") continue;
    depth -= 1;
    if (depth === 0) { labelEnd = i; break; }
  }
  if (labelEnd === -1 || source[labelEnd + 1] === "(") return null;
  const label = source.slice(labelStartMarker + 1, labelEnd);
  let referenceLabel = label;
  let to = labelEnd + 1;
  if (source[to] === "[") {
    const referenceEnd = source.indexOf("]", to + 1);
    if (referenceEnd === -1 || isBackslashEscaped(source, referenceEnd)) return null;
    referenceLabel = source.slice(to + 1, referenceEnd) || label;
    to = referenceEnd + 1;
  }
  const reference = references.get(normalizeReferenceLabel(referenceLabel));
  if (!reference) return null;
  return {
    bang,
    label,
    url: reference.url,
    title: reference.title,
    from: start,
    to,
    labelStart: labelStartMarker + 1,
    labelEnd,
    full: source.slice(start, to),
  };
}

function isInlineWhitespace(char) {
  return !char || /\s/u.test(char);
}

function isInlinePunctuation(char) {
  return Boolean(char) && /[\p{P}\p{S}]/u.test(char);
}

// Emphasis is handled by the Nostorgraph inline engine defined near the top of
// this file (nostorgraphEmphasisSpans / renderNostorgraphEmphasis).

function htmlToMarkdown(html) {
  const template = document.createElement("template");
  template.innerHTML = String(html ?? "");
  const escapeMd = text => String(text ?? "").replace(/\u00a0/g, " ").replace(/\n{3,}/g, "\n\n");
  const walk = node => {
    if (node.nodeType === Node.TEXT_NODE) return escapeMd(node.nodeValue);
    if (node.nodeType !== Node.ELEMENT_NODE) return "";
    const tag = node.tagName.toLowerCase();
    const children = () => Array.from(node.childNodes).map(walk).join("");
    const block = text => `\n\n${text.trim()}\n\n`;
    if (tag === "br") return "\n";
    if (/^h[1-6]$/.test(tag)) return block(`${"#".repeat(Math.min(4, Number(tag[1])))} ${children().trim()}`);
    if (tag === "strong" || tag === "b") return `**${children()}**`;
    if (tag === "em" || tag === "i") return `*${children()}*`;
    if (tag === "code" && node.parentElement?.tagName?.toLowerCase() !== "pre") return `\`${children()}\``;
    if (tag === "pre") return block(`\`\`\`\n${node.textContent.replace(/\n+$/g, "")}\n\`\`\``);
    if (tag === "blockquote") return block(children().trim().split("\n").map(line => `> ${line}`).join("\n"));
    if (tag === "a") { const href = node.getAttribute("href") || ""; const label = children().trim() || href; return href && isSafeUrl(href) ? `[${label}](${href})` : label; }
    if (tag === "img") { const src = node.getAttribute("src") || ""; const alt = node.getAttribute("alt") || ""; return src && isSafeUrl(src, { allowDataImage: false }) ? `![${alt}](${src})` : alt; }
    if (tag === "ul" || tag === "ol") {
      const items = Array.from(node.children).filter(el => el.tagName.toLowerCase() === "li");
      return block(items.map((li, i) => `${tag === "ol" ? `${i + 1}.` : "-"} ${Array.from(li.childNodes).map(walk).join("").trim()}`).join("\n"));
    }
    if (tag === "table") {
      const rows = Array.from(node.querySelectorAll("tr")).map(tr => Array.from(tr.children).map(cell => Array.from(cell.childNodes).map(walk).join("").replace(/\|/g, "\\|").trim()));
      if (!rows.length) return "";
      const cols = Math.max(...rows.map(r => r.length));
      const pad = r => Array.from({ length: cols }, (_, i) => r[i] || "");
      const header = pad(rows[0]);
      const body = rows.slice(1).map(pad);
      return block([`| ${header.join(" | ")} |`, `| ${Array.from({ length: cols }, () => "---").join(" | ")} |`, ...body.map(r => `| ${r.join(" | ")} |`)].join("\n"));
    }
    if (["p", "div", "section", "article"].includes(tag)) return block(children());
    return children();
  };
  return Array.from(template.content.childNodes).map(walk).join("").replace(/\n{3,}/g, "\n\n").trim();
}
function tsvToMarkdownTable(text) {
  const rows = normalizeLineEndings(text).split("\n").filter(row => row.length > 0).map(row => row.split("\t").map(cell => cell.replace(/\|/g, "\\|").trim()));
  if (rows.length < 2 || rows.every(row => row.length < 2)) return null;
  const cols = Math.max(...rows.map(row => row.length));
  const pad = row => Array.from({ length: cols }, (_, i) => row[i] || "");
  const header = pad(rows[0]);
  const body = rows.slice(1).map(pad);
  return [`| ${header.join(" | ")} |`, `| ${Array.from({ length: cols }, () => "---").join(" | ")} |`, ...body.map(row => `| ${row.join(" | ")} |`)].join("\n");
}

function safeClipboardGet(clipboard, type) {
  try { return clipboard?.getData?.(type) || ""; } catch { return ""; }
}
function looksLikeMarkdown(text) {
  const source = normalizeLineEndings(text).trim();
  if (!source) return false;
  return looksLikeBlockMarkdown(source)
    || /(^|\s)(\*\*[^*]+\*\*|__[^_]+__|~~[^~]+~~|`[^`\n]+`|!\[[^\]]*\]\([^)]+\)|\[[^\]]+\]\([^)]+\))/m.test(source);
}
function looksLikeBlockMarkdown(text) {
  const source = normalizeLineEndings(text).trim();
  if (!source) return false;
  const lines = source.split("\n");
  if (lines.some(line => /^(\s{0,3}#{1,4}\s+|\s*([-+*])\s+|\s*\d+[.)]\s+|\s*[-+*]\s+\[(?: |x|X)\]\s+|\s*>\s?|\s*```|\s*~~~)/.test(line))) return true;
  if (lines.some(line => isHorizontalRule(line))) return true;
  if (lines.length >= 2 && isLikelyTableRow(lines[0]) && isTableDelimiter(lines[1])) return true;
  return false;
}
function markdownFromClipboardData(clipboard) {
  const explicit = safeClipboardGet(clipboard, "text/markdown") || safeClipboardGet(clipboard, "text/x-markdown");
  const text = normalizeLineEndings(safeClipboardGet(clipboard, "text/plain"));
  const html = safeClipboardGet(clipboard, "text/html");
  const table = text ? tsvToMarkdownTable(text) : null;
  if (table) return { markdown: table, kind: "table" };
  if (explicit) return { markdown: normalizeLineEndings(explicit), kind: "markdown" };
  if (html && (!text || !looksLikeMarkdown(text))) {
    const converted = htmlToMarkdown(html);
    if (converted) return { markdown: normalizeLineEndings(converted), kind: "html" };
  }
  if (text) return { markdown: text, kind: looksLikeMarkdown(text) ? "markdown" : "text" };
  if (html) {
    const converted = htmlToMarkdown(html);
    if (converted) return { markdown: normalizeLineEndings(converted), kind: "html" };
  }
  return { markdown: "", kind: "empty" };
}
function collectInlineMarkdownRanges(source) {
  const ranges = [];
  const addMatches = (regex, openLength, closeLength, labelGroup = 1, markerOffsetGroup = null) => {
    regex.lastIndex = 0;
    let match;
    while ((match = regex.exec(source))) {
      const label = match[labelGroup] ?? "";
      const markerOffset = markerOffsetGroup == null ? 0 : (match[markerOffsetGroup]?.length ?? 0);
      const from = match.index + markerOffset;
      const to = match.index + match[0].length;
      const innerFrom = from + openLength;
      const innerTo = to - closeLength;
      if (isBackslashEscaped(source, from) || isBackslashEscaped(source, to - closeLength)) continue;
      if (innerTo >= innerFrom && label.length >= 0) ranges.push({ from, to, innerFrom, innerTo });
      if (match.index === regex.lastIndex) regex.lastIndex += 1;
    }
  };
  // Nostorgraph inline markers (see NOSTORGRAPH_INLINE_RULES).
  addMatches(/\*\*([^*\n]+)\*\*/g, 2, 2);
  addMatches(/==([^=\n]+)==/g, 2, 2);
  addMatches(/(^|[^*])\*([^*\n]+)\*(?!\*)/g, 1, 1, 2, 1);
  addMatches(/_([^_\n]+)_/g, 1, 1);
  addMatches(/~([^~\n]+)~/g, 1, 1);
  addMatches(/\^([^^\n]+)\^/g, 1, 1);
  addMatches(/#([^#\n]+)#/g, 1, 1);
  for (let i = 0; i < source.length; i += 1) {
    const code = parseCodeSpanAt(source, i);
    if (!code) continue;
    ranges.push({ from: code.from, to: code.to, innerFrom: code.contentStart, innerTo: code.contentEnd });
    i = code.to - 1;
  }
  for (let i = 0; i < source.length; i += 1) {
    const link = parseInlineLinkAt(source, i) || parseBareLinkAt(source, i);
    if (!link) continue;
    ranges.push({ from: link.from, to: link.to, innerFrom: link.labelStart, innerTo: link.labelEnd });
    i = link.to - 1;
  }
  return ranges.sort((a, b) => (a.to - a.from) - (b.to - b.from));
}
function expandMarkdownFormattingRange(value, start, end) {
  let s = clamp(start, 0, value.length);
  let e = clamp(end, 0, value.length);
  if (s > e) [s, e] = [e, s];
  if (s === e) return { start: s, end: e };
  let changed = true;
  while (changed) {
    changed = false;
    const startLine = getLineRange(value, s);
    const endLine = getLineRange(value, e);
    if (startLine.start === endLine.start) {
      const lineInfo = makeLineInfo(startLine.start, startLine.end, startLine.text);
      const list = parseListItem(lineInfo.text);
      const heading = parseHeading(lineInfo.text);
      const quote = parseBlockquote(lineInfo.text);
      const contentStart = heading?.contentStart ?? list?.contentStart ?? quote?.contentStart;
      if (Number.isFinite(contentStart) && s === lineInfo.start + contentStart && e === lineInfo.end) {
        s = lineInfo.start;
        e = lineInfo.end;
        changed = true;
        continue;
      }
      const localStart = s - lineInfo.start;
      const localEnd = e - lineInfo.start;
      for (const r of collectInlineMarkdownRanges(lineInfo.text)) {
        if (localStart === r.innerFrom && localEnd === r.innerTo) {
          s = lineInfo.start + r.from;
          e = lineInfo.start + r.to;
          changed = true;
          break;
        }
      }
    }
  }
  return { start: s, end: e };
}
function normalizeIndentAttribute(value) {
  if (value === "tab" || value === "\\t") return "\t";
  if (value === "4" || value === "4-spaces") return "    ";
  if (value === "2" || value === "2-spaces") return "  ";
  if (value === "\t" || value === "  " || value === "    ") return value;
  return DEFAULTS.indentString;
}
function displayShortcut(shortcut) {
  if (!shortcut) return "";
  const ua = globalThis.navigator?.userAgent ?? "";
  let coarse = false;
  try { coarse = globalThis.matchMedia?.("(hover: none) and (pointer: coarse)")?.matches === true; } catch { coarse = false; }
  if (coarse || /iPhone|iPad|iPod/.test(ua)) return "";
  const isMac = /Mac|iPhone|iPad|iPod/.test(globalThis.navigator?.platform ?? "") || /Mac OS X/.test(ua);
  if (!isMac) return shortcut.replace(/Mod/g, "Ctrl");
  const symbols = { Mod: "⌘", Command: "⌘", Meta: "⌘", Alt: "⌥", Option: "⌥", Shift: "⇧", Ctrl: "⌃", Control: "⌃" };
  return shortcut.split("+").map(part => symbols[part] || part).join("");
}
function uid(prefix = "mfe") { return `${prefix}-${Math.random().toString(36).slice(2)}`; }

function getLines(value) {
  const source = normalizeLineEndings(value);
  const lines = [];
  let start = 0;
  for (let i = 0; i <= source.length; i += 1) {
    if (i === source.length || source[i] === "\n") {
      lines.push({ index: lines.length, start, end: i, text: source.slice(start, i), newlineEnd: i < source.length ? i + 1 : i });
      start = i + 1;
    }
  }
  if (source.length === 0) lines.length = 0;
  return lines;
}

function getLineRange(value, offset) {
  const source = normalizeLineEndings(value);
  const safe = clamp(offset, 0, source.length);
  const before = source.lastIndexOf("\n", Math.max(0, safe - 1));
  const start = before === -1 ? 0 : before + 1;
  const after = source.indexOf("\n", safe);
  const end = after === -1 ? source.length : after;
  return { start, end, text: source.slice(start, end) };
}

function getSelectedLineRanges(value, selectionStart, selectionEnd, opts = {}) {
  const startLine = getLineRange(value, selectionStart);
  const endProbe = selectionEnd > selectionStart && value[selectionEnd - 1] === "\n" ? selectionEnd - 1 : selectionEnd;
  const endLine = getLineRange(value, endProbe);
  const out = [];
  let cursor = startLine.start;
  while (cursor <= endLine.start) {
    const line = getLineRange(value, cursor);
    out.push(makeLineInfo(line.start, line.end, line.text, opts));
    if (line.end >= value.length) break;
    cursor = line.end + 1;
  }
  return out;
}

function makeLineInfo(start, end, text, opts = {}) {
  const indent = (/^(\s*)/.exec(text) || ["", ""])[1];
  const list = parseListItem(text, opts);
  const contentStart = list ? start + list.contentStart : start + indent.length;
  return { start, end, text, indent, marker: list?.markerText ?? null, contentStart };
}

function usesGfm(opts = {}) { return opts.gfm ?? opts.markdownFlavor !== "commonmark"; }

function parseListItem(line, opts = {}) {
  if (usesGfm(opts)) {
    const task = /^(\s*)([-+*])\s+\[( |x|X)\]\s+(.*)$/.exec(line);
    if (task) {
      const markerText = `${task[2]} [${task[3]}] `;
      return { kind: "task-list-item", listType: "ul", indent: task[1], marker: task[2], markerText, checked: task[3].toLowerCase() === "x", content: task[4], contentStart: task[1].length + markerText.length, fullMarkerStart: task[1].length, fullMarkerEnd: task[1].length + markerText.length };
    }
  }
  const ordered = /^(\s*)(\d+)([.)])\s+(.*)$/.exec(line);
  if (ordered) {
    const markerText = `${ordered[2]}${ordered[3]} `;
    return { kind: "ordered-list-item", listType: "ol", indent: ordered[1], marker: ordered[2], number: Number(ordered[2]), delimiter: ordered[3], markerText, content: ordered[4], contentStart: ordered[1].length + markerText.length, fullMarkerStart: ordered[1].length, fullMarkerEnd: ordered[1].length + markerText.length };
  }
  const bullet = /^(\s*)([-+*])\s+(.*)$/.exec(line);
  if (bullet) {
    const markerText = `${bullet[2]} `;
    return { kind: "bullet-list-item", listType: "ul", indent: bullet[1], marker: bullet[2], markerText, content: bullet[3], contentStart: bullet[1].length + markerText.length, fullMarkerStart: bullet[1].length, fullMarkerEnd: bullet[1].length + markerText.length };
  }
  return null;
}

function parseHeading(line) {
  const m = /^(\s{0,3})(#{1,4})(?:([ \t]+)(.*)|[ \t]*)$/.exec(line);
  if (!m) return null;
  let content = m[4] ?? "";
  if (/^#+[ \t]*$/.test(content)) content = "";
  else {
    const closing = /^(.*?)[ \t]+#+[ \t]*$/.exec(content);
    content = closing ? closing[1] : content.replace(/[ \t]+$/, "");
  }
  const separator = m[3] ?? "";
  return {
    indent: m[1],
    level: m[2].length,
    markerText: `${m[2]}${separator}`,
    content,
    contentStart: m[1].length + m[2].length + separator.length,
  };
}
function parseBlockquote(line) {
  const m = /^(\s*>\s?)(.*)$/.exec(line);
  if (!m) return null;
  let depth = 1;
  let fullContentStart = m[1].length;
  let nestedContent = m[2];
  while (true) {
    const nested = /^(\s*>\s?)(.*)$/.exec(nestedContent);
    if (!nested) break;
    depth += 1;
    fullContentStart += nested[1].length;
    nestedContent = nested[2];
  }
  return {
    markerText: m[1],
    content: m[2],
    contentStart: m[1].length,
    depth,
    fullContentStart,
  };
}
function contentSourceRange(line, contentStart, content) {
  const text = String(line?.text ?? "");
  const body = String(content ?? "");
  let start = Number(contentStart) || 0;
  if (text.slice(start, start + body.length) !== body) {
    const found = body ? text.indexOf(body) : -1;
    if (found >= 0) start = found;
  }
  const from = (Number(line?.start) || 0) + start;
  return { from, to: from + body.length };
}
// Nostorgraph dividers are exactly ***, -*-, ---, or === on their own line
// (mirrors process_dividers in src/parser.rs). Note: **** (four stars, e.g. from
// bolding an empty line) is intentionally NOT a divider.
function isHorizontalRule(line) { const t = line.trim(); return t === "***" || t === "-*-" || t === "---" || t === "==="; }
function getFenceInfo(line) {
  const m = /^(\s{0,3})(`{3,}|~{3,})[ \t]*(.*)$/.exec(line);
  if (!m) return null;
  const marker = m[2][0];
  const info = m[3].trim();
  if (marker === "`" && info.includes("`")) return null;
  return { marker, length: m[2].length, sequence: m[2], info, language: info.split(/[ \t]+/, 1)[0] || "" };
}
function isFenceLine(line) { return Boolean(getFenceInfo(line)); }
function isFenceCloseLine(line, opener) {
  const info = getFenceInfo(line);
  return Boolean(info && opener && info.marker === opener.marker && info.length >= opener.length && info.language === "");
}
function isCommentLine(line) {
  return String(line ?? "").trimStart().startsWith("// ");
}
function loneImageMarkdown(line) {
  const source = String(line ?? "");
  const start = source.search(/\S/);
  if (start < 0) return null;
  const image = parseInlineLinkAt(source, start);
  if (!image || image.bang !== "!" || image.from !== start) return null;
  if (source.slice(image.to).trim() !== "") return null;
  return image;
}
function isFenceOpenerLine(line) { return Boolean(getFenceInfo(line)); }
function parseSetextHeadingLevel(line) {
  const m = /^\s{0,3}(=+|-+)\s*$/.exec(line);
  if (!m) return null;
  return m[1][0] === "=" ? 1 : 2;
}
function isInsideInlineCode(lineBeforeCursor) { return ((lineBeforeCursor.match(/(?<!\\)`/g) || []).length % 2) === 1; }

function trimTableCellPadding(cell) {
  let value = String(cell ?? "");
  if (value.startsWith(" ")) value = value.slice(1);
  if (value.endsWith(" ")) value = value.slice(0, -1);
  return value;
}
function splitTableRow(line) {
  let row = String(line ?? "").trim();
  if (row.startsWith("|")) row = row.slice(1);
  if (row.endsWith("|")) row = row.slice(0, -1);
  const cells = [];
  let current = "";
  let escaped = false;
  for (const char of row) {
    if (escaped) { current += char; escaped = false; continue; }
    if (char === "\\") { current += char; escaped = true; continue; }
    if (char === "|") { cells.push(trimTableCellPadding(current)); current = ""; continue; }
    current += char;
  }
  cells.push(trimTableCellPadding(current));
  return cells;
}
function isTableDelimiter(line) {
  const t = String(line ?? "").trim();
  if (!t.includes("|")) return false;
  const cells = splitTableRow(t);
  return cells.length >= 2 && cells.every(cell => /^:?-{3,}:?$/.test(cell.trim()));
}
function isLikelyTableRow(line) {
  const t = String(line ?? "").trim();
  return t.includes("|") && splitTableRow(t).length >= 2;
}
function parseTableLineRanges(line, absoluteStart) {
  const raw = String(line ?? "");
  const cells = [];
  let start = 0;
  let end = raw.length;
  if (raw[start] === "|") start += 1;
  if (raw[end - 1] === "|") end -= 1;
  let cellStart = start;
  let escaped = false;
  for (let i = start; i <= end; i += 1) {
    const atEnd = i === end;
    const ch = raw[i];
    if (!atEnd && escaped) { escaped = false; continue; }
    if (!atEnd && ch === "\\") { escaped = true; continue; }
    if (atEnd || ch === "|") {
      const rawCellStart = cellStart;
      const rawCellEnd = i;
      let from = cellStart;
      let to = i;
      if (from < to && raw[from] === " ") from += 1;
      if (to > from && raw[to - 1] === " ") to -= 1;
      if (from === to && rawCellEnd > rawCellStart && raw[rawCellStart] === " ") {
        from = Math.min(rawCellStart + 1, rawCellEnd);
        to = from;
      }
      cells.push({ text: raw.slice(from, to), from: absoluteStart + from, to: absoluteStart + to });
      cellStart = i + 1;
    }
  }
  return cells;
}
function tableAlignmentFromDelimiter(cellText) {
  const value = String(cellText ?? "").trim();
  const left = value.startsWith(":");
  const right = value.endsWith(":");
  if (left && right) return "center";
  if (right) return "right";
  if (left) return "left";
  return "";
}
function tableAlignmentClass(alignment) {
  if (alignment === "center" || alignment === "right") return ` class="md-align-${alignment}"`;
  return "";
}
function listDepthClass(depth) {
  const level = Math.max(0, Math.min(32, Number(depth) || 0));
  return `md-list-depth-${level}`;
}
function unescapeTableCellText(cellText) {
  return String(cellText ?? "").replace(/\\([\\|])/g, "$1");
}
function unescapedCellOffset(raw, rel) {
  let display = 0;
  for (let i = 0; i < raw.length && i < rel;) {
    if (raw[i] === "\\" && (raw[i + 1] === "|" || raw[i + 1] === "\\")) {
      if (rel <= i + 1) return display;
      display += 1;
      i += 2;
    } else {
      display += 1;
      i += 1;
    }
  }
  return display;
}
function sourceIndexForUnescapedOffset(raw, unescapedOffset) {
  let display = 0;
  const target = Math.max(0, Number(unescapedOffset) || 0);
  for (let i = 0; i < raw.length;) {
    if (display >= target) return i;
    if (raw[i] === "\\" && (raw[i + 1] === "|" || raw[i + 1] === "\\")) {
      if (display + 1 >= target) return i + 2;
      display += 1;
      i += 2;
    } else {
      if (display + 1 >= target) return i + 1;
      display += 1;
      i += 1;
    }
  }
  return raw.length;
}

function classifyLine(value, offset, lineInfo, opts = {}) {
  if (isInsideFence(value, offset)) return { kind: "fenced-code" };
  const list = parseListItem(lineInfo.text, opts); if (list) return { kind: list.kind, list };
  const heading = parseHeading(lineInfo.text); if (heading) return { kind: "heading", heading };
  const quote = parseBlockquote(lineInfo.text); if (quote) return { kind: "blockquote", blockquote: quote };
  if (isHorizontalRule(lineInfo.text)) return { kind: "horizontal-rule" };
  if (usesGfm(opts) && isLikelyTableRow(lineInfo.text)) return { kind: "table" };
  return { kind: "paragraph" };
}
function isInsideFence(value, offset) {
  const source = normalizeLineEndings(value);
  const lines = getLines(source);
  let opener = null;
  for (const line of lines) {
    if (line.start >= offset) break;
    if (!opener) {
      const info = getFenceInfo(line.text);
      if (!info) continue;
      if (offset <= line.end) break;
      opener = info;
      continue;
    }
    if (isFenceCloseLine(line.text, opener)) {
      if (offset <= line.end) break;
      opener = null;
    }
  }
  return Boolean(opener);
}
function hasClosingFenceAfter(value, lineEnd, opener) {
  const rest = normalizeLineEndings(value).slice(lineEnd + 1);
  return getLines(rest).some(line => isFenceCloseLine(line.text, opener));
}

function applyTextChanges(value, changes) {
  const sorted = [...changes].sort((a, b) => b.from - a.from);
  let out = value;
  for (const c of sorted) {
    const from = clamp(c.from, 0, out.length);
    const to = clamp(c.to, from, out.length);
    out = out.slice(0, from) + normalizeLineEndings(c.insert ?? "") + out.slice(to);
  }
  return out;
}
function diffTextChange(before, after) {
  const oldValue = normalizeLineEndings(before ?? "");
  const newValue = normalizeLineEndings(after ?? "");
  if (oldValue === newValue) return [];
  let start = 0;
  const maxStart = Math.min(oldValue.length, newValue.length);
  while (start < maxStart && oldValue[start] === newValue[start]) start += 1;
  let oldEnd = oldValue.length;
  let newEnd = newValue.length;
  while (oldEnd > start && newEnd > start && oldValue[oldEnd - 1] === newValue[newEnd - 1]) {
    oldEnd -= 1;
    newEnd -= 1;
  }
  return [{ from: start, to: oldEnd, insert: newValue.slice(start, newEnd) }];
}
function normalizeChanges(changes = []) {
  return [...changes]
    .map(change => ({
      from: Number(change.from) || 0,
      to: Number(change.to) || Number(change.from) || 0,
      insert: normalizeLineEndings(change.insert ?? ""),
    }))
    .sort((a, b) => a.from - b.from || a.to - b.to);
}
function changedSpans(changes = []) {
  const sorted = normalizeChanges(changes);
  if (!sorted.length) return null;
  let oldStart = Infinity;
  let oldEnd = -Infinity;
  let newStart = Infinity;
  let newEnd = -Infinity;
  let shift = 0;
  for (const change of sorted) {
    oldStart = Math.min(oldStart, change.from);
    oldEnd = Math.max(oldEnd, change.to);
    const newFrom = change.from + shift;
    const newTo = newFrom + change.insert.length;
    newStart = Math.min(newStart, newFrom);
    newEnd = Math.max(newEnd, newTo);
    shift += change.insert.length - (change.to - change.from);
  }
  return { oldStart, oldEnd, newStart, newEnd, delta: shift };
}
function sameSelection(a, b) { return a && b && a.start === b.start && a.end === b.end && (a.direction || "none") === (b.direction || "none"); }
function makeSnapshot(value, selectionStart, selectionEnd, direction = "none") { return { value, selection: { start: selectionStart, end: selectionEnd, direction } }; }
function tx(ctx, actionId, changes, selectionAfter, undoGroup = actionId) {
  return { changes, selectionBefore: { start: ctx.selectionStart, end: ctx.selectionEnd, direction: ctx.selectionDirection ?? "none" }, selectionAfter, source: "api", actionId, undoGroup, timestamp: now() };
}
function ok(transaction, announcement) { return { ok: true, transaction, announcement }; }
function okNoop(announcement, preventDefault = false) { return { ok: true, announcement, preventDefault }; }
function fail(reason, message) { return { ok: false, reason, message }; }
function insertionTransaction(ctx, actionId, insert, selectionOffset = insert.length, undoGroup = actionId) {
  const from = ctx.selectionStart; const to = ctx.selectionEnd; const cursor = from + selectionOffset;
  return ok(tx(ctx, actionId, [{ from, to, insert }], { start: cursor, end: cursor, direction: "none" }, undoGroup));
}
function removePrefixFromLine(ctx, actionId, prefixEndOffset, announcement) {
  const from = ctx.currentLine.start; const to = ctx.currentLine.start + prefixEndOffset;
  return ok(tx(ctx, actionId, [{ from, to, insert: "" }], { start: from, end: from, direction: "none" }, actionId), announcement);
}

function parseBlocks(markdown, opts = {}) {
  const source = normalizeLineEndings(markdown);
  const lines = getLines(source);
  const blocks = [];
  const gfm = usesGfm(opts);
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    if (gfm && i + 1 < lines.length && isLikelyTableRow(line.text) && isTableDelimiter(lines[i + 1].text)) {
      const header = { ...lines[i], cells: parseTableLineRanges(lines[i].text, lines[i].start) };
      const delimiter = { ...lines[i + 1], cells: parseTableLineRanges(lines[i + 1].text, lines[i + 1].start) };
      const rows = [];
      let j = i + 2;
      while (j < lines.length && lines[j].text.trim() && isLikelyTableRow(lines[j].text)) {
        rows.push({ ...lines[j], cells: parseTableLineRanges(lines[j].text, lines[j].start) });
        j += 1;
      }
      blocks.push({ type: "table", from: line.start, to: (rows.at(-1) ?? delimiter).end, newlineEnd: (rows.at(-1) ?? delimiter).newlineEnd, header, delimiter, rows });
      i = j - 1;
      continue;
    }
    const fenceInfo = getFenceInfo(line.text);
    if (fenceInfo && (line.newlineEnd > line.end || i + 1 < lines.length)) {
      const codeLines = [];
      let j = i + 1;
      while (j < lines.length && !isFenceCloseLine(lines[j].text, fenceInfo)) { codeLines.push(lines[j]); j += 1; }
      const closing = j < lines.length ? lines[j] : null;
      blocks.push({ type: "code-fence", from: line.start, to: (closing ?? codeLines.at(-1) ?? line).end, newlineEnd: (closing ?? codeLines.at(-1) ?? line).newlineEnd, opening: line, closing, codeLines, language: fenceInfo.language, fence: fenceInfo });
      i = closing ? j : j - 1;
      continue;
    }
    const setextLevel = i + 1 < lines.length ? parseSetextHeadingLevel(lines[i + 1].text) : null;
    const canSetext = setextLevel
      && line.text.trim()
      && !parseHeading(line.text)
      && !parseListItem(line.text, opts)
      && !parseBlockquote(line.text)
      && !isHorizontalRule(line.text);
    if (canSetext) {
      blocks.push({ type: "heading", from: line.start, to: lines[i + 1].end, newlineEnd: lines[i + 1].newlineEnd, line, heading: { indent: "", level: setextLevel, markerText: lines[i + 1].text, content: line.text.trim(), contentStart: 0 }, setext: lines[i + 1] });
      i += 1;
      continue;
    }
    const heading = parseHeading(line.text);
    if (heading) { blocks.push({ type: "heading", from: line.start, to: line.end, newlineEnd: line.newlineEnd, line, heading }); continue; }
    const list = parseListItem(line.text, opts);
    if (list) { blocks.push({ type: list.kind, from: line.start, to: line.end, newlineEnd: line.newlineEnd, line, list }); continue; }
    const quote = parseBlockquote(line.text);
    if (quote) { blocks.push({ type: "blockquote", from: line.start, to: line.end, newlineEnd: line.newlineEnd, line, quote }); continue; }
    if (isHorizontalRule(line.text)) { blocks.push({ type: "horizontal-rule", from: line.start, to: line.end, newlineEnd: line.newlineEnd, line }); continue; }
    blocks.push({ type: line.text.trim() ? "paragraph" : "blank", from: line.start, to: line.end, newlineEnd: line.newlineEnd, line });
  }
  if (blocks.length === 0) blocks.push({ type: "blank", from: 0, to: 0, newlineEnd: 0, line: { start: 0, end: 0, newlineEnd: 0, text: "" } });
  return assignHeadingIds(blocks);
}

function decorateInline(raw, opts = {}) {
  const text = String(raw ?? "");
  const tokens = [];
  const reserve = html => {
    const placeholder = `\uE000${tokens.length}\uE001`;
    tokens.push([placeholder, html]);
    return placeholder;
  };
  const token = t => `<span class="md-token">${escapeHtml(t)}</span>`;
  let prepared = "";
  for (let i = 0; i < text.length; i += 1) {
    if (text[i] === "`") {
      const code = parseCodeSpanAt(text, i);
      if (code) {
        prepared += reserve(`${token(code.marker)}<code>${escapeHtml(code.content)}</code>${token(code.marker)}`);
        i = code.to - 1;
        continue;
      }
    }
    if (text[i] === "\\" && isEscapablePunctuation(text[i + 1])) {
      prepared += reserve(token("\\") + escapeHtml(text[i + 1]));
      i += 2;
      i -= 1;
      continue;
    }
    const bareLink = parseBareLinkAt(text, i);
    if (bareLink) {
      const safe = safeHref(bareLink.url);
      const rendered = `${token("[")}<a href="${escapeAttribute(safe)}" tabindex="-1">${escapeHtml(bareLink.url)}</a>${token("]")}`;
      prepared += reserve(rendered);
      i = bareLink.to - 1;
      continue;
    }
    const link = parseInlineLinkAt(text, i)
      || parseReferenceLinkAt(text, opts.references, i);
    if (link) {
      const safe = safeHref(link.url);
      // Empty labels must render as "" (not the <br> that decorateInline uses for
      // empty lines), otherwise `![]()`/`[]()` visually breaks across two lines.
      const labelHtml = link.label ? decorateInline(link.label, opts) : "";
      const prefix = token(`${link.bang}[`);
      const inline = text[link.labelEnd + 1] === "(";
      const suffix = inline
        ? `${token("](")}<span class="md-url">${escapeHtml(link.url)}</span>${token(")")}`
        : token(text.slice(link.labelEnd, link.to));
      const rendered = link.bang
        ? `${prefix}${labelHtml}${suffix}`
        : `${prefix}<a href="${escapeAttribute(safe)}" tabindex="-1">${labelHtml}</a>${suffix}`;
      prepared += reserve(rendered);
      i = link.to - 1;
      continue;
    }
    prepared += text[i];
  }
  const html = renderNostorgraphEmphasis(prepared, token);
  let restored = html;
  for (const [placeholder, reserved] of tokens) {
    restored = restored.replaceAll(escapeHtml(placeholder), reserved).replaceAll(placeholder, reserved);
  }
  return restored || "<br>";
}

function renderInlineMarkdown(source, opts = {}) {
  // Preview renderer: sanitize by construction. Unlike decorateInline, markdown delimiters are not retained.
  let text = String(source ?? "");
  const tokens = [];
  const reserve = html => { const token = `\uE000${tokens.length}\uE001`; tokens.push([token, html]); return token; };
  let codeReserved = "";
  for (let i = 0; i < text.length; i += 1) {
    const code = parseCodeSpanAt(text, i);
    if (!code) { codeReserved += text[i]; continue; }
    codeReserved += reserve(`<code>${escapeHtml(normalizeCodeSpanContent(code.content))}</code>`);
    i = code.to - 1;
  }
  text = codeReserved;
  let escapesReserved = "";
  for (let i = 0; i < text.length; i += 1) {
    if (text[i] === "\\" && isEscapablePunctuation(text[i + 1])) {
      escapesReserved += reserve(escapeHtml(text[i + 1]));
      i += 1;
    } else escapesReserved += text[i];
  }
  text = escapesReserved;
  let linked = "";
  for (let i = 0; i < text.length; i += 1) {
    const bareLink = parseBareLinkAt(text, i);
    if (bareLink) {
      const safe = safeHref(bareLink.url);
      const target = opts.linkTarget === "_blank" ? " target=\"_blank\" rel=\"noopener noreferrer\"" : "";
      linked += (safe === "#")
        ? bareLink.full
        : reserve(`<a href="${escapeAttribute(safe)}"${target}>${escapeHtml(bareLink.url)}</a>`);
      i = bareLink.to - 1;
      continue;
    }
    const link = parseInlineLinkAt(text, i)
      || parseReferenceLinkAt(text, opts.references, i);
    if (!link) { linked += text[i]; continue; }
    if (link.bang) {
      const safe = safeHref(link.url, { allowDataImage: false });
      linked += (safe === "#" && String(link.url).trim() !== "#")
        ? link.full
        : reserve(`<img src="${escapeAttribute(safe)}" alt="${escapeAttribute(link.label)}"${link.title ? ` title="${escapeAttribute(link.title)}"` : ""}>`);
    } else {
      const safe = safeHref(link.url);
      const target = opts.linkTarget === "_blank" ? " target=\"_blank\" rel=\"noopener noreferrer\"" : "";
      linked += (safe === "#" && String(link.url).trim() !== "#")
        ? link.label
        : reserve(`<a href="${escapeAttribute(safe)}"${target}${link.title ? ` title="${escapeAttribute(link.title)}"` : ""}>${renderInlineMarkdown(link.label, opts)}</a>`);
    }
    i = link.to - 1;
  }
  text = linked;
  text = text.replace(/(^|[\s(])((?:https?:\/\/)[^\s<]+[^\s<.,;:!?\])}])/g, (match, prefix, url) =>
    `${prefix}${reserve(`<a href="${escapeAttribute(safeHref(url))}">${escapeHtml(url)}</a>`)}`);
  text = text.replace(/(?: {2,}|\\)\n/g, () => `${reserve("<br>")}\n`);
  text = renderNostorgraphEmphasis(text);
  for (const [token, html] of tokens) text = text.replaceAll(escapeHtml(token), html).replaceAll(token, html);
  return text;
}

function isListBlock(block) {
  return ["bullet-list-item", "ordered-list-item", "task-list-item"].includes(block?.type);
}

function listIndentWidth(block) {
  return String(block?.list?.indent ?? "").replace(/\t/g, "    ").length;
}

function lineIndentWidth(line) {
  return (/^[ \t]*/.exec(String(line?.text ?? ""))?.[0] ?? "").replace(/\t/g, "    ").length;
}

function stripContinuationIndent(text, width) {
  let remaining = Math.max(0, width);
  let index = 0;
  const source = String(text ?? "");
  while (index < source.length && remaining > 0) {
    if (source[index] === " ") { index += 1; remaining -= 1; continue; }
    if (source[index] === "\t") { index += 1; remaining = Math.max(0, remaining - 4); continue; }
    break;
  }
  return source.slice(index);
}

function renderListFromBlocks(blocks, start, options) {
  const first = blocks[start];
  const baseIndent = listIndentWidth(first);
  const listType = first.list.listType;
  const sameLevel = block =>
    isListBlock(block)
    && block.list.listType === listType
    && listIndentWidth(block) === baseIndent;
  const items = [];
  let loose = false;
  let index = start;
  let stopList = false;

  while (sameLevel(blocks[index])) {
    const block = blocks[index];
    const contentIndent = baseIndent + block.list.contentStart - block.list.indent.length;
    const item = {
      block,
      paragraphs: [[block.list.content]],
      children: [],
    };
    index += 1;
    let pendingBlank = false;

    while (index < blocks.length) {
      const next = blocks[index];
      if (next.type === "blank") {
        const after = blocks[index + 1];
        if (sameLevel(after)) {
          loose = true;
          index += 1;
          break;
        }
        if (isListBlock(after) && listIndentWidth(after) > baseIndent) {
          loose = true;
          pendingBlank = true;
          index += 1;
          continue;
        }
        if (after?.type === "paragraph" && lineIndentWidth(after.line) >= contentIndent) {
          loose = true;
          pendingBlank = true;
          index += 1;
          continue;
        }
        stopList = true;
        break;
      }
      if (sameLevel(next)) break;
      if (isListBlock(next)) {
        if (listIndentWidth(next) <= baseIndent) {
          stopList = true;
          break;
        }
        const child = renderListFromBlocks(blocks, index, options);
        item.children.push(child.html);
        index = child.nextIndex;
        pendingBlank = false;
        continue;
      }
      if (next.type === "paragraph") {
        if (pendingBlank) {
          if (lineIndentWidth(next.line) < contentIndent) {
            stopList = true;
            break;
          }
          item.paragraphs.push([stripContinuationIndent(next.line.text, contentIndent)]);
          pendingBlank = false;
        } else {
          item.paragraphs.at(-1).push(stripContinuationIndent(next.line.text, contentIndent));
        }
        index += 1;
        continue;
      }
      stopList = true;
      break;
    }
    items.push(item);
    if (stopList || !sameLevel(blocks[index])) break;
  }

  const tag = listType === "ol" ? "ol" : "ul";
  const startNumber = first.list.number;
  const startAttribute = tag === "ol" && Number.isFinite(startNumber) && startNumber !== 1
    ? ` start="${startNumber}"`
    : "";
  const html = items.map(item => {
    const checkbox = item.block.list.kind === "task-list-item"
      ? `<input type="checkbox" disabled${item.block.list.checked ? " checked" : ""}> `
      : "";
    const paragraphs = item.paragraphs.map((lines, paragraphIndex) => {
      const prefix = paragraphIndex === 0 ? checkbox : "";
      const content = `${prefix}${renderInlineMarkdown(lines.join("\n"), options)}`;
      return loose || paragraphIndex > 0 ? `<p>${content}</p>` : content;
    }).join("");
    const children = item.children.length ? `\n${item.children.join("\n")}\n` : "";
    return `<li>${paragraphs}${children}</li>`;
  }).join("");
  return { html: `<${tag}${startAttribute}>${html}</${tag}>`, nextIndex: index };
}

function displayOffsetToSourceOffset(source, display, displayOffset) {
  const target = clamp(Number(displayOffset) || 0, 0, display.length);
  let shown = 0;
  let i = 0;
  while (i < source.length) {
    if (shown === target) {
      if (target < display.length) {
        const rest = source.indexOf(display[shown], i);
        if (rest === -1) return i;
        i = rest;
      }
      return i;
    }
    if (shown < display.length && source[i] === display[shown]) shown += 1;
    else if (shown < display.length && source.indexOf(display[shown], i) === -1) {
      shown += 1;
      continue;
    }
    i += 1;
  }
  return i;
}
function sourceOffsetToDisplayOffset(source, display, sourceOffset) {
  const target = clamp(Number(sourceOffset) || 0, 0, source.length);
  let shown = 0;
  for (let i = 0; i < target; i += 1) {
    if (shown < display.length && source[i] === display[shown]) shown += 1;
  }
  return Math.min(shown, display.length);
}
function sourceEditFromDisplay(source, oldDisplay, newDisplay) {
  const change = diffTextChange(oldDisplay, newDisplay)[0];
  if (!change) return { value: source, cursor: source.length };
  const from = displayOffsetToSourceOffset(source, oldDisplay, change.from);
  const to = displayOffsetToSourceOffset(source, oldDisplay, change.to);
  const end = Math.max(from, to);
  return {
    value: source.slice(0, from) + change.insert + source.slice(end),
    cursor: from + change.insert.length,
  };
}
function cursorAfterLineInput(displayCursor, mappedCursor, sourceCursor, fallbackCursor) {
  if (mappedCursor != null) return mappedCursor;
  if (displayCursor != null) return displayCursor;
  if (sourceCursor != null) return sourceCursor;
  return fallbackCursor;
}

// Tests eval everything above this line as a script.
export {
  TAG_NAME,
  LEGACY_TAG_NAME,
  DEFAULTS,
  REFLECTED_ATTRIBUTES,
  LANGUAGES,
  ALIASES,
  LIVE_ARROW_KEYS,
  NAVIGATION_KEYS,
  markerPairEdit,
  now,
  clamp,
  normalizeLineEndings,
  parseLengthConstraint,
  escapeHtml,
  escapeAttribute,
  stripHtml,
  isProbablyUrl,
  assignHeadingIds,
  extractReferenceDefinitions,
  htmlToMarkdown,
  tsvToMarkdownTable,
  safeClipboardGet,
  looksLikeBlockMarkdown,
  markdownFromClipboardData,
  expandMarkdownFormattingRange,
  normalizeIndentAttribute,
  displayShortcut,
  uid,
  getLineRange,
  getSelectedLineRanges,
  makeLineInfo,
  parseListItem,
  parseHeading,
  parseBlockquote,
  contentSourceRange,
  isHorizontalRule,
  getFenceInfo,
  isFenceLine,
  isCommentLine,
  loneImageMarkdown,
  isInsideInlineCode,
  splitTableRow,
  isTableDelimiter,
  isLikelyTableRow,
  tableAlignmentFromDelimiter,
  tableAlignmentClass,
  listDepthClass,
  unescapeTableCellText,
  unescapedCellOffset,
  sourceIndexForUnescapedOffset,
  classifyLine,
  isInsideFence,
  hasClosingFenceAfter,
  applyTextChanges,
  diffTextChange,
  normalizeChanges,
  changedSpans,
  sameSelection,
  makeSnapshot,
  tx,
  ok,
  okNoop,
  fail,
  insertionTransaction,
  removePrefixFromLine,
  parseBlocks,
  renderInlineMarkdown,
  displayOffsetToSourceOffset,
  sourceOffsetToDisplayOffset,
  sourceEditFromDisplay,
  cursorAfterLineInput,
};
