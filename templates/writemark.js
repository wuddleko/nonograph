/*
 * <writemark-editor> v1.3.1 live inline Markdown editor.
 * Dependency-free. No network calls. Markdown source is canonical.
 */

import {
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
} from "./writemark-live.js";

let pageModule = null;
function renderMarkdown(markdown, opts = {}) {
  const page = pageModule;
  if (!page || typeof page.render_markdown !== "function") return "";
  const theme = opts.syntaxTheme || "base16-ocean.dark";
  const parsedLimit = Number(opts.maxUrl);
  const maxUrl = Number.isInteger(parsedLimit) && parsedLimit >= 0 ? parsedLimit : 4096;
  const external = opts.external !== false;
  return page.render_markdown(String(markdown ?? ""), String(theme), maxUrl, external);
}
function renderInline(markdown, opts = {}) {
  const text = String(markdown ?? "");
  const page = pageModule;
  if (!page || typeof page.render_inline !== "function") return renderInlineMarkdown(text, opts);
  const parsedLimit = Number(opts.maxUrl);
  const maxUrl = Number.isInteger(parsedLimit) && parsedLimit >= 0 ? parsedLimit : 4096;
  const external = opts.external !== false;
  return page.render_inline(text, maxUrl, external);
}
function highlightCodeLines(language, code, opts = {}) {
  const page = pageModule;
  if (!page) return null;
  const theme = String(opts.syntaxTheme || "base16-ocean.dark");
  const lang = String(language ?? "");
  const source = String(code ?? "");
  if (typeof page.highlight_code_lines === "function") return Array.from(page.highlight_code_lines(lang, source, theme) || []);
  if (typeof page.highlight_code_line !== "function") return null;
  if (!source) return [];
  return source.split("\n").map((line, index, lines) => page.highlight_code_line(lang, index < lines.length - 1 ? `${line}\n` : line, theme));
}
function codeLineHtml(html) {
  const stripped = String(html ?? "").replace(/\n(?=(?:<\/[^>]+>)*$)/, "");
  return stripped ? stripped : "<br>";
}
function decodeHtmlAttribute(value) {
  return String(value ?? "")
    .replace(/&quot;/g, "\"")
    .replace(/&#39;/g, "'")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&");
}
function mediaFromInlineHtml(html) {
  const source = String(html ?? "");
  const img = /<img\b([^>]*)>/i.exec(source);
  if (img) {
    const src = /\bsrc="([^"]*)"/i.exec(img[1]);
    if (!src) return null;
    const alt = /\balt="([^"]*)"/i.exec(img[1]);
    return { kind: "img", src: decodeHtmlAttribute(src[1]), alt: alt ? decodeHtmlAttribute(alt[1]) : "" };
  }
  const video = /<source\b([^>]*)>/i.exec(source);
  if (video) {
    const src = /\bsrc="([^"]*)"/i.exec(video[1]);
    if (!src) return null;
    const type = /\btype="([^"]*)"/i.exec(video[1]);
    return { kind: "video", src: decodeHtmlAttribute(src[1]), type: type ? decodeHtmlAttribute(type[1]) : "" };
  }
  return null;
}
function codeLineRanges(block) {
  if (block.codeLines?.length) return block.codeLines.map(line => ({ from: line.start, to: line.end, text: line.text }));
  const at = block.opening ? block.opening.newlineEnd : block.to;
  return [{ from: at, to: at, text: "" }];
}
function liveInlineHtml(markdown, opts) {
  const text = String(markdown ?? "");
  const html = renderInline(text, opts);
  if (html) return html;
  return text ? escapeHtml(text) : "<br>";
}
function codeFenceBlockHtml(block, opts = {}) {
  if (opts.lineWindow) return codeFenceWindowHtml(block, opts);
  const editable = opts.editable || "true";
  const spellcheck = opts.spellcheck == null || opts.spellcheck === "" ? "true" : String(opts.spellcheck);
  const language = String(block.language || "").trim();
  const ranges = codeLineRanges(block);
  const highlighted = highlightCodeLines(language, ranges.map(range => range.text).join("\n"), opts) || [];
  const lines = ranges.map((range, index) => {
    const body = codeLineHtml(highlighted[index] ?? (range.text ? escapeHtml(range.text) : ""));
    return `<div class="md-code-line" part="code-line" data-editable="line" data-kind="code-fence" data-from="${range.from}" data-to="${range.to}" data-source-form="true" contenteditable="${editable}" spellcheck="false">${body}</div>`;
  }).join("");
  const fenceLine = (line, role) => {
    if (!line) return "";
    const text = line.text ? escapeHtml(line.text) : "<br>";
    return `<div class="md-code-fence-line" part="code-fence" data-editable="line" data-kind="code-fence" data-fence-role="${role}" data-from="${line.start}" data-to="${line.end}" data-source-form="true" contenteditable="${editable}" spellcheck="false">${text}</div>`;
  };
  const afterAnchor = block.closing && block.newlineEnd === block.to
    ? `<div class="md-line md-code-after" part="line" data-editable="virtual-code-after" data-kind="blank" data-from="${block.to}" data-to="${block.to}" contenteditable="${editable}" spellcheck="${spellcheck}" aria-label="After code block"><br></div>`
    : "";
  return `<div class="md-code-block" part="code-block" data-kind="code-fence" data-language="${escapeAttribute(language)}" contenteditable="false">${fenceLine(block.opening, "open")}<div class="md-code-lines">${lines}</div>${fenceLine(block.closing, "close")}</div>${afterAnchor}`;
}
function virtualGapHtml(height) {
  const size = Math.round(Number(height) || 0);
  if (size <= 0) return "";
  return `<div class="md-virtual-spacer" data-block-size="${size}"></div>`;
}
function codeFenceWindowHtml(block, opts) {
  const editable = opts.editable || "true";
  const spellcheck = opts.spellcheck == null || opts.spellcheck === "" ? "true" : String(opts.spellcheck);
  const language = String(block.language || "").trim();
  const ranges = codeLineRanges(block);
  const rows = [];
  if (block.opening) rows.push({ role: "open", line: block.opening });
  ranges.forEach(range => rows.push({ role: "code", range }));
  if (block.closing) rows.push({ role: "close", line: block.closing });
  const requested = opts.lineWindow;
  const start = clamp(requested.start, 0, rows.length);
  const end = clamp(requested.end, start, rows.length);
  const visible = rows.slice(start, end);
  const codeRows = visible.filter(row => row.role === "code");
  const highlighted = codeRows.length ? highlightCodeLines(language, codeRows.map(row => row.range.text).join("\n"), opts) || [] : [];
  let codeIndex = 0;
  const fenceLine = (line, role) => {
    if (!line) return "";
    const text = line.text ? escapeHtml(line.text) : "<br>";
    return `<div class="md-code-fence-line" part="code-fence" data-editable="line" data-kind="code-fence" data-fence-role="${role}" data-from="${line.start}" data-to="${line.end}" data-source-form="true" contenteditable="${editable}" spellcheck="false">${text}</div>`;
  };
  const codeLines = [];
  const parts = [];
  for (const row of visible) {
    if (row.role === "code") {
      const body = codeLineHtml(highlighted[codeIndex] ?? (row.range.text ? escapeHtml(row.range.text) : ""));
      codeIndex += 1;
      codeLines.push(`<div class="md-code-line" part="code-line" data-editable="line" data-kind="code-fence" data-from="${row.range.from}" data-to="${row.range.to}" data-source-form="true" contenteditable="${editable}" spellcheck="false">${body}</div>`);
      continue;
    }
    if (codeLines.length) {
      parts.push(`<div class="md-code-lines">${codeLines.join("")}</div>`);
      codeLines.length = 0;
    }
    parts.push(fenceLine(row.line, row.role));
  }
  if (codeLines.length) parts.push(`<div class="md-code-lines">${codeLines.join("")}</div>`);
  const lineHeight = Math.max(1, requested.lineHeight || 20);
  const topSpacer = virtualGapHtml(start > 0 ? start * lineHeight : 0);
  const bottomSpacer = virtualGapHtml(end < rows.length ? (rows.length - end) * lineHeight : 0);
  const afterAnchor = block.closing && block.newlineEnd === block.to
    ? `<div class="md-line md-code-after" part="line" data-editable="virtual-code-after" data-kind="blank" data-from="${block.to}" data-to="${block.to}" contenteditable="${editable}" spellcheck="${spellcheck}" aria-label="After code block"><br></div>`
    : "";
  return `<div class="md-code-block" part="code-block" data-kind="code-fence" data-language="${escapeAttribute(language)}" contenteditable="false">${topSpacer}${parts.join("")}${bottomSpacer}</div>${afterAnchor}`;
}
function taskLineHtml(block, opts = {}, sourceForm = false) {
  const editable = opts.editable || "true";
  const spellcheck = opts.spellcheck == null || opts.spellcheck === "" ? "true" : String(opts.spellcheck);
  const indent = opts.indentString || "  ";
  const locked = Boolean(opts.disabled || opts.readonly);
  const list = block.list;
  const checkOffset = block.line.start + list.indent.length + `${list.marker} [`.length;
  const range = contentSourceRange(block.line, list.contentStart, list.content);
  const taskName = textFromMarkdown(list.content, opts) || "task";
  const checkboxLabel = `${list.checked ? "Mark task incomplete" : "Mark task complete"}: ${taskName}`;
  const depthClass = listDepthClass(Math.floor(list.indent.length / Math.max(1, indent.length)));
  const body = sourceForm ? (list.content ? escapeHtml(list.content) : "<br>") : liveInlineHtml(list.content, opts);
  const form = sourceForm ? ` data-source-form="true"` : "";
  return `<div class="md-line md-task-line md-list ${depthClass}" part="line" data-kind="task-list-item" data-from="${block.line.start}" data-to="${block.line.end}"><input type="checkbox" part="checkbox" data-task-checkbox="true" data-check-offset="${checkOffset}" aria-label="${escapeAttribute(checkboxLabel)}" ${list.checked ? "checked" : ""} ${locked ? "disabled" : ""}><span class="md-task-source" data-editable="line" data-kind="task-list-item" data-from="${range.from}" data-to="${range.to}"${form} contenteditable="${editable}" spellcheck="${spellcheck}">${body}</span></div>`;
}
function liveDocumentHtml(markdown, opts = {}) {
  const blocks = assignHeadingIds(parseBlocks(markdown, opts));
  return blocks.map(block => liveBlockHtml(block, opts)).join("");
}
function liveBlockHtml(block, opts = {}) {
  const editable = opts.editable || "true";
  const spellcheck = opts.spellcheck == null || opts.spellcheck === "" ? "true" : String(opts.spellcheck);
  const indent = opts.indentString || "  ";
  const locked = Boolean(opts.disabled || opts.readonly);
  const lineAttrs = (line, kind, extra = "", attrs = "", from = line.start, to = line.end) => `class="md-line ${extra}" part="line" data-editable="line" data-kind="${kind}" data-from="${from}" data-to="${to}" contenteditable="${editable}" spellcheck="${spellcheck}"${attrs ? ` ${attrs}` : ""}`;
  if (block.type === "code-fence") return codeFenceBlockHtml(block, opts);
  if (block.line && isCommentLine(block.line.text)) {
    const line = block.line;
    const html = line.text ? escapeHtml(line.text) : "<br>";
    return `<div class="md-line md-comment" part="line" data-editable="line" data-kind="comment" data-from="${line.start}" data-to="${line.end}" data-source-form="true" contenteditable="${editable}" spellcheck="${spellcheck}">${html}</div>`;
  }
  if (block.line && loneImageMarkdown(block.line.text)) {
    const line = block.line;
    const media = mediaFromInlineHtml(renderInline(line.text, opts));
    const body = line.text ? escapeHtml(line.text) : "<br>";
    let mediaHtml = "";
    if (media?.kind === "img" && media.src) mediaHtml = `<img src="${escapeAttribute(media.src)}" alt="${escapeAttribute(media.alt)}" contenteditable="false">`;
    else if (media?.kind === "video" && media.src) mediaHtml = `<video controls contenteditable="false"><source src="${escapeAttribute(media.src)}" type="${escapeAttribute(media.type)}"></video>`;
    return `<div class="md-line md-image" part="line" data-editable="line" data-kind="image" data-from="${line.start}" data-to="${line.end}" data-source-form="true" contenteditable="${editable}" spellcheck="${spellcheck}">${body}${mediaHtml}</div>`;
  }
  if (block.type === "blank") {
    const placeholderAttrs = opts.emptyDocument ? `data-placeholder="${escapeAttribute(opts.placeholder || "")}"` : "";
    const placeholderClass = opts.emptyDocument ? "md-empty-placeholder" : "";
    return `<div ${lineAttrs(block.line, "blank", placeholderClass, placeholderAttrs)}>${block.line.text ? liveInlineHtml(block.line.text, opts) : "<br>"}</div>`;
  }
  if (block.type === "heading") {
    const afterAnchor = block.setext && block.newlineEnd === block.to
      ? `<div class="md-line md-setext-after" part="line" data-editable="virtual-setext-after" data-kind="blank" data-from="${block.to}" data-to="${block.to}" contenteditable="${editable}" spellcheck="${spellcheck}" aria-label="After heading"><br></div>`
      : "";
    const range = contentSourceRange(block.line, block.heading.contentStart, block.heading.content);
    return `<div ${lineAttrs(block.line, "heading", `md-heading md-h${block.heading.level}`, `id="${escapeAttribute(block.heading.id)}"`, range.from, range.to)}>${liveInlineHtml(block.heading.content, opts)}</div>${afterAnchor}`;
  }
  if (block.type === "blockquote") {
    const depth = Math.max(1, Number(block.quote.depth) || 1);
    const visualDepth = Math.min(depth, 16);
    const range = contentSourceRange(block.line, block.quote.contentStart, block.quote.content);
    return `<div ${lineAttrs(block.line, "blockquote", `md-quote md-quote-depth-${visualDepth}`, `data-quote-depth="${depth}"`, range.from, range.to)}>${liveInlineHtml(block.quote.content, opts)}</div>`;
  }
  if (block.type === "horizontal-rule") {
    return `<div ${lineAttrs(block.line, "horizontal-rule", "md-hr-line")} aria-label="Divider">${liveInlineHtml(block.line.text, opts)}</div>`;
  }
  if (block.type === "task-list-item") return taskLineHtml(block, opts, false);
  if (block.type === "bullet-list-item" || block.type === "ordered-list-item") {
    const list = block.list;
    const depth = Math.floor(list.indent.length / Math.max(1, indent.length));
    const range = contentSourceRange(block.line, list.contentStart, list.content);
    const marker = block.type === "ordered-list-item" ? `${list.number}${list.delimiter || "."}` : "•";
    const decorative = block.type === "bullet-list-item" ? ` aria-hidden="true"` : "";
    return `<div class="md-line md-list md-list-row ${listDepthClass(depth)}" part="line" data-kind="${block.type}" data-from="${block.line.start}" data-to="${block.line.end}"><span class="md-list-marker" contenteditable="false"${decorative}>${escapeHtml(marker)}</span><span class="md-list-content" data-editable="line" data-kind="${block.type}" data-from="${range.from}" data-to="${range.to}" contenteditable="${editable}" spellcheck="${spellcheck}">${liveInlineHtml(list.content, opts)}</span></div>`;
  }
  if (block.type === "table") return "";
  return `<div ${lineAttrs(block.line, "paragraph")}>${liveInlineHtml(block.line.text, opts)}</div>`;
}
function singleParagraphHtml(html) {
  const text = String(html ?? "").trim();
  if (!text.startsWith("<p>") || !text.endsWith("</p>")) return html;
  const inner = text.slice("<p>".length, -"</p>".length);
  if (inner.includes("<p>") || inner.includes("</p>")) return html;
  return inner;
}
// Positions on the DOM are from the last paint. Each section is the range one
// key replaced, in the coordinates of that moment, and how much the text grew.
// Replay walks the sections in order. The edited range grows. A range that only
// ends where the edit starts stays put, so a setext heading does not swallow
// the after-anchor that shares its edge. A point at or after the edit moves.
function mapPaintedOffset(pos, sections) {
  let out = pos;
  for (const section of sections) {
    if (out >= section.to) out += section.delta;
  }
  return out;
}
function mapPaintedRange(from, to, sections) {
  let start = from;
  let end = to;
  for (const section of sections) {
    const edited = from === section.start && to === section.at && start === section.from && end === section.to;
    if (edited) {
      end += section.delta;
      continue;
    }
    if (end <= section.from) continue;
    if (start >= section.to) {
      start += section.delta;
      end += section.delta;
      continue;
    }
    if (start <= section.from && end >= section.to) end += section.delta;
  }
  return { from: start, to: end };
}
// The caret is in the current note. Blocks are still from the last paint.
// Walk the sections backward so the caret lands in that painted block.
function unmapPaintedOffset(pos, sections) {
  let out = pos;
  for (let i = sections.length - 1; i >= 0; i -= 1) {
    const section = sections[i];
    const boundary = section.to + section.delta;
    if (section.delta > 0 && out > section.to && out < boundary) out = section.to;
    else if (out >= boundary) out -= section.delta;
  }
  return out;
}

function textFromMarkdown(markdown, opts = {}) {
  const extracted = extractReferenceDefinitions(markdown, opts.references);
  const options = { ...DEFAULTS, ...opts, references: extracted.references };
  const inlineText = text => stripHtml(renderInlineMarkdown(text, options));
  const lines = [];
  for (const block of parseBlocks(extracted.markdown, options)) {
    if (block.type === "blank") { lines.push(""); continue; }
    if (block.type === "heading") { lines.push(inlineText(block.heading.content)); continue; }
    if (block.type === "horizontal-rule") continue;
    if (block.type === "blockquote") { lines.push(inlineText(block.quote.content)); continue; }
    if (block.type === "bullet-list-item" || block.type === "ordered-list-item" || block.type === "task-list-item") { lines.push(inlineText(block.list.content)); continue; }
    if (block.type === "code-fence") { lines.push(block.codeLines.map(l => l.text).join("\n")); continue; }
    if (block.type === "table") {
      const tableRows = [block.header, ...block.rows];
      for (const row of tableRows) lines.push(row.cells.map(cell => inlineText(unescapeTableCellText(cell.text))).join("\t"));
      continue;
    }
    lines.push(inlineText(block.line.text));
  }
  return lines.join("\n").replace(/\n{3,}/g, "\n\n").trim();
}

function rememberPageModule(module) {
  pageModule = module;
  return module;
}
function clearPageLoad(pending, shared) {
  if (loadPageModule.pending === pending) loadPageModule.pending = null;
  if (globalThis.nonographPageModule === shared) globalThis.nonographPageModule = null;
}
async function loadPageModule() {
  if (pageModule) return pageModule;
  if (!loadPageModule.pending) {
    const shared = globalThis.nonographPageModule;
    if (shared) {
      let pending;
      pending = Promise.resolve(shared).then(rememberPageModule).catch((error) => {
        clearPageLoad(pending, shared);
        throw error;
      });
      loadPageModule.pending = pending;
    } else {
      const search = new URL(import.meta.url).search;
      const js = new URL("./page/nonograph_page.js" + search, import.meta.url);
      const wasm = new URL("./page/nonograph_page_bg.wasm" + search, import.meta.url);
      let pending;
      pending = import(js.href)
        .then((module) => module.default({ module_or_path: wasm }).then(() => module))
        .then(rememberPageModule)
        .catch((error) => {
          clearPageLoad(pending, pending);
          throw error;
        });
      loadPageModule.pending = pending;
      globalThis.nonographPageModule = pending;
    }
  }
  return loadPageModule.pending;
}

class WritemarkEditorElement extends HTMLElement {
  static formAssociated = true;
  static get observedAttributes() { return REFLECTED_ATTRIBUTES; }

  constructor() {
    super();
    this._internals = this.attachInternals?.() ?? null;
    this._shadow = this.attachShadow({ mode: "open", delegatesFocus: true });
    this._value = "";
    this._defaultValue = "";
    this._selection = { start: 0, end: 0, direction: "none" };
    this._dirty = false;
    this._formDisabled = false;
    this._hasConnected = false;
    this._isComposing = false;
    this._compositionEnding = false;
    this._compositionEndTimer = 0;
    this._compositionStamp = 0;
    this._keyRepeatAt = 0;
    this._suppressEnterUntil = 0;
    this._beforeInputSnapshot = null;
    this._beforeInputTarget = null;
    this._liveSelectionAPI = null;
    this._fallbackEditable = null;
    this._fallbackSelectionPending = false;
    this._undoStack = [];
    this._redoStack = [];
    this._maxUndo = 300;
    this._selectAllLevel = 0;
    this._structuredSelection = null;
    this._ignoreSelectionChangeCount = 0;
    this._pointerSelection = null;
    this._suppressLiveClick = false;
    this._focusWithin = false;
    this._sourceFormKeyShown = "";
    this._sourceFormLock = false;
    this._blockCacheValue = null;
    this._blockCache = null;
    this._liveBlocks = [];
    this._liveEditablesCache = [];
    this._liveNavigationCache = [];
    this._liveIndexDirty = true;
    this._validationVisible = false;
    this._completionUpdateFrame = 0;
    this._virtualState = { active: false, start: 0, end: 0, total: 0, lineHeight: 24, slices: new Map(), sliceKey: "" };
    this._blockHeights = new Map();
    this._virtualPainting = 0;
    this._virtualScrollFrame = 0;
    this._paintFrame = 0;
    this._pageRenderFrame = 0;
    this._deferredPaint = null;
    this._offsetMap = [];
    this._paintedOffsets = new WeakMap();
    this._paintedChecks = new WeakMap();
    this._spellEditable = null;
    this._sourceFormatTimer = 0;
    this._idleFormattedKey = "";
    this._actions = new Map();
    this._providers = new Map();
    this._completion = { open: false, providerId: null, match: null, items: [], activeIndex: 0, requestId: 0, abort: null };
    this._ids = { label: uid("mfe-label"), source: uid("mfe-source"), live: uid("mfe-live"), completion: uid("mfe-completion"), status: uid("mfe-status"), validation: uid("mfe-validation"), toolbar: uid("mfe-toolbar") };
    // Buttons shown in the floating selection toolbar (bubble menu), in order.
    // Icons are inline SVGs styled white via currentColor.
    const svg = inner => `<svg viewBox="0 0 24 24" aria-hidden="true">${inner}</svg>`;
    this._selectionToolbarActions = [
      { actionId: "inline.bold", label: "Bold", html: svg('<path d="M7 5h6a3.5 3.5 0 0 1 0 7H7z"/><path d="M7 12h7a3.5 3.5 0 0 1 0 7H7z"/>') },
      { actionId: "inline.italic", label: "Italic", html: svg('<line x1="10" y1="5" x2="18" y2="5"/><line x1="6" y1="19" x2="14" y2="19"/><line x1="14" y1="5" x2="10" y2="19"/>') },
      { actionId: "inline.underline", label: "Underline", html: svg('<path d="M7 4v6a5 5 0 0 0 10 0V4"/><line x1="5" y1="20" x2="19" y2="20"/>') },
      { actionId: "inline.strikethrough", label: "Strikethrough", html: svg('<path d="M18 6.5c-1.2-2.2-3.4-3-5.8-3C9 3.5 6.5 5.2 6.5 7.8c0 5.4 11 2.7 11 8.2 0 2.5-2.4 4.2-5.5 4.2-2.5 0-4.9-1-6.2-3.2"/><line x1="3" y1="12" x2="21" y2="12"/>') },
      { actionId: "inline.highlight", label: "Highlight", html: svg('<path d="M15 4l5 5-9 9-5 1 1-5z"/><line x1="4" y1="21" x2="20" y2="21"/>') },
      { actionId: "inline.code", label: "Inline code", html: svg('<polyline points="8 6 2 12 8 18"/><polyline points="16 6 22 12 16 18"/>') },
      { actionId: "inline.link", label: "Link", html: svg('<path d="M10 14a4 4 0 0 0 5.66 0l3-3a4 4 0 1 0-5.66-5.66l-1.5 1.5"/><path d="M14 10a4 4 0 0 0-5.66 0l-3 3a4 4 0 1 0 5.66 5.66l1.5-1.5"/>') },
    ];
    this._installBuiltInActions();
    this._installBuiltInProviders();
  }

  connectedCallback() {
    this._ensurePageRenderer();
    if (!this._hasConnected) {
      this._upgradeProperties();
      this._renderShell();
      this._bindEvents();
      this._hasConnected = true;
      const initial = this.getAttribute("value") ?? this._value ?? "";
      this._defaultValue = normalizeLineEndings(initial);
      this._setValueInternal(this._defaultValue, { source: "init", silent: true, recordUndo: false, preserveSelection: false });
      this._syncAttributesToControls();
      this._renderAll({ restoreSelection: false });
      this._updateFormValue();
      this._updateValidity();
    } else {
      this._bindDocumentScroll();
      this._syncAttributesToControls();
      this._renderAll({ restoreSelection: true });
      this._updateFormValue();
      this._updateValidity();
    }
  }
  disconnectedCallback() {
    this._completion.abort?.abort();
    if (this._completionUpdateFrame) cancelAnimationFrame(this._completionUpdateFrame);
    if (this._virtualScrollFrame) cancelAnimationFrame(this._virtualScrollFrame);
    if (this._paintFrame) cancelAnimationFrame(this._paintFrame);
    if (this._pageRenderFrame) cancelAnimationFrame(this._pageRenderFrame);
    if (this._boundDocumentScroll) {
      this.ownerDocument?.removeEventListener("scroll", this._boundDocumentScroll, true);
      this.ownerDocument?.defaultView?.removeEventListener("scroll", this._boundDocumentScroll, true);
    }
    if (this._compositionEndTimer) clearTimeout(this._compositionEndTimer);
    this._compositionEndTimer = 0;
    this._completionUpdateFrame = 0;
    this._virtualScrollFrame = 0;
    this._paintFrame = 0;
    this._pageRenderFrame = 0;
    this._deferredPaint = null;
    this._clearOffsetMap();
    if (this._sourceFormatTimer) clearTimeout(this._sourceFormatTimer);
    this._sourceFormatTimer = 0;
    this._idleFormattedKey = "";
  }
  attributeChangedCallback(name, oldValue, newValue) {
    if (oldValue === newValue) return;
    if (name === "value") {
      const nextDefault = normalizeLineEndings(newValue ?? "");
      if (!this._hasConnected) { this._value = nextDefault; this._defaultValue = nextDefault; return; }
      const wasDirty = this._dirty;
      this._defaultValue = nextDefault;
      if (!wasDirty) this._setValueInternal(nextDefault, { source: "attribute", silent: true, recordUndo: false, preserveSelection: false });
      else {
        const oldDirty = this._dirty;
        this._dirty = this._value !== this._defaultValue;
        if (oldDirty !== this._dirty) this._dispatch("md-dirty-change", { dirty: this._dirty });
      }
      return;
    }
    if (!this._hasConnected) return;
    const restoreFocus = (name === "mode" || name === "readonly") && (this._focusWithin || Boolean(this._shadow.activeElement));
    if ((name === "disabled" || name === "readonly") && (this.disabled || this.readonly)) this._closeCompletion();
    this._syncAttributesToControls();
    if (name === "disabled" && this.disabled) this.blur();
    if (["mode", "disabled", "readonly"].includes(name)) this._renderAll({ restoreSelection: !this.disabled, force: true });
    if (["required", "disabled", "readonly", "maxlength", "minlength"].includes(name)) { this._updateFormValue(); this._updateValidity(); }
    if (restoreFocus && !this.disabled) this._focusEditable();
  }

  get value() { return this._value; }
  set value(next) { this._setValueInternal(next, { source: "api", silent: false, recordUndo: false }); }
  get defaultValue() { return this._defaultValue; }
  set defaultValue(next) { this._defaultValue = normalizeLineEndings(next ?? ""); this.setAttribute("value", this._defaultValue); }
  get name() { return this.getAttribute("name") ?? ""; }
  set name(v) { v == null ? this.removeAttribute("name") : this.setAttribute("name", String(v)); }
  get label() { return this.getAttribute("label") ?? ""; }
  set label(v) { v == null ? this.removeAttribute("label") : this.setAttribute("label", String(v)); }
  get placeholder() { return this.getAttribute("placeholder") ?? DEFAULTS.placeholder; }
  set placeholder(v) { v == null ? this.removeAttribute("placeholder") : this.setAttribute("placeholder", String(v)); }
  get syntaxTheme() { return this.getAttribute("data-syntax-theme") || "base16-ocean.dark"; }
  get maxUrlLength() {
    const parsed = Number(this.getAttribute("data-max-url-length"));
    return Number.isInteger(parsed) && parsed >= 0 ? parsed : 4096;
  }
  get externalLinks() { return this.getAttribute("data-external-links") !== "false"; }
  get mode() { const v = this.getAttribute("mode") ?? DEFAULTS.mode; return ["live", "source"].includes(v) ? v : DEFAULTS.mode; }
  set mode(v) { v == null ? this.removeAttribute("mode") : this.setAttribute("mode", String(v)); }
  get markdownFlavor() { const v = this.getAttribute("markdown-flavor") ?? DEFAULTS.markdownFlavor; return ["gfm", "commonmark"].includes(v) ? v : DEFAULTS.markdownFlavor; }
  set markdownFlavor(v) { v == null ? this.removeAttribute("markdown-flavor") : this.setAttribute("markdown-flavor", String(v)); }
  get tabBehavior() { const v = this.getAttribute("tab-behavior") ?? DEFAULTS.tabBehavior; return ["accessibility-first", "editor-first"].includes(v) ? v : DEFAULTS.tabBehavior; }
  set tabBehavior(v) { v == null ? this.removeAttribute("tab-behavior") : this.setAttribute("tab-behavior", String(v)); }
  get indentString() { return normalizeIndentAttribute(this.getAttribute("indent-string") ?? DEFAULTS.indentString); }
  set indentString(v) { this.setAttribute("indent-string", v === "\t" ? "tab" : String(v)); }
  get disabled() { return this.hasAttribute("disabled") || this._formDisabled; }
  set disabled(v) { this.toggleAttribute("disabled", Boolean(v)); }
  get readonly() { return this.hasAttribute("readonly"); }
  set readonly(v) { this.toggleAttribute("readonly", Boolean(v)); }
  get required() { return this.hasAttribute("required"); }
  set required(v) { this.toggleAttribute("required", Boolean(v)); }
  get dirty() { return this._dirty; }
  get selectionStart() { return this._getCurrentSelection().start; }
  set selectionStart(v) { this.setSelectionRange(v, this.selectionEnd); }
  get selectionEnd() { return this._getCurrentSelection().end; }
  set selectionEnd(v) { this.setSelectionRange(this.selectionStart, v); }
  get validationMessage() { return this._internals?.validationMessage || this._validationMessage || ""; }
  get validity() { return this._internals?.validity ?? this._fallbackValidity(); }
  get willValidate() { return this._internals?.willValidate ?? !this.disabled; }

  focus(options) { this._focusEditable(options); }
  blur() { this._focusWithin = false; this._shadow?.activeElement?.blur?.(); this._sourceTextarea?.blur(); this._liveEditor?.blur(); this._hideSelectionToolbar?.(); this._formatWhenIdle(); }
  select() { this.setSelectionRange(0, this._value.length); }
  setSelectionRange(start, end, direction = "none") {
    const s = clamp(Number(start) || 0, 0, this._value.length);
    const e = clamp(Number(end) || 0, 0, this._value.length);
    this._selection = { start: s, end: e, direction };
    this._structuredSelection = (!this._isSourceActive() && s !== e) ? { start: s, end: e, direction, label: "selection" } : null;
    if (this._sourceTextarea && this._isSourceActive()) this._sourceTextarea.setSelectionRange(s, e, direction);
    if (this._liveEditor && !this._isSourceActive()) { this._ignoreSelectionChangeCount = 2; this._restoreLiveSelection(this._selection); }
    this._emitSelectionChange();
    this._scheduleCompletionUpdate();
    this._syncSourceForm();
  }
  exec(actionId, args) { const result = this._runAction(actionId, args, { source: "api", apply: true }); return Boolean(result?.ok); }
  registerAction(action) {
    if (!action || typeof action.id !== "string" || typeof action.run !== "function") throw new TypeError("registerAction(action) requires an action with string id and run(ctx,args).");
    this._actions.set(action.id, { group: "Custom", visibleInSlash: false, aliases: [], keywords: [], ...action });
  }
  unregisterAction(actionId) { this._actions.delete(actionId); }
  registerCompletionProvider(provider) {
    if (!provider || typeof provider.id !== "string" || typeof provider.match !== "function" || typeof provider.getItems !== "function" || typeof provider.apply !== "function") throw new TypeError("Completion provider requires id, match, getItems, apply.");
    this._providers.set(provider.id, { priority: 0, triggers: [], ...provider });
  }
  unregisterCompletionProvider(providerId) { this._providers.delete(providerId); if (this._completion.providerId === providerId) this._closeCompletion(); }
  getHTML() { return renderMarkdown(this._value, this._rendererOptions()); }
  getText() { return textFromMarkdown(this._value, this._rendererOptions()); }
  getMarkdown() { return this._value; }
  setMarkdown(markdown) { this.value = markdown; }
  getPlainText() { return this.getText(); }
  getSelectionMarkdown() { const sel = this._getCurrentSelection(); return this._value.slice(Math.min(sel.start, sel.end), Math.max(sel.start, sel.end)); }
  insertMarkdown(markdown) { return this.exec("editor.insertText", { text: markdown }); }
  canExec(actionId, args) { const action = this._actions.get(actionId); if (!action) return false; const ctx = this._getContext(); if (ctx.mode === "disabled" && !action.viewSafe) return false; if (ctx.mode === "readonly" && !action.readonlySafe && !action.viewSafe) return false; return !action.when || action.when(ctx, args); }
  getCurrentBlock() { const sel = this._getCurrentSelection(); return this._findBlockAtOffset(sel.start) || null; }
  getSelectedBlocks() { const sel = this._getCurrentSelection(); const start = Math.min(sel.start, sel.end); const end = Math.max(sel.start, sel.end); return this._getBlocks().filter(block => block.to >= start && block.from <= end); }
  getActiveMarks() { return this._getActiveStateIds(this._getContext()); }
  find(query, options = {}) { return this._findText(query, options); }
  replace(query, replacement, options = {}) { return this._replaceText(query, replacement, { ...options, all: false }); }
  replaceAll(query, replacement, options = {}) { return this._replaceText(query, replacement, { ...options, all: true }); }
  commit() { const old = this._dirty; this._defaultValue = this._value; this._dirty = false; this._dispatch("md-change", { value: this._value }); if (old) this._dispatch("md-dirty-change", { dirty: false }); }
  reset() { this._setValueInternal(this._defaultValue, { source: "api", recordUndo: true }); this.setSelectionRange(0, 0); this._dirty = false; this._dispatch("md-dirty-change", { dirty: false }); }
  checkValidity() { this._updateValidity(); return this._internals ? this._internals.checkValidity() : this._fallbackValidity().valid; }
  reportValidity() { this._validationVisible = true; this._updateValidity(); return this._internals ? this._internals.reportValidity() : this.checkValidity(); }
  setCustomValidity(message) { this._customValidityMessage = String(message ?? ""); this._updateValidity(); }

  _upgradeProperties() {
    for (const prop of ["value", "defaultValue", "name", "label", "placeholder", "mode", "markdownFlavor", "tabBehavior", "indentString", "disabled", "readonly", "required"]) {
      if (Object.prototype.hasOwnProperty.call(this, prop)) { const value = this[prop]; delete this[prop]; this[prop] = value; }
    }
  }

  _renderShell() {
    const stylesheet = new URL("./writemark.css", import.meta.url);
    stylesheet.search = new URL(import.meta.url).search;
    this._shadow.innerHTML = `
      <link rel="stylesheet" href="${escapeAttribute(stylesheet.href)}" />
      <div class="container" part="container">
        <label class="label" part="label" id="${this._ids.label}" for="${this._ids.source}"></label>
        <div class="workspace">
          <div class="editor-shell" part="editor">
            <div class="live-editor" part="live-editor" id="${this._ids.live}" role="textbox" aria-multiline="true" tabindex="0" aria-controls="${this._ids.completion}" aria-expanded="false" aria-autocomplete="list" aria-describedby="${this._ids.validation}"></div>
            <textarea part="textarea" id="${this._ids.source}" aria-controls="${this._ids.completion}" aria-expanded="false" aria-autocomplete="list" aria-describedby="${this._ids.validation}" rows="12"></textarea>
            <div class="completion-popup" part="completion-popup" id="${this._ids.completion}" role="listbox" hidden></div>
            <div class="selection-toolbar" part="selection-toolbar" id="${this._ids.toolbar}" role="toolbar" aria-label="Formatting" hidden></div>
          </div>
        </div>
        <div class="validation" part="error" id="${this._ids.validation}"></div>
        <div class="sr-only" part="status" id="${this._ids.status}" aria-live="polite" aria-atomic="true"></div>
      </div>`;
    this._label = this._shadow.querySelector(".label");
    this._liveEditor = this._shadow.querySelector(".live-editor");
    this._sourceTextarea = this._shadow.querySelector("textarea");
    this._completionPopup = this._shadow.querySelector(".completion-popup");
    this._selectionToolbar = this._shadow.querySelector(".selection-toolbar");
    this._validation = this._shadow.querySelector(".validation");
    this._status = this._shadow.querySelector(".sr-only");
    this._buildSelectionToolbar();
  }

  _buildSelectionToolbar() {
    if (!this._selectionToolbar) return;
    this._selectionToolbar.innerHTML = this._selectionToolbarActions
      .map(item => `<button type="button" part="selection-toolbar-button" data-action="${escapeAttribute(item.actionId)}" title="${escapeAttribute(item.label)}" aria-label="${escapeAttribute(item.label)}" tabindex="-1">${item.html}</button>`)
      .join("");
    // Keep focus/selection in the editor when interacting with the toolbar.
    this._selectionToolbar.addEventListener("mousedown", event => event.preventDefault());
    this._selectionToolbar.addEventListener("click", event => {
      const button = event.target.closest?.("button[data-action]");
      if (!button) return;
      event.preventDefault();
      this._runAction(button.dataset.action, undefined, { source: "toolbar", apply: true });
      this._updateSelectionToolbar();
    });
  }

  _bindEvents() {
    this._sourceTextarea.addEventListener("beforeinput", () => this._beforeInputSnapshot = this._snapshot());
    this._sourceTextarea.addEventListener("input", event => this._onSourceInput(event));
    this._sourceTextarea.addEventListener("change", () => this._dispatch("md-change", { value: this._value }));
    this._sourceTextarea.addEventListener("keydown", event => this._onKeyDown(event));
    this._sourceTextarea.addEventListener("keyup", event => this._onNavigationKey(event));
    this._sourceTextarea.addEventListener("click", () => this._onSelectionChanged());
    this._sourceTextarea.addEventListener("select", () => this._onSelectionChanged());
    this._sourceTextarea.addEventListener("paste", event => this._onPaste(event));
    this._sourceTextarea.addEventListener("drop", event => this._onDrop(event));
    this._sourceTextarea.addEventListener("compositionstart", () => this._onCompositionStart());
    this._sourceTextarea.addEventListener("compositionend", event => this._onCompositionEnd(event));

    this._liveEditor.addEventListener("focus", () => { if (this._selection.start > this._value.length) this._selection = { start: 0, end: 0, direction: "none" }; }, true);
    this._liveEditor.addEventListener("keydown", event => this._onKeyDown(event));
    this._liveEditor.addEventListener("keyup", event => this._onNavigationKey(event));
    this._liveEditor.addEventListener("beforeinput", event => this._onLiveBeforeInput(event));
    this._liveEditor.addEventListener("input", event => this._onLiveInput(event));
    this._liveEditor.addEventListener("click", event => this._onLiveClick(event));
    this._liveEditor.addEventListener("mousedown", event => this._onLiveMouseDown(event));
    this._liveEditor.addEventListener("mouseup", () => this._onSelectionChanged());
    this._liveEditor.addEventListener("scroll", () => this._onLiveScroll());
    this._bindDocumentScroll();
    this._liveEditor.addEventListener("copy", event => this._onLiveCopy(event));
    this._liveEditor.addEventListener("cut", event => this._onLiveCut(event));
    this._liveEditor.addEventListener("paste", event => this._onPaste(event));
    this._liveEditor.addEventListener("drop", event => this._onDrop(event));
    this._liveEditor.addEventListener("compositionstart", () => this._onCompositionStart());
    this._liveEditor.addEventListener("compositionend", event => this._onCompositionEnd(event));

    this._completionPopup.addEventListener("mousedown", e => e.preventDefault());
    this._completionPopup.addEventListener("click", e => { const item = e.target.closest("[data-index]"); if (!item) return; const index = Number(item.dataset.index); if (this._completion.items[index]?.disabled) return; this._completion.activeIndex = index; this._acceptCompletion("pointer"); });
    this._label.addEventListener("click", event => { event.preventDefault(); this._focusEditable(); });
    this._shadow.addEventListener("focusin", () => { this._focusWithin = true; this._syncSourceForm(); });
    this._shadow.addEventListener("focusout", () => { queueMicrotask(() => { this._focusWithin = Boolean(this._shadow.activeElement); if (!this._focusWithin) { this._hideSelectionToolbar?.(); this._formatWhenIdle(); } }); });
    this._shadow.addEventListener("selectionchange", () => this._onSelectionChanged?.());
  }

  _syncAttributesToControls() {
    if (!this._sourceTextarea) return;
    this._label.textContent = this.label; this._label.hidden = !this.label;
    this._sourceTextarea.placeholder = this.placeholder;
    this._sourceTextarea.disabled = this.disabled;
    this._sourceTextarea.readOnly = this.readonly;
    this._sourceTextarea.required = this.required;
    this._sourceTextarea.name = this.name;
    this._liveEditor.setAttribute("aria-readonly", this.readonly ? "true" : "false");
    this._liveEditor.setAttribute("aria-disabled", this.disabled ? "true" : "false");
    this._liveEditor.contentEditable = this._liveSelectionAPI === false
      ? "false"
      : this._lineEditable();
    this._liveEditor.tabIndex = this.disabled ? -1 : 0;
    const maxLength = parseLengthConstraint(this.getAttribute("maxlength"));
    const minLength = parseLengthConstraint(this.getAttribute("minlength"));
    maxLength != null ? this._sourceTextarea.maxLength = maxLength : this._sourceTextarea.removeAttribute("maxlength");
    minLength != null ? this._sourceTextarea.minLength = minLength : this._sourceTextarea.removeAttribute("minlength");
    const rawSpellcheck = this.getAttribute("spellcheck");
    const spellcheck = rawSpellcheck == null || rawSpellcheck === "" || rawSpellcheck === "true";
    this._sourceTextarea.spellcheck = spellcheck;
    this._liveEditor.spellcheck = spellcheck;
    const ariaLabel = this.getAttribute("aria-label"); const ariaLabelledby = this.getAttribute("aria-labelledby");
    for (const el of [this._sourceTextarea, this._liveEditor]) {
      if (ariaLabel) el.setAttribute("aria-label", ariaLabel); else el.removeAttribute("aria-label");
      if (ariaLabelledby) el.setAttribute("aria-labelledby", ariaLabelledby); else if (this.label) el.setAttribute("aria-labelledby", this._ids.label); else el.removeAttribute("aria-labelledby");
      const dir = this.getAttribute("dir");
      if (dir) el.dir = dir; else el.removeAttribute("dir");
    }
  }

  _getActiveStateIds(ctx) {
    const ids = [];
    const block = ctx.block || {};
    if (block.kind === "heading" && block.heading) ids.push(`block.heading.${block.heading.level}`);
    if (block.kind === "bullet-list-item") ids.push("block.bulletList");
    if (block.kind === "ordered-list-item") ids.push("block.orderedList");
    if (block.kind === "task-list-item") ids.push("block.taskList");
    if (block.kind === "blockquote") ids.push("block.blockquote");
    if (block.kind === "fenced-code") ids.push("block.codeFence");
    if (block.kind === "table") ids.push("block.table");
    const line = ctx.currentLine?.text ?? "";
    const pos = clamp(ctx.selectionStart - (ctx.currentLine?.start ?? 0), 0, line.length);
    const before = line.slice(0, pos); const after = line.slice(pos);
    if ((before.match(/\*\*/g) || []).length % 2 === 1 && after.includes("**")) ids.push("inline.bold");
    if ((before.match(/(?<!\*)\*(?!\*)/g) || []).length % 2 === 1 && /(?<!\*)\*(?!\*)/.test(after)) ids.push("inline.italic");
    if ((before.match(/(?<!\\)`/g) || []).length % 2 === 1 && /(?<!\\)`/.test(after)) ids.push("inline.code");
    if ((before.match(/_/g) || []).length % 2 === 1 && after.includes("_")) ids.push("inline.underline");
    if ((before.match(/~/g) || []).length % 2 === 1 && after.includes("~")) ids.push("inline.strikethrough");
    if ((before.match(/\^/g) || []).length % 2 === 1 && after.includes("^")) ids.push("inline.superscript");
    if ((before.match(/==/g) || []).length % 2 === 1 && after.includes("==")) ids.push("inline.highlight");
    if ((before.match(/#/g) || []).length % 2 === 1 && after.includes("#")) ids.push("inline.secret");
    if (/\[[^\]]*$/.test(before) && /\]\([^)]+\)/.test(after)) ids.push("inline.link");
    return ids;
  }
  _findText(query, options = {}) {
    const q = String(query ?? ""); if (!q) return null;
    const hay = options.caseSensitive ? this._value : this._value.toLowerCase();
    const needle = options.caseSensitive ? q : q.toLowerCase();
    const from = clamp(Number(options.from ?? this.selectionEnd ?? 0), 0, this._value.length);
    let index = hay.indexOf(needle, from);
    if (index === -1 && options.wrap !== false) index = hay.indexOf(needle, 0);
    if (index === -1) return null;
    this.setSelectionRange(index, index + q.length, "forward");
    this._announce("Match found.");
    return { start: index, end: index + q.length, text: this._value.slice(index, index + q.length) };
  }
  _replaceText(query, replacement, options = {}) {
    const q = String(query ?? ""); if (!q) return 0;
    const repl = normalizeLineEndings(replacement ?? "");
    const source = options.caseSensitive ? this._value : this._value.toLowerCase();
    const needle = options.caseSensitive ? q : q.toLowerCase();
    const changes = [];
    if (options.all) {
      let i = 0;
      while ((i = source.indexOf(needle, i)) !== -1) { changes.push({ from: i, to: i + q.length, insert: repl }); i += q.length || 1; }
    } else {
      const sel = this._getCurrentSelection();
      const selected = this._value.slice(Math.min(sel.start, sel.end), Math.max(sel.start, sel.end));
      const matchSelected = (options.caseSensitive ? selected : selected.toLowerCase()) === needle;
      const found = matchSelected ? { start: Math.min(sel.start, sel.end), end: Math.max(sel.start, sel.end) } : this._findText(q, options);
      if (found) changes.push({ from: found.start, to: found.end, insert: repl });
    }
    if (!changes.length) return 0;
    const first = changes[0].from + repl.length;
    this._applyTransaction({ changes, selectionAfter: { start: first, end: first, direction: "none" }, actionId: options.all ? "editor.replaceAll" : "editor.replace", undoGroup: "replace", source: "api" }, { source: "api" });
    return changes.length;
  }

  _parseOptions() { return { markdownFlavor: this.markdownFlavor }; }
  _referenceDefinitions() {
    if (this._referenceCacheValue === this._value && this._referenceCache) return this._referenceCache;
    this._referenceCacheValue = this._value;
    this._referenceCache = extractReferenceDefinitions(this._value).references;
    return this._referenceCache;
  }
  _rendererOptions() {
    return {
      ...this._parseOptions(),
      allowRawHtml: false,
      sanitize: true,
      linkTarget: DEFAULTS.linkTarget,
      references: this._referenceDefinitions(),
      syntaxTheme: this.syntaxTheme,
      maxUrl: this.maxUrlLength,
      external: this.externalLinks,
    };
  }
  readyPage() {
    return pageModule;
  }
  _ensurePageRenderer() {
    if (pageModule || this._pageLoad) return;
    this._pageLoad = loadPageModule().then(() => {
      this._pageLoad = null;
      this._applyPageRenderer();
    }).catch(() => {
      this._pageLoad = null;
    });
  }
  _applyPageRenderer() {
    if (!this.isConnected || !pageModule || this._userEdited) return;
    if (this._isComposing || this._holdAccentTarget || this._deferredPaint || this._paintFrame || this._pointerSelection) {
      this._schedulePageRenderer();
      return;
    }
    const focused = Boolean(this._focusWithin && this._shadow?.activeElement);
    this._renderAll({ restoreSelection: focused, force: true });
  }
  _schedulePageRenderer() {
    if (this._pageRenderFrame) return;
    this._pageRenderFrame = requestAnimationFrame(() => {
      this._pageRenderFrame = 0;
      this._applyPageRenderer();
    });
  }
  _inlineHtml(markdown) {
    const text = String(markdown ?? "");
    const html = renderInline(text, this._rendererOptions());
    if (html) return html;
    return text ? escapeHtml(text) : "<br>";
  }
  _usesInlinePaint(editable) {
    if (!editable) return false;
    if (editable.dataset.editable === "cell") return true;
    return ["paragraph", "heading", "blockquote", "bullet-list-item", "ordered-list-item", "task-list-item", "horizontal-rule", "blank"].includes(editable.dataset.kind);
  }
  _textFromHtml(html) {
    const template = document.createElement("template");
    template.innerHTML = String(html ?? "");
    return this._plainText(template.content);
  }
  _isEditingText(node) {
    let el = node?.nodeType === Node.ELEMENT_NODE ? node : node?.parentElement;
    while (el) {
      if (el.matches?.("button, .code-header, .line-numbers, svg, img, video")) return false;
      if (el.matches?.("[data-editable]")) return true;
      if (el.getAttribute?.("contenteditable") === "false") return false;
      el = el.parentElement;
    }
    return true;
  }
  _setValueInternal(next, opts = {}) {
    const previousValue = this._value;
    const value = normalizeLineEndings(next ?? ""); const before = this._snapshot(); const changed = value !== previousValue;
    this._value = value;
    if (this._sourceTextarea && !this._isLiveVisible() && this._sourceTextarea.value !== value) this._sourceTextarea.value = value;
    if (!opts.preserveSelection) this._selection = { start: clamp(this._selection.start, 0, value.length), end: clamp(this._selection.end, 0, value.length), direction: "none" };
    if (opts.recordUndo && changed) this._recordUndo(before, this._snapshot(), opts.undoGroup || opts.source || "api", { coalesce: false });
    if (changed || opts.force || !this._hasRenderedOnce) this._afterValueChanged({ source: opts.source || "api", silent: opts.silent, restoreSelection: opts.preserveSelection !== false, previousValue, changes: changed ? diffTextChange(previousValue, value) : [] });
  }
  _afterValueChanged({ source = "api", inputType = null, silent = false, restoreSelection = true, previousValue = null, changes = null, defer = false, editable = null } = {}) {
    this._selectAllLevel = 0;
    this._structuredSelection = null;
    if (["user", "keyboard", "paste", "pointer"].includes(source)) {
      this._validationVisible = true;
      this._userEdited = true;
    }
    this._updateFormValue(); this._updateValidity(); this._renderAll({ restoreSelection, previousValue, changes, defer, editable });
    const oldDirty = this._dirty; this._dirty = this._value !== this._defaultValue; if (oldDirty !== this._dirty) this._dispatch("md-dirty-change", { dirty: this._dirty });
    this._updateSelectionToolbar();
    if (!silent) this._dispatch("md-input", { value: this._value, source, inputType });
  }
  _renderAll({ restoreSelection = true, previousValue = null, changes = null, force = false, defer = false, editable = null } = {}) {
    if (!this._liveEditor) return;
    // Replacing the DOM (or writing the textarea value) ends a Mac accent menu
    // or IME session, so the preview can only land on the first candidate.
    if (this._isComposing || this._holdAccentTarget) {
      if (this._holdAccentTarget && this._isLiveVisible()) this._syncAccentMetadata(previousValue, changes);
      return;
    }
    const args = { restoreSelection, previousValue, changes, force };
    if (force) {
      this._cancelDeferredPaint();
      this._renderAllNow(args);
      return;
    }
    // Only a live key whose text is already in the line can wait. Undo, the
    // slash menu, and the toolbar paint now, because their DOM is still old.
    if (defer && this._canDeferLivePaint({ previousValue, changes }) && this._prepareDeferredEditable(editable, changes)) {
      if (this._deferredPaint?.previousValue == null) this._deferredPaint = { previousValue, restoreSelection };
      else this._deferredPaint.restoreSelection = restoreSelection;
      if (!this._paintFrame) this._paintFrame = requestAnimationFrame(() => this._flushDeferredPaint());
      return;
    }
    const pending = this._deferredPaint;
    this._cancelDeferredPaint();
    const merged = pending?.previousValue != null
      ? { restoreSelection, previousValue: pending.previousValue, changes: diffTextChange(pending.previousValue, this._value), force: false }
      : args;
    this._renderAllNow(merged);
  }
  _deferredPaintBlocked() {
    return this._isComposing || this._holdAccentTarget || this._accentMenuHoldingTarget();
  }
  _canDeferLivePaint({ previousValue = null, changes = null } = {}) {
    if (!this._isLiveVisible() || !this._hasRenderedOnce || previousValue == null || !changes?.length) return false;
    if (changes.length !== 1) return false;
    const change = changes[0];
    const insert = String(change.insert ?? "");
    if (insert.includes("\n")) return false;
    if (String(previousValue).slice(change.from, change.to).includes("\n")) return false;
    return true;
  }
  _cancelDeferredPaint() {
    if (this._paintFrame) cancelAnimationFrame(this._paintFrame);
    this._paintFrame = 0;
    this._deferredPaint = null;
    this._clearOffsetMap();
  }
  _clearOffsetMap() {
    this._offsetMap = [];
    this._paintedOffsets = new WeakMap();
    this._paintedChecks = new WeakMap();
  }
  _flushDeferredPaint() {
    this._paintFrame = 0;
    const pending = this._deferredPaint;
    this._deferredPaint = null;
    if (!pending) return;
    if (!this.isConnected) return;
    if (this._deferredPaintBlocked()) {
      this._deferredPaint = pending;
      this._paintFrame = requestAnimationFrame(() => this._flushDeferredPaint());
      return;
    }
    const previousValue = pending.previousValue;
    const caretMoved = this._sourceFormKey() !== (this._sourceFormKeyShown ?? "");
    this._renderAllNow({
      restoreSelection: pending.restoreSelection !== false,
      previousValue,
      changes: diffTextChange(previousValue, this._value),
      force: false,
      deferredFrame: true,
      caretMoved,
    });
  }
  // Attributes stay at the last paint. The map records the growth, and readers
  // use the map. The frame paint writes the real offsets after the map is cleared.
  _prepareDeferredEditable(editable, changes) {
    if (!editable?.isConnected || changes?.length !== 1) return false;
    const change = changes[0];
    const from = Number(change.from);
    const to = Number(change.to);
    if (!Number.isFinite(from) || !Number.isFinite(to) || to < from) return false;
    const delta = String(change.insert ?? "").length - (to - from);
    this._rememberPaintedOffsets();
    const paintedFrom = this._paintedOffset(editable, "from");
    const paintedTo = this._paintedOffset(editable, "to");
    if (!Number.isFinite(paintedFrom) || !Number.isFinite(paintedTo) || paintedTo < paintedFrom) return false;
    const current = mapPaintedRange(paintedFrom, paintedTo, this._offsetMap);
    const cell = editable.dataset.editable === "cell";
    if (cell ? from < current.from || to > current.to : from !== current.from || to !== current.to) return false;
    if (!delta) return true;
    if (current.to + delta < current.from) return false;
    this._noteOffsetDelta(paintedFrom, paintedTo, from, to, delta);
    return true;
  }
  _rememberPaintedOffsets() {
    const root = this._liveEditor;
    if (!root) return;
    for (const node of root.querySelectorAll("[data-from]")) {
      if (!node.hasAttribute("data-to") || this._paintedOffsets.has(node)) continue;
      const from = Number(node.dataset.from);
      const to = Number(node.dataset.to);
      if (!Number.isFinite(from) || !Number.isFinite(to)) continue;
      this._paintedOffsets.set(node, { from, to });
    }
    for (const box of root.querySelectorAll("[data-check-offset]")) {
      if (this._paintedChecks.has(box)) continue;
      const offset = Number(box.dataset.checkOffset);
      if (!Number.isFinite(offset)) continue;
      this._paintedChecks.set(box, offset);
    }
  }
  _paintedOffset(node, key) {
    const stored = this._paintedOffsets.get(node);
    if (stored && Number.isFinite(stored[key])) return stored[key];
    const raw = Number(node?.dataset?.[key]);
    return Number.isFinite(raw) ? raw : NaN;
  }
  _noteOffsetDelta(start, at, from, to, delta) {
    const index = this._offsetMap.findIndex(section => section.start === start && section.at === at);
    if (index === -1) {
      this._offsetMap.push({ start, at, from, to, delta });
      return;
    }
    const existing = this._offsetMap[index];
    existing.delta += delta;
    // Later sections were stored after the previous growth. Move the ones at
    // or past this edit by the added growth, or their replay lands short.
    const boundary = existing.to;
    for (let i = index + 1; i < this._offsetMap.length; i += 1) {
      const later = this._offsetMap[i];
      if (later.from >= boundary) {
        later.from += delta;
        later.to += delta;
      } else if (later.to > boundary) {
        later.to += delta;
      }
    }
  }
  _liveFrom(node) {
    const paintedTo = this._paintedOffset(node, "to");
    const paintedFrom = this._paintedOffset(node, "from");
    if (!Number.isFinite(paintedFrom)) return paintedFrom;
    return mapPaintedRange(paintedFrom, Number.isFinite(paintedTo) ? paintedTo : paintedFrom, this._offsetMap).from;
  }
  _liveTo(node) {
    const paintedFrom = this._paintedOffset(node, "from");
    const paintedTo = this._paintedOffset(node, "to");
    if (!Number.isFinite(paintedTo)) return paintedTo;
    return mapPaintedRange(Number.isFinite(paintedFrom) ? paintedFrom : paintedTo, paintedTo, this._offsetMap).to;
  }
  _liveCheckOffset(box) {
    const painted = this._paintedChecks.has(box) ? this._paintedChecks.get(box) : Number(box?.dataset?.checkOffset);
    if (!Number.isFinite(painted)) return painted;
    return mapPaintedOffset(painted, this._offsetMap);
  }
  _renderAllNow({ restoreSelection = true, previousValue = null, changes = null, force = false, deferredFrame = false, caretMoved = false } = {}) {
    this._clearOffsetMap();
    this._hasRenderedOnce = true;
    if (this._sourceTextarea && !this._isLiveVisible()) this._sourceTextarea.value = this._value;
    if (restoreSelection && this._isSourceActive() && this._sourceTextarea) {
      const sel = this._selection || { start: 0, end: 0, direction: "none" };
      this._sourceTextarea.setSelectionRange(sel.start, sel.end, sel.direction || "none");
    }
    if (this._isLiveVisible()) {
      this._renderLive({ previousValue, changes, force, deferredFrame, caretMoved });
      if (restoreSelection && !this._isSourceActive()) this._restoreLiveSelection(this._selection);
      this._armSpellcheck();
    }
  }
  _armSpellcheck() {
    const want = Boolean(this._sourceTextarea?.spellcheck);
    const active = want ? this._activeEditableFromSelection() : null;
    const editable = active?.dataset.kind === "code-fence" ? null : active;
    if (this._spellEditable && this._spellEditable !== editable && this._spellEditable.isConnected) {
      this._spellEditable.spellcheck = false;
    }
    if (active?.dataset.kind === "code-fence") active.spellcheck = false;
    if (editable) editable.spellcheck = true;
    this._spellEditable = editable || null;
  }
  _isLiveVisible() {
    return this.mode === "live";
  }
  _getBlocks() {
    if (this._blockCacheValue === this._value && this._blockCache) return this._blockCache;
    const blocks = parseBlocks(this._value, this._parseOptions());
    this._setBlockCache(blocks);
    return blocks;
  }
  _setBlockCache(blocks) {
    assignHeadingIds(blocks);
    this._blockCacheValue = this._value;
    this._blockCache = blocks;
  }
  _blocksForRender(previousValue, changes) {
    const incremental = this._tryIncrementalBlocks(previousValue, changes);
    if (incremental) {
      this._setBlockCache(incremental);
      return incremental;
    }
    const blocks = parseBlocks(this._value, this._parseOptions());
    this._setBlockCache(blocks);
    return blocks;
  }
  _renderLive({ previousValue = null, changes = null, force = false, deferredFrame = false, caretMoved = false } = {}) {
    const previousBlocks = this._liveBlocks || [];
    const blocks = this._blocksForRender(previousValue, changes);
    if (this._shouldVirtualize(blocks)) {
      const focus = this._selectionFocusOffset();
      let painted = this._renderLiveVirtual(blocks, { anchorOffset: focus, force });
      // The caret moved while this window was waiting. Paint it so the line
      // entered is source and the line left is formatted.
      if (!painted && deferredFrame && caretMoved) {
        painted = this._renderLiveVirtual(blocks, { anchorOffset: focus, force: true });
      }
      if (!painted) this._syncVirtualWindow(blocks, changes);
      this._liveBlocks = blocks;
      this._rebuildLiveIndex();
      if (painted) this._sourceFormKeyShown = this._sourceFormKey();
      return;
    }
    this._liveEditor.classList.remove("md-virtual");
    this._virtualState = { ...this._virtualState, active: false, start: 0, end: blocks.length, total: blocks.length };
    if (deferredFrame && this._paintDeferredSourceFrame(previousBlocks, blocks, changes, previousValue, caretMoved)) {
      this._liveBlocks = blocks;
      this._rebuildLiveIndex();
      this._sourceFormKeyShown = this._sourceFormKey();
      return;
    }
    const patched = !force && this._tryPatchLiveBlocks(previousBlocks, blocks, changes, previousValue);
    if (!patched) this._renderLiveFull(blocks);
    this._liveBlocks = blocks;
    this._rebuildLiveIndex();
    this._sourceFormKeyShown = this._sourceFormKey();
  }
  // The caret line is already source text. Update offsets in place. When the
  // caret has moved, format the line it left and show source on the new one.
  _paintDeferredSourceFrame(previousBlocks, blocks, changes, previousValue, caretMoved) {
    if (!previousValue || !changes?.length || !previousBlocks.length || !this._liveEditor) return false;
    const children = Array.from(this._liveEditor.children).filter(node => !node.classList?.contains("md-virtual-spacer"));
    if (children.length !== previousBlocks.length || children.length !== blocks.length) return false;
    const spans = changedSpans(changes);
    if (!spans) return false;
    const oldRange = this._blockRangeForSourceRange(previousBlocks, spans.oldStart, spans.oldEnd, previousValue.length);
    const newRange = this._blockRangeForSourceRange(blocks, spans.newStart, spans.newEnd, this._value.length);
    if (!oldRange || !newRange || (oldRange.end - oldRange.start) !== (newRange.end - newRange.start)) return false;
    // Highlighted code is not source text. Rebuild it, or the spans the key
    // edited stay on screen. A code fence has no source-form key, so leaving
    // it would otherwise omit it from the replace list too.
    const fresh = this._indexesNeedingFreshDom(blocks, newRange);
    if (!caretMoved) {
      if (fresh.length && !this._replaceLiveBlockNodes(blocks, fresh)) return false;
      if (this._syncLiveMetadataFrom(blocks, newRange.start)) return true;
      if ((newRange.end - newRange.start) !== 1) return false;
      if (!this._replaceLiveBlockNodes(blocks, [newRange.start])) return false;
      return this._syncLiveMetadataFrom(blocks, newRange.end);
    }
    const previousIndexes = this._sourceFormBlockIndexes(this._sourceFormKeyShown ?? "", previousBlocks) || [];
    const freshIndexes = this._sourceFormBlockIndexes(this._sourceFormKeyFor(blocks, false), blocks) || [];
    const replace = [...new Set([...previousIndexes, ...freshIndexes, ...fresh])].filter(index => index >= 0 && index < blocks.length);
    if (!replace.length) return this._syncLiveMetadataFrom(blocks, newRange.start);
    if (!this._replaceLiveBlockNodes(blocks, replace)) return false;
    return this._syncLiveMetadataFrom(blocks, newRange.start);
  }
  _indexesNeedingFreshDom(blocks, range) {
    const indexes = [];
    if (!range) return indexes;
    for (let index = range.start; index < range.end; index += 1) {
      if (blocks[index]?.type === "code-fence") indexes.push(index);
    }
    return indexes;
  }
  _replaceLiveBlockNodes(blocks, indexes) {
    const children = Array.from(this._liveEditor.children).filter(node => !node.classList?.contains("md-virtual-spacer"));
    const prepared = [];
    for (const index of indexes) {
      if (!blocks[index] || !children[index]) return false;
      const fresh = Array.from(this._liveFragmentForBlocks([blocks[index]]).children);
      if (fresh.length !== 1) return false;
      prepared.push({ node: children[index], fresh: fresh[0] });
    }
    for (const item of prepared) item.node.replaceWith(item.fresh);
    return true;
  }
  _renderLiveFull(blocks) {
    const html = blocks.map(block => this._renderLiveBlock(block)).join("");
    this._liveEditor.innerHTML = html || `<div class="live-placeholder">${escapeHtml(this.placeholder)}</div>`;
  }
  _tryPatchLiveBlocks(previousBlocks, blocks, changes, previousValue) {
    if (!previousValue || !changes?.length || !previousBlocks.length || !this._liveEditor?.children?.length || this._virtualState.active) return false;
    const children = Array.from(this._liveEditor.children);
    if (children.length !== previousBlocks.length) return false;
    const spans = changedSpans(changes);
    if (!spans) return false;
    const oldRange = this._expandedBlockRange(previousBlocks, spans.oldStart, spans.oldEnd, previousValue.length);
    const newRange = this._expandedBlockRange(blocks, spans.newStart, spans.newEnd, this._value.length);
    if (!oldRange || !newRange) return false;
    if ((oldRange.end - oldRange.start) > 250 || (newRange.end - newRange.start) > 250) return false;
    const fragment = this._liveFragmentForBlocks(blocks.slice(newRange.start, newRange.end));
    const reference = children[oldRange.end] || null;
    for (const node of children.slice(oldRange.start, oldRange.end)) node.remove();
    this._liveEditor.insertBefore(fragment, reference);
    if (!this._syncLiveMetadataFrom(blocks, newRange.end)) {
      this._renderLiveFull(blocks);
    }
    return true;
  }
  _expandedBlockRange(blocks, start, end, valueLength = this._value.length) {
    if (!blocks.length) return { start: 0, end: 0 };
    const range = this._blockRangeForSourceRange(blocks, start, end, valueLength);
    if (!range) return null;
    return {
      start: clamp(range.start - 1, 0, blocks.length),
      end: clamp(range.end + 1, 0, blocks.length),
    };
  }
  _blockRangeForSourceRange(blocks, start, end, valueLength = this._value.length) {
    if (!blocks.length) return { start: 0, end: 0 };
    const rangeStart = clamp(Number(start) || 0, 0, valueLength);
    const rangeEnd = clamp(Number(end) || rangeStart, rangeStart, Math.max(rangeStart, valueLength));
    let first = -1;
    let last = -1;
    const pointRange = rangeStart === rangeEnd;
    for (let i = 0; i < blocks.length; i += 1) {
      const block = blocks[i];
      const blockEnd = Math.max(block.newlineEnd ?? block.to ?? block.from, block.to ?? block.from, block.from);
      if (first === -1 && (pointRange ? blockEnd >= rangeStart : blockEnd > rangeStart)) first = i;
      if (first !== -1 && block.from <= rangeEnd) last = i;
      if (first !== -1 && block.from > rangeEnd) break;
    }
    if (first === -1) first = Math.max(0, blocks.length - 1);
    if (last === -1 || last < first) last = first;
    return { start: first, end: Math.min(blocks.length, last + 1) };
  }
  _liveFragmentForBlocks(blocks) {
    const template = document.createElement("template");
    template.innerHTML = blocks.map(block => this._renderLiveBlock(block)).join("");
    return template.content;
  }
  _syncLiveMetadata(blocks) {
    return this._syncLiveMetadataFrom(blocks, 0);
  }
  _syncLiveMetadataFrom(blocks, startIndex) {
    const children = Array.from(this._liveEditor.children).filter(node => !node.classList?.contains("md-virtual-spacer"));
    if (children.length !== blocks.length) return false;
    const start = clamp(startIndex, 0, blocks.length);
    for (let i = start; i < blocks.length; i += 1) {
      if (!this._syncLiveBlockNodeMetadata(children[i], blocks[i])) return false;
    }
    return true;
  }
  _setEditableMetadata(el, from, to, editable = this._lineEditable(), spellcheck = undefined) {
    if (!el) return false;
    el.dataset.from = String(from);
    el.dataset.to = String(to);
    if (el.hasAttribute("contenteditable")) el.contentEditable = editable;
    if (spellcheck != null && el.hasAttribute("spellcheck")) el.setAttribute("spellcheck", spellcheck);
    return true;
  }
  _syncLiveBlockNodeMetadata(node, block) {
    if (!node || !block) return false;
    const blockFrom = block.from ?? block.line?.start ?? 0;
    const blockTo = block.to ?? block.line?.end ?? blockFrom;
    if (node.dataset) {
      node.dataset.kind = block.type;
      node.dataset.from = String(blockFrom);
      node.dataset.to = String(blockTo);
    }
    if (block.type !== "table" && node.matches?.("[data-source-form]") && !node.querySelector?.(".md-code-line")) {
      if (block.type === "heading" && block.heading?.id) node.id = block.heading.id;
      if (block.type === "code-fence") {
        node.dataset.language = String(block.language || "");
        return this._setEditableMetadata(node, block.from, block.to, this._lineEditable(), "false");
      }
      return this._setEditableMetadata(node, block.from, block.to);
    }
    if (block.type === "table") {
      const cells = Array.from(node.querySelectorAll('.md-cell[data-editable="cell"]'));
      const cols = Math.max(block.header.cells.length, ...block.rows.map(row => row.cells.length), 1);
      const bodyRows = block.rows.length ? block.rows : [{ cells: Array.from({ length: cols }, () => ({ text: "", from: block.delimiter.end, to: block.delimiter.end })) }];
      const expected = [
        ...Array.from({ length: cols }, (_, col) => ({ cell: block.header.cells[col] ?? { from: block.header.end, to: block.header.end }, row: -1, col })),
        ...bodyRows.flatMap((row, rowIndex) => Array.from({ length: cols }, (_, col) => ({ cell: row.cells[col] ?? { from: row.end, to: row.end }, row: rowIndex, col }))),
      ];
      if (cells.length !== expected.length) return false;
      cells.forEach((cell, index) => {
        const meta = expected[index];
        cell.dataset.row = String(meta.row);
        cell.dataset.col = String(meta.col);
        this._setEditableMetadata(cell, meta.cell.from, meta.cell.to);
      });
      return true;
    }
    if (block.type === "code-fence") {
      const lines = Array.from(node.querySelectorAll(":scope > .md-code-lines > .md-code-line"));
      const expected = codeLineRanges(block);
      if (lines.length !== expected.length) return false;
      const open = node.querySelector(':scope > [data-fence-role="open"]');
      const close = node.querySelector(':scope > [data-fence-role="close"]');
      if (Boolean(open) !== Boolean(block.opening) || Boolean(close) !== Boolean(block.closing)) return false;
      node.dataset.language = String(block.language || "");
      if (open && !this._setEditableMetadata(open, block.opening.start, block.opening.end, this._lineEditable(), "false")) return false;
      if (close && !this._setEditableMetadata(close, block.closing.start, block.closing.end, this._lineEditable(), "false")) return false;
      lines.forEach((line, index) => {
        this._setEditableMetadata(line, expected[index].from, expected[index].to, this._lineEditable(), "false");
      });
      return true;
    }
    if (block.type === "task-list-item") {
      const source = node.querySelector("[data-editable]");
      const checkbox = node.querySelector("[data-task-checkbox]");
      if (!source) return false;
      if (checkbox) checkbox.dataset.checkOffset = String(block.line.start + block.list.indent.length + `${block.list.marker} [`.length);
      const range = contentSourceRange(block.line, block.list.contentStart, block.list.content);
      return this._setEditableMetadata(source, range.from, range.to);
    }
    if (block.type === "heading") {
      const heading = node.matches?.(".md-heading") ? node : node.querySelector?.(".md-heading");
      if (!heading) return false;
      heading.id = block.heading.id;
      const editable = heading.matches?.("[data-editable]") ? heading : heading.querySelector?.("[data-editable]");
      if (!editable) return false;
      const range = contentSourceRange(block.line, block.heading.contentStart, block.heading.content);
      return this._setEditableMetadata(editable, range.from, range.to);
    }
    if (block.type === "blockquote" || block.type === "bullet-list-item" || block.type === "ordered-list-item") {
      const editable = node.matches?.("[data-editable]") ? node : node.querySelector?.("[data-editable]");
      const parsed = block.type === "blockquote" ? block.quote : block.list;
      if (!editable || !block.line || !parsed) return false;
      const range = contentSourceRange(block.line, parsed.contentStart, parsed.content);
      return this._setEditableMetadata(editable, range.from, range.to);
    }
    const editable = node.matches?.("[data-editable]") ? node : node.querySelector?.("[data-editable]");
    if (editable && block.line) return this._setEditableMetadata(editable, block.line.start, block.line.end);
    return block.type === "horizontal-rule";
  }
  _lineAt(value, offset) {
    const line = getLineRange(value, offset);
    return { ...line, newlineEnd: line.end < value.length && value[line.end] === "\n" ? line.end + 1 : line.end };
  }
  _previousLineText(value, lineStart) {
    if (lineStart <= 0) return "";
    return getLineRange(value, lineStart - 1).text;
  }
  _nextLineText(value, lineEnd) {
    if (lineEnd >= value.length) return "";
    return getLineRange(value, lineEnd + 1).text;
  }
  _lineHasStructuralNeighbors(value, line) {
    const texts = [this._previousLineText(value, line.start), line.text, this._nextLineText(value, line.end)];
    return texts.some(text => isFenceLine(text) || isLikelyTableRow(text) || isTableDelimiter(text));
  }
  _parseSingleLineBlock(line) {
    const heading = parseHeading(line.text);
    if (heading) return { type: "heading", from: line.start, to: line.end, newlineEnd: line.newlineEnd, line, heading };
    const list = parseListItem(line.text, this._parseOptions());
    if (list) return { type: list.kind, from: line.start, to: line.end, newlineEnd: line.newlineEnd, line, list };
    const quote = parseBlockquote(line.text);
    if (quote) return { type: "blockquote", from: line.start, to: line.end, newlineEnd: line.newlineEnd, line, quote };
    if (isHorizontalRule(line.text)) return { type: "horizontal-rule", from: line.start, to: line.end, newlineEnd: line.newlineEnd, line };
    return { type: line.text.trim() ? "paragraph" : "blank", from: line.start, to: line.end, newlineEnd: line.newlineEnd, line };
  }
  _tryIncrementalBlocks(previousValue, changes) {
    const sorted = normalizeChanges(changes || []);
    if (!previousValue || sorted.length !== 1 || this._blockCacheValue !== previousValue || !this._blockCache) return null;
    const change = sorted[0];
    const removed = previousValue.slice(change.from, change.to);
    if (removed.includes("\n") || change.insert.includes("\n")) return null;
    const oldLine = this._lineAt(previousValue, change.from);
    const newLine = this._lineAt(this._value, change.from + change.insert.length);
    if (oldLine.start !== newLine.start || this._lineHasStructuralNeighbors(previousValue, oldLine) || this._lineHasStructuralNeighbors(this._value, newLine)) return null;
    const oldBlocks = this._blockCache;
    const index = oldBlocks.findIndex(block => block.from === oldLine.start && block.to === oldLine.end && block.line);
    if (index === -1) return null;
    const oldBlock = oldBlocks[index];
    if (oldBlock.type === "table" || oldBlock.type === "code-fence") return null;
    const newBlock = this._parseSingleLineBlock(newLine);
    const delta = this._value.length - previousValue.length;
    return oldBlocks.map((block, i) => {
      if (i < index) return block;
      if (i === index) return newBlock;
      return this._shiftBlockOffsets(block, delta);
    });
  }
  _shiftCell(cell, delta) {
    return cell ? { ...cell, from: cell.from + delta, to: cell.to + delta } : cell;
  }
  _shiftLineOffsets(line, delta) {
    if (!line) return line;
    const shifted = { ...line, start: line.start + delta, end: line.end + delta, newlineEnd: line.newlineEnd + delta };
    if (line.cells) shifted.cells = line.cells.map(cell => this._shiftCell(cell, delta));
    return shifted;
  }
  _shiftBlockOffsets(block, delta) {
    const shifted = { ...block, from: block.from + delta, to: block.to + delta, newlineEnd: block.newlineEnd + delta };
    if (block.line) shifted.line = this._shiftLineOffsets(block.line, delta);
    if (block.opening) shifted.opening = this._shiftLineOffsets(block.opening, delta);
    if (block.closing) shifted.closing = this._shiftLineOffsets(block.closing, delta);
    if (block.codeLines) shifted.codeLines = block.codeLines.map(line => this._shiftLineOffsets(line, delta));
    if (block.header) shifted.header = this._shiftLineOffsets(block.header, delta);
    if (block.delimiter) shifted.delimiter = this._shiftLineOffsets(block.delimiter, delta);
    if (block.rows) shifted.rows = block.rows.map(row => this._shiftLineOffsets(row, delta));
    return shifted;
  }
  _bindDocumentScroll() {
    this._boundDocumentScroll ??= () => this._onLiveScroll();
    const doc = this.ownerDocument;
    const view = doc?.defaultView;
    if (!doc) return;
    doc.removeEventListener("scroll", this._boundDocumentScroll, true);
    view?.removeEventListener("scroll", this._boundDocumentScroll, true);
    doc.addEventListener("scroll", this._boundDocumentScroll, true);
    view?.addEventListener("scroll", this._boundDocumentScroll, true);
  }
  _selectionFocusOffset() {
    const sel = this._selection;
    if (!sel) return 0;
    return sel.direction === "backward" ? sel.start : sel.end;
  }
  _metrics() {
    const editor = this._liveEditor;
    const width = editor?.clientWidth || 0;
    if (this._metricsCache && this._metricsWidth === width) return this._metricsCache;
    if (this._metricsWidth != null && width && this._metricsWidth !== width) {
      this._blockHeights?.clear();
      this._heightTops = null;
    }
    const style = globalThis.getComputedStyle?.(editor || this);
    const fontSize = Number.parseFloat(style?.fontSize || "") || 16;
    const gap = style?.getPropertyValue?.("--md-editor-paragraph-gap")?.trim?.() || "";
    let blank = fontSize * 1.1;
    if (gap.endsWith("px")) blank = Number.parseFloat(gap) || blank;
    else if (gap.endsWith("em")) blank = (Number.parseFloat(gap) || 1.1) * fontSize;
    const pad = (Number.parseFloat(style?.paddingLeft || "") || 0) + (Number.parseFloat(style?.paddingRight || "") || 0);
    this._metricsWidth = width;
    this._metricsCache = {
      fontSize,
      blank,
      line: Math.max(18, this._computedLineHeight(editor || this)),
      chars: Math.max(8, Math.floor(((width || 640) - pad) / Math.max(1, fontSize * 0.5))),
      fenceLine: 20,
      fenceLead: 20,
      fenceChrome: 52,
      tableRow: fontSize * 1.35 + 14,
      tableChrome: fontSize,
    };
    return this._metricsCache;
  }
  _visibleExtent() {
    const editor = this._liveEditor;
    const line = this._metrics().line;
    const viewport = globalThis.innerHeight || 800;
    if (!editor) return { y: 0, height: viewport };
    const pad = Number.parseFloat(globalThis.getComputedStyle?.(editor)?.paddingTop || "") || 0;
    const box = editor.clientHeight || 0;
    if (box > 0 && editor.scrollHeight > box + 2) return { y: editor.scrollTop || 0, height: Math.max(line * 8, box) };
    // The writing page grows with the note and the document scrolls. The
    // editor's own box is the whole note, so the visible slice is the viewport.
    const rect = editor.getBoundingClientRect();
    return { y: Math.max(0, -rect.top - pad), height: Math.max(line * 8, viewport) };
  }
  _virtualMargin() {
    return this._metrics().line * 18;
  }
  _shouldVirtualize(blocks) {
    if (!blocks?.length) return false;
    const factor = this._virtualState.active ? 1.5 : 2;
    return (this._blockTops(blocks)[blocks.length] || 0) > this._visibleExtent().height * factor;
  }
  _virtualLineHeight() {
    return this._metrics().line;
  }
  _blockHeightKey(block) {
    const extra = block.type === "code-fence"
      ? (block.codeLines?.length || 0)
      : block.type === "table"
        ? (block.rows?.length || 0)
        : (block.line?.text?.length || 0);
    return `${block.type}:${block.from}:${block.to}:${extra}`;
  }
  _blockHeight(block) {
    const cached = this._blockHeights?.get(this._blockHeightKey(block));
    return cached || this._estimateBlockHeight(block);
  }
  _storeBlockHeight(block, height) {
    if (!block || !(height > 0)) return false;
    if (!this._blockHeights) this._blockHeights = new Map();
    if (this._blockHeights.size > 20000) this._blockHeights.clear();
    const key = this._blockHeightKey(block);
    const prev = this._blockHeights.get(key);
    if (prev != null && Math.abs(prev - height) < 1) return false;
    this._blockHeights.set(key, height);
    this._heightTops = null;
    return true;
  }
  _hasAfterAnchor(block) {
    if (!block) return false;
    if (block.type === "code-fence") return Boolean(block.closing) && block.newlineEnd === block.to;
    if (block.type === "table") return block.newlineEnd === block.to;
    if (block.type === "heading") return Boolean(block.setext) && block.newlineEnd === block.to;
    return false;
  }
  _estimateBlockHeight(block) {
    const metrics = this._metrics();
    if (!block) return metrics.line;
    const anchor = this._hasAfterAnchor(block) ? metrics.blank : 0;
    if (block.type === "blank") return metrics.blank;
    if (block.type === "heading") {
      const level = Number(block.heading?.level) || 1;
      const scale = [1, 2, 1.6, 1.35, 1.18, 1.1, 1][level] || 1;
      return metrics.fontSize * scale * 1.18 + metrics.fontSize * 0.44 + anchor;
    }
    if (block.type === "horizontal-rule") return metrics.fontSize * 1.35;
    if (block.type === "code-fence") return metrics.fenceChrome + this._fenceRows(block).length * metrics.fenceLine + anchor;
    if (block.type === "table") {
      const body = Math.max(block.rows?.length || 0, 1);
      return metrics.tableChrome + (1 + body) * metrics.tableRow + anchor;
    }
    return metrics.line * this._textLineCount(block.line?.text ?? "");
  }
  _textLineCount(text) {
    const chars = this._metrics().chars;
    let lines = 0;
    for (const part of String(text ?? "").split("\n")) lines += Math.max(1, Math.ceil((part.length || 1) / chars));
    return Math.max(1, lines);
  }
  _fenceRows(block) {
    const rows = [];
    if (block?.opening) rows.push(block.opening);
    for (const line of block?.codeLines || []) rows.push(line);
    if (block?.closing) rows.push(block.closing);
    return rows;
  }
  _blockTops(blocks) {
    if (this._heightBlocks === blocks && this._heightTops) return this._heightTops;
    const tops = new Float64Array(blocks.length + 1);
    for (let i = 0; i < blocks.length; i += 1) tops[i + 1] = tops[i] + this._blockHeight(blocks[i]);
    this._heightBlocks = blocks;
    this._heightTops = tops;
    return tops;
  }
  _indexAtContentY(tops, y) {
    const last = tops.length - 2;
    if (last < 0) return 0;
    let low = 0;
    let high = last;
    let best = 0;
    while (low <= high) {
      const mid = Math.floor((low + high) / 2);
      if (tops[mid] <= y) { best = mid; low = mid + 1; }
      else high = mid - 1;
    }
    return best;
  }
  _blockIndexForOffset(blocks, offset) {
    const last = blocks[blocks.length - 1];
    // Painted blocks can still end past the note after a deletion. Clamping to
    // the shorter note pulls a caret on the next line back into the edited one.
    const paintedEnd = Math.max(last?.newlineEnd ?? 0, last?.to ?? 0, last?.from ?? 0);
    const safe = clamp(offset, 0, Math.max(this._value.length, paintedEnd));
    let low = 0;
    let high = blocks.length - 1;
    let best = 0;
    while (low <= high) {
      const mid = Math.floor((low + high) / 2);
      const block = blocks[mid];
      if (safe < block.from) high = mid - 1;
      else {
        best = mid;
        if (safe <= Math.max(block.to, block.from)) return mid;
        low = mid + 1;
      }
    }
    return clamp(best, 0, Math.max(0, blocks.length - 1));
  }
  _virtualSpan(blocks, opts) {
    const tops = this._blockTops(blocks);
    const extent = this._visibleExtent();
    const margin = this._virtualMargin();
    let y0 = extent.y - margin;
    let y1 = extent.y + extent.height + margin;
    if (!opts.fromScroll && opts.anchorOffset != null) {
      const index = opts.anchorIndex ?? this._blockIndexForOffset(blocks, opts.anchorOffset);
      const mid = ((tops[index] || 0) + (tops[Math.min(index + 1, tops.length - 1)] || 0)) / 2;
      const inView = mid >= extent.y - margin && mid <= extent.y + extent.height + margin;
      if (!inView) {
        y0 = mid - extent.height / 2 - margin;
        y1 = mid + extent.height / 2 + margin;
      }
    }
    let start = this._indexAtContentY(tops, Math.max(0, y0));
    let end = this._indexAtContentY(tops, Math.max(0, y1)) + 1;
    for (const index of [opts.coverIndex, opts.coverIndex2]) {
      if (index == null) continue;
      start = Math.min(start, index);
      end = Math.max(end, index + 1);
    }
    end = clamp(end, 0, blocks.length);
    start = clamp(start, 0, end);
    if (start === end && blocks.length) end = Math.min(blocks.length, start + 1);
    return { start, end, tops, y0, y1 };
  }
  _sliceView(opts, y0, y1) {
    return { y0, y1, pinSelection: !opts.fromScroll, focusOffset: opts.fromScroll ? null : opts.anchorOffset };
  }
  _rowWindow(rows, top, lead, line, view) {
    const total = rows.length;
    if (!total) return { start: 0, end: 0, total: 0, lineHeight: line };
    const height = Math.max(1, line);
    const rowAt = y => clamp(Math.floor((y - top - lead) / height), 0, total);
    let start = rowAt(view.y0);
    let end = Math.min(total, rowAt(view.y1) + 1);
    const pins = [];
    if (view.pinSelection && this._selection) pins.push(this._selection.start, this._selection.end);
    if (view.focusOffset != null) pins.push(view.focusOffset);
    for (const offset of pins) {
      const row = rows.findIndex(item => offset >= item.start && offset <= item.end);
      if (row >= 0) {
        start = Math.min(start, row);
        end = Math.max(end, row + 1);
      }
    }
    const budget = Math.max(8, Math.ceil((view.y1 - view.y0) / height));
    if (end - start > budget) {
      const focus = view.focusOffset ?? this._selectionFocusOffset();
      let center = rows.findIndex(item => focus >= item.start && focus <= item.end);
      if (center < 0) center = start;
      start = clamp(center - Math.floor(budget / 2), 0, total);
      end = clamp(start + budget, 0, total);
    }
    if (end <= start) end = Math.min(total, start + 1);
    return { start, end, total, lineHeight: height };
  }
  _fenceLineWindow(block, top, view) {
    const rows = this._fenceRows(block);
    const metrics = this._metrics();
    const budget = this._visibleExtent().height + this._virtualMargin() * 2;
    if (rows.length < 2 || this._blockHeight(block) <= budget) return null;
    const window = this._rowWindow(rows, top, metrics.fenceLead, metrics.fenceLine, view);
    if (window.start <= 0 && window.end >= rows.length) return null;
    const rendered = rows.slice(window.start, window.end);
    return { ...window, sourceFrom: rendered[0]?.start ?? block.from, sourceTo: rendered.at(-1)?.end ?? block.to };
  }
  _tableRowWindow(block, top, view) {
    const rows = block.rows || [];
    const metrics = this._metrics();
    const budget = this._visibleExtent().height + this._virtualMargin() * 2;
    if (rows.length < 2 || this._blockHeight(block) <= budget) return null;
    const window = this._rowWindow(rows, top, metrics.tableRow, metrics.tableRow, view);
    if (window.start <= 0 && window.end >= rows.length) return null;
    const rendered = rows.slice(window.start, window.end);
    return {
      ...window,
      headerFrom: block.header?.start ?? block.from,
      headerTo: block.header?.end ?? block.from,
      sourceFrom: rendered[0]?.start ?? block.from,
      sourceTo: rendered.at(-1)?.end ?? block.to,
    };
  }
  _sliceSignature(blocks, start, end, tops, view) {
    const parts = [];
    for (let index = start; index < end; index += 1) {
      const block = blocks[index];
      const window = block?.type === "code-fence"
        ? this._fenceLineWindow(block, tops[index] || 0, view)
        : block?.type === "table"
          ? this._tableRowWindow(block, tops[index] || 0, view)
          : null;
      if (window) parts.push(`${index}:${window.start}:${window.end}`);
    }
    return parts.join(",");
  }
  _htmlForVirtualBlock(block, index, tops, view, slices) {
    let html;
    if (block.type === "code-fence") {
      const window = this._fenceLineWindow(block, tops[index] || 0, view);
      if (window) {
        slices.set(index, window);
        html = codeFenceBlockHtml(block, { ...this._livePaintOptions(), lineWindow: window });
      } else html = this._renderLiveBlock(block);
    } else if (block.type === "table") {
      const window = this._tableRowWindow(block, tops[index] || 0, view);
      if (window) {
        slices.set(index, window);
        html = this._renderTable(block, window);
      } else html = this._renderLiveBlock(block);
    } else html = this._renderLiveBlock(block);
    return this._stampVirtualBlock(html, index, block);
  }
  _stampVirtualBlock(html, index, block) {
    const template = document.createElement("template");
    template.innerHTML = html;
    const nodes = [...template.content.children];
    const blank = this._metrics().blank;
    const total = this._blockHeight(block);
    const tail = nodes.length > 1 ? blank : 0;
    nodes.forEach((node, i) => {
      node.dataset.virtualBlock = String(index);
      const size = tail && i === nodes.length - 1 ? tail : Math.max(1, total - tail);
      node.style.setProperty("--md-virtual-size", `${Math.round(size)}px`);
    });
    return template.innerHTML;
  }
  _renderLiveVirtual(blocks, opts = {}) {
    const paintOpts = { ...opts, anchorOffset: opts.anchorOffset ?? this._selectionFocusOffset() };
    const { start, end, tops, y0, y1 } = this._virtualSpan(blocks, paintOpts);
    const view = this._sliceView(paintOpts, y0, y1);
    const sliceKey = this._sliceSignature(blocks, start, end, tops, view);
    const total = blocks.length;
    if (!paintOpts.force && this._virtualState.active && start === this._virtualState.start && end === this._virtualState.end && total === this._virtualState.total && sliceKey === this._virtualState.sliceKey) return false;
    const slices = new Map();
    const html = [];
    for (let index = start; index < end; index += 1) html.push(this._htmlForVirtualBlock(blocks[index], index, tops, view, slices));
    this._beginVirtualPaint();
    try {
      this._clearOffsetMap();
      this._liveEditor.innerHTML = html.join("");
      this._paintGapSpacers();
      this._liveEditor.prepend(this._virtualSpacer(Math.max(0, Math.round(tops[start] || 0))));
      this._liveEditor.append(this._virtualSpacer(Math.max(0, Math.round((tops[total] || 0) - (tops[end] || 0)))));
      this._virtualState = { active: true, start, end, total, lineHeight: this._metrics().line, slices, sliceKey };
      this._markVirtualCaret(blocks);
      if (this._measureVirtualBlocks(blocks) && !paintOpts.reflow && !this._virtualWindowCovers(blocks)) {
        return this._renderLiveVirtual(blocks, { ...paintOpts, force: true, reflow: true });
      }
    } finally {
      this._endVirtualPaint();
    }
    return true;
  }
  _beginVirtualPaint() { this._virtualPainting = (this._virtualPainting || 0) + 1; }
  _endVirtualPaint() { this._virtualPainting = Math.max(0, (this._virtualPainting || 0) - 1); }
  _virtualWindowCovers(blocks) {
    if (!this._virtualState.active) return false;
    const { start, end } = this._virtualState;
    const tops = this._blockTops(blocks);
    const extent = this._visibleExtent();
    const slack = this._metrics().line * 8;
    const topOk = start === 0 ? extent.y <= slack : (tops[start] || 0) <= Math.max(0, extent.y - slack);
    const bottomOk = end === blocks.length ? true : (tops[end] || 0) >= extent.y + extent.height + slack;
    if (!topOk || !bottomOk) return false;
    const view = this._sliceView({ fromScroll: true }, extent.y - this._virtualMargin(), extent.y + extent.height + this._virtualMargin());
    return this._sliceSignature(blocks, start, end, tops, view) === this._virtualState.sliceKey;
  }
  _measureVirtualBlocks(blocks) {
    const editor = this._liveEditor;
    if (!editor) return false;
    const extent = this._visibleExtent();
    const tops = this._blockTops(blocks);
    const groups = new Map();
    for (const node of editor.children) {
      if (node.classList.contains("md-virtual-spacer")) continue;
      const index = Number(node.dataset.virtualBlock);
      if (!Number.isFinite(index)) continue;
      const group = groups.get(index) || [];
      group.push(node);
      groups.set(index, group);
    }
    let changed = false;
    for (const [index, nodes] of groups) {
      if (this._virtualState.slices?.has(index)) continue;
      const top = tops[index] || 0;
      const bottom = tops[index + 1] || top;
      const caret = nodes.some(node => node.classList.contains("md-caret-block"));
      if (!caret && (bottom <= extent.y || top >= extent.y + extent.height)) continue;
      let height = 0;
      for (const node of nodes) {
        const style = globalThis.getComputedStyle?.(node);
        const margin = (Number.parseFloat(style?.marginTop || "") || 0) + (Number.parseFloat(style?.marginBottom || "") || 0);
        height += node.offsetHeight + margin;
      }
      if (this._storeBlockHeight(blocks[index], height)) changed = true;
    }
    if (this._calibrateCodeLine()) changed = true;
    return changed;
  }
  _calibrateCodeLine() {
    const line = this._liveEditor?.querySelector(".md-code-line, .md-code-fence-line");
    if (!line) return false;
    const height = line.getBoundingClientRect().height;
    const metrics = this._metrics();
    if (!(height > 8) || Math.abs(height - metrics.fenceLine) < 0.5) return false;
    metrics.fenceLine = height;
    this._heightTops = null;
    return true;
  }
  _markVirtualCaret(blocks = this._liveBlocks) {
    const editor = this._liveEditor;
    if (!editor) return;
    if (!this._virtualState.active || !blocks?.length) {
      editor.classList.remove("md-virtual");
      return;
    }
    editor.classList.add("md-virtual");
    const sel = this._selection || { start: 0, end: 0, direction: "none" };
    const focus = sel.direction === "backward" ? sel.start : sel.end;
    const anchor = sel.direction === "backward" ? sel.end : sel.start;
    const indexes = new Set([
      this._blockIndexForOffset(blocks, focus),
      this._blockIndexForOffset(blocks, anchor),
    ]);
    for (const node of editor.children) {
      if (node.classList?.contains("md-virtual-spacer")) continue;
      node.classList.toggle("md-caret-block", indexes.has(Number(node.dataset.virtualBlock)));
    }
  }
  _syncVirtualWindow(blocks, changes) {
    const { start, end } = this._virtualState;
    if (!this._liveEditor || end <= start) return;
    const children = Array.from(this._liveEditor.children).filter(node => !node.classList?.contains("md-virtual-spacer"));
    const slice = blocks.slice(start, end);
    if (children.length !== slice.length) {
      this._renderLiveVirtual(blocks, { force: true });
      return;
    }
    const touch = Number(changes?.[0]?.from);
    const touchFrom = Number.isFinite(touch) ? touch : 0;
    const spans = changedSpans(changes);
    for (let i = 0; i < slice.length; i += 1) {
      const block = slice[i];
      const blockEnd = Math.max(block.newlineEnd ?? block.to ?? block.from ?? 0, block.to ?? block.from ?? 0, block.from ?? 0);
      if (changes?.length && blockEnd < touchFrom) continue;
      const edited = spans && block.from < spans.newEnd && blockEnd > spans.newStart;
      if (edited && (block.type === "code-fence" || block.type === "table")) {
        this._renderLiveVirtual(blocks, { anchorOffset: this._selectionFocusOffset(), force: true });
        return;
      }
      if (!this._syncLiveBlockNodeMetadata(children[i], block)) {
        this._renderLiveVirtual(blocks, { force: true });
        return;
      }
    }
  }
  _virtualSpacer(height) {
    const spacer = document.createElement("div");
    spacer.className = "md-virtual-spacer";
    spacer.contentEditable = "false";
    spacer.setAttribute("aria-hidden", "true");
    spacer.style.blockSize = `${height}px`;
    return spacer;
  }
  _paintGapSpacers() {
    for (const spacer of this._liveEditor.querySelectorAll(".md-virtual-spacer[data-block-size]")) {
      spacer.style.blockSize = `${spacer.dataset.blockSize}px`;
    }
  }
  _sliceCovers(slice, offset) {
    if (!slice) return true;
    if (offset >= slice.sourceFrom && offset <= slice.sourceTo) return true;
    return slice.headerFrom != null && offset >= slice.headerFrom && offset <= slice.headerTo;
  }
  _isSourceOffsetRendered(offset) {
    if (!this._virtualState.active) return true;
    const blocks = this._liveBlocks?.length ? this._liveBlocks : this._getBlocks();
    const index = this._blockIndexForOffset(blocks, offset);
    if (index < this._virtualState.start || index >= this._virtualState.end) return false;
    return this._sliceCovers(this._virtualState.slices?.get(index), offset);
  }
  _ensureVirtualSelectionVisible(selection) {
    if (!this._virtualState.active) return true;
    const blocks = this._liveBlocks?.length ? this._liveBlocks : this._getBlocks();
    const startOff = Math.min(selection.start, selection.end);
    const endOff = Math.max(selection.start, selection.end);
    if (this._isSourceOffsetRendered(startOff) && this._isSourceOffsetRendered(endOff)) return true;
    const tops = this._blockTops(blocks);
    const startIndex = this._blockIndexForOffset(blocks, startOff);
    const endIndex = this._blockIndexForOffset(blocks, endOff);
    const span = (tops[endIndex + 1] || 0) - (tops[startIndex] || 0);
    if (span > this._visibleExtent().height + this._virtualMargin() * 2) return false;
    const focus = selection.direction === "backward" ? selection.start : selection.end;
    this._renderLiveVirtual(blocks, { anchorOffset: focus, coverIndex: startIndex, coverIndex2: endIndex, force: true });
    this._rebuildLiveIndex();
    return this._isSourceOffsetRendered(startOff) && this._isSourceOffsetRendered(endOff);
  }
  _onLiveScroll() {
    if (this._selectionToolbar && !this._selectionToolbar.hidden) this._positionSelectionToolbar();
    if (!this._virtualState.active || this._virtualScrollFrame) return;
    this._virtualScrollFrame = requestAnimationFrame(() => {
      this._virtualScrollFrame = 0;
      if (!this._virtualState.active || !this._liveEditor) return;
      const blocks = this._liveBlocks?.length ? this._liveBlocks : this._getBlocks();
      if (this._virtualWindowCovers(blocks)) return;
      this._beginVirtualPaint();
      try {
        this._renderLiveVirtual(blocks, { fromScroll: true, force: true });
        this._rebuildLiveIndex();
        const sel = this._selection;
        if (sel && this._isSourceOffsetRendered(sel.start) && this._isSourceOffsetRendered(sel.end)) this._restoreLiveSelection(sel);
      } finally {
        this._endVirtualPaint();
      }
    });
  }
  _contentYFromClient(clientY) {
    const editor = this._liveEditor;
    if (!editor) return 0;
    const rect = editor.getBoundingClientRect();
    const pad = Number.parseFloat(globalThis.getComputedStyle?.(editor)?.paddingTop || "") || 0;
    return clientY - rect.top + (editor.scrollTop || 0) - pad;
  }
  _offsetForContentY(blocks, index, tops, y) {
    const block = blocks[index];
    if (!block) return 0;
    const metrics = this._metrics();
    if (block.type === "code-fence") {
      const rows = this._fenceRows(block);
      const row = clamp(Math.floor((y - (tops[index] || 0) - metrics.fenceLead) / metrics.fenceLine), 0, Math.max(0, rows.length - 1));
      return rows[row]?.start ?? block.from ?? 0;
    }
    if (block.type === "table") {
      const rows = block.rows || [];
      const row = clamp(Math.floor((y - (tops[index] || 0) - metrics.tableRow) / metrics.tableRow), 0, Math.max(0, rows.length - 1));
      return rows[row]?.cells?.[0]?.from ?? rows[row]?.start ?? block.from ?? 0;
    }
    return block.line?.start ?? block.from ?? 0;
  }
  _sourceTextHtml(from, to) {
    const text = this._value.slice(from, to);
    return text ? escapeHtml(text) : "<br>";
  }
  _keepsSourceNewlines(editable) {
    return editable?.dataset?.sourceForm === "true" && editable.dataset.editable !== "cell";
  }
  _visibleText(editable) {
    const text = this._plainText(editable);
    return this._keepsSourceNewlines(editable) ? text : text.replace(/\n/g, "");
  }
  _selectionFullyInside(from, to) {
    if (!this._focusWithin || this.mode === "source") return false;
    const sel = this._selection;
    if (!sel) return false;
    const start = Math.min(sel.start, sel.end);
    const end = Math.max(sel.start, sel.end);
    const blockFrom = Number(from) || 0;
    const blockTo = Math.max(Number(to) || 0, blockFrom);
    return start >= blockFrom && end <= blockTo;
  }
  _blockIsSourceForm(block) {
    if (!block || block.type === "table" || block.type === "code-fence") return false;
    if (isCommentLine(block.line?.text) || loneImageMarkdown(block.line?.text)) return false;
    return this._selectionFullyInside(block.from, block.to);
  }
  _tableCellHolding(block, start, end) {
    const consider = (cell, row, col) => {
      if (!cell || !Number.isFinite(cell.from) || !Number.isFinite(cell.to)) return null;
      const cellFrom = cell.from;
      const cellTo = Math.max(cell.to, cell.from);
      if (start >= cellFrom && end <= cellTo) return { cell, row, col };
      return null;
    };
    const header = block.header?.cells || [];
    for (let col = 0; col < header.length; col += 1) {
      const hit = consider(header[col], -1, col);
      if (hit) return hit;
    }
    const rows = block.rows || [];
    for (let row = 0; row < rows.length; row += 1) {
      const cells = rows[row].cells || [];
      for (let col = 0; col < cells.length; col += 1) {
        const hit = consider(cells[col], row, col);
        if (hit) return hit;
      }
    }
    return null;
  }
  _sourceFormKey() {
    const painted = this._offsetMap.length > 0 && this._liveBlocks?.length;
    const blocks = this._liveBlocks?.length ? this._liveBlocks : this._blockCache;
    return this._sourceFormKeyFor(blocks, painted);
  }
  _sourceFormKeyFor(blocks, unmap) {
    if (!this._focusWithin || this.mode === "source" || !this._selection || !blocks?.length) return "";
    let start = Math.min(this._selection.start, this._selection.end);
    let end = Math.max(this._selection.start, this._selection.end);
    if (unmap) {
      start = unmapPaintedOffset(start, this._offsetMap);
      end = unmapPaintedOffset(end, this._offsetMap);
      if (end < start) [start, end] = [end, start];
    }
    const index = this._blockIndexForOffset(blocks, start);
    const block = blocks[index];
    if (!block || start < block.from || end > Math.max(block.to, block.from)) return "";
    if (block.type === "code-fence" || isCommentLine(block.line?.text) || loneImageMarkdown(block.line?.text)) return "";
    if (block.type === "table") {
      const hit = this._tableCellHolding(block, start, end);
      if (!hit) return "";
      return `table:${block.from}:${hit.row}:${hit.col}:${hit.cell.from}`;
    }
    return `${index}:${block.type}:${block.from}`;
  }
  _sourceFormBlockIndexes(key, blocks) {
    if (!key) return [];
    if (key.startsWith("table:")) {
      const from = Number(key.slice("table:".length).split(":")[0]);
      const index = blocks.findIndex(block => block.type === "table" && block.from === from);
      return index < 0 ? null : [index];
    }
    const parts = key.split(":");
    const index = Number(parts[0]);
    const type = parts[1];
    const from = Number(parts[2]);
    const block = blocks[index];
    if (!block || block.type !== type || block.from !== from) return null;
    return [index];
  }
  _repaintSourceFormBlocks(previousKey, nextKey) {
    if (this._virtualState.active || !this._liveEditor) return false;
    const blocks = this._liveBlocks;
    if (!blocks?.length) return false;
    const previous = this._sourceFormBlockIndexes(previousKey, blocks);
    const next = this._sourceFormBlockIndexes(nextKey, blocks);
    if (!previous || !next) return false;
    const children = Array.from(this._liveEditor.children);
    if (children.length !== blocks.length) return false;
    const indexes = [...new Set([...previous, ...next])].sort((a, b) => b - a);
    const prepared = [];
    for (const index of indexes) {
      const fragment = this._liveFragmentForBlocks([blocks[index]]);
      const fresh = Array.from(fragment.children);
      if (fresh.length !== 1 || !children[index]) return false;
      prepared.push({ oldNode: children[index], fresh: fresh[0] });
    }
    for (const item of prepared) item.oldNode.replaceWith(item.fresh);
    this._rebuildLiveIndex();
    return true;
  }
  _syncSourceForm() {
    if (!this._liveEditor || this.mode === "source" || this._sourceFormLock || this._pointerSelection || this._isComposing) return;
    const key = this._sourceFormKey();
    if (this._idleFormattedKey && key === this._idleFormattedKey && (this._sourceFormKeyShown ?? "") === "") return;
    this._idleFormattedKey = "";
    if (this._deferredPaint || this._paintFrame) {
      if (this._deferredPaintBlocked()) return;
      if (this._sourceFormKey() === (this._sourceFormKeyShown ?? "")) return;
      this._sourceFormLock = true;
      try {
        this._flushDeferredPaint();
      } finally {
        this._sourceFormLock = false;
      }
      // The flush already painted this window, including the line the caret entered.
      if (this._virtualState.active) return;
    }
    if (key === (this._sourceFormKeyShown ?? "")) return;
    if (this._sourceFormatTimer) clearTimeout(this._sourceFormatTimer);
    this._sourceFormatTimer = 0;
    const previousKey = this._sourceFormKeyShown ?? "";
    this._sourceFormLock = true;
    this._sourceFormKeyShown = key;
    try {
      if (this._repaintSourceFormBlocks(previousKey, key)) {
        if (!this._isSourceActive()) this._restoreLiveSelection(this._selection);
      } else {
        this._renderAll({ restoreSelection: true, force: true });
      }
    } finally {
      this._sourceFormLock = false;
    }
  }
  _keepsActiveSource(editable) {
    if (!editable) return false;
    if (editable.dataset.editable === "cell") return true;
    return ["paragraph", "heading", "blockquote", "bullet-list-item", "ordered-list-item", "task-list-item", "blank"].includes(editable.dataset.kind);
  }
  _scheduleActiveSourceFormat() {
    if (this._sourceFormatTimer) clearTimeout(this._sourceFormatTimer);
    this._sourceFormatTimer = setTimeout(() => {
      this._sourceFormatTimer = 0;
      this._formatShownSourceLine();
    }, 600);
  }
  _formatShownSourceLine() {
    if (!this._liveEditor || this.mode === "source" || this._sourceFormLock) return;
    if (this._isComposing || this._completion?.open || this._deferredPaintBlocked()) return;
    if (this._deferredPaint || this._paintFrame) {
      if (this._deferredPaintBlocked()) return;
      this._flushDeferredPaint();
    }
    const key = this._sourceFormKeyShown ?? "";
    if (!key) return;
    const blocks = this._liveBlocks;
    const indexes = this._sourceFormBlockIndexes(key, blocks);
    if (!indexes?.length) return;
    const selection = { ...this._selection };
    this._sourceFormLock = true;
    const savedSelection = this._selection;
    this._selection = { start: -1, end: -1, direction: "none" };
    let prepared = null;
    try {
      prepared = this._planFormattedBlockReplacement(blocks, indexes);
    } finally {
      this._selection = savedSelection;
    }
    if (!prepared) {
      this._selection = { start: -1, end: -1, direction: "none" };
      try {
        this._renderAll({ restoreSelection: false, force: true });
      } finally {
        this._selection = savedSelection;
      }
    }
    this._sourceFormKeyShown = "";
    this._idleFormattedKey = key;
    try {
      if (prepared) {
        for (const item of prepared) {
          item.oldNodes[0].replaceWith(...item.fresh);
          for (const extra of item.oldNodes.slice(1)) extra.remove();
        }
        this._rebuildLiveIndex();
      }
      this._restoreLiveSelection(selection);
      this._armSpellcheck();
    } finally {
      this._sourceFormLock = false;
    }
  }
  // Top-level nodes for one block, including a trailing anchor when the block
  // is the last thing in the note. Built while the caret is outside the block
  // so the replacement is formatted text.
  _planFormattedBlockReplacement(blocks, indexes) {
    const children = Array.from(this._liveEditor.children).filter(node => !node.classList?.contains("md-virtual-spacer"));
    const planned = [];
    for (const index of indexes) {
      const block = blocks[index];
      if (!block) return null;
      const nextFrom = blocks[index + 1]?.from ?? Number.POSITIVE_INFINITY;
      let start = -1;
      let end = -1;
      for (let i = 0; i < children.length; i += 1) {
        const from = Number(children[i].dataset?.from);
        if (!Number.isFinite(from)) return null;
        if (from >= block.from && from < nextFrom) {
          if (start < 0) start = i;
          end = i + 1;
        }
      }
      if (start < 0) return null;
      const fresh = Array.from(this._liveFragmentForBlocks([blocks[index]]).children);
      if (!fresh.length) return null;
      planned.push({ start, oldNodes: children.slice(start, end), fresh });
    }
    planned.sort((a, b) => b.start - a.start);
    return planned;
  }
  _formatWhenIdle() {
    if (this._sourceFormatTimer) clearTimeout(this._sourceFormatTimer);
    this._sourceFormatTimer = 0;
    this._idleFormattedKey = "";
    if (!this._liveEditor || this.mode === "source" || this._sourceFormLock) return;
    if (this._deferredPaint || this._paintFrame) {
      if (this._deferredPaintBlocked()) return;
      this._flushDeferredPaint();
    }
    if ((this._sourceFormKeyShown ?? "") === "") return;
    const previousKey = this._sourceFormKeyShown ?? "";
    this._sourceFormLock = true;
    this._sourceFormKeyShown = "";
    try {
      if (!this._repaintSourceFormBlocks(previousKey, "")) this._renderAll({ restoreSelection: false, force: true });
    } finally {
      this._sourceFormLock = false;
    }
  }
  _renderSourceFormBlock(block) {
    const lineAttrs = (line, kind, extra = "", attrs = "", from = block.from, to = block.to) => `class="md-line ${extra}" part="line" data-editable="line" data-kind="${kind}" data-from="${from}" data-to="${to}" data-source-form="true" contenteditable="${this._lineEditable()}" spellcheck="false"${attrs ? ` ${attrs}` : ""}`;
    const html = this._sourceTextHtml(block.from, block.to);
    if (block.type === "code-fence") return codeFenceBlockHtml(block, this._livePaintOptions());
    const line = block.line || { start: block.from, end: block.to };
    if (block.type === "blank") {
      const placeholderAttrs = this._value.length === 0 ? `data-placeholder="${escapeAttribute(this.placeholder)}"` : "";
      const placeholderClass = this._value.length === 0 ? "md-empty-placeholder" : "";
      return `<div ${lineAttrs(line, "blank", `${placeholderClass} md-source-form`.trim(), placeholderAttrs)}>${html}</div>`;
    }
    if (block.type === "heading") {
      const afterAnchor = block.setext && block.newlineEnd === block.to
        ? `<div class="md-line md-setext-after" part="line" data-editable="virtual-setext-after" data-kind="blank" data-from="${block.to}" data-to="${block.to}" contenteditable="${this._lineEditable()}" spellcheck="false" aria-label="After heading"><br></div>`
        : "";
      return `<div ${lineAttrs(line, "heading", `md-heading md-h${block.heading.level} md-source-form`, `id="${escapeAttribute(block.heading.id)}"`)}>${html}</div>${afterAnchor}`;
    }
    if (block.type === "blockquote") {
      const depth = Math.max(1, Number(block.quote.depth) || 1);
      const visualDepth = Math.min(depth, 16);
      return `<div ${lineAttrs(line, "blockquote", `md-quote md-quote-depth-${visualDepth} md-source-form`, `data-quote-depth="${depth}"`)}>${html}</div>`;
    }
    if (block.type === "horizontal-rule") {
      return `<div ${lineAttrs(line, "horizontal-rule", "md-hr-line md-source-form")} aria-label="Divider">${html}</div>`;
    }
    if (block.type === "task-list-item") return taskLineHtml(block, this._livePaintOptions(), true);
    if (block.type === "bullet-list-item" || block.type === "ordered-list-item") {
      const depth = Math.floor((block.list?.indent.length || 0) / Math.max(1, this.indentString.length));
      return `<div ${lineAttrs(line, block.type, `md-list ${listDepthClass(depth)} md-source-form`)}>${html}</div>`;
    }
    return `<div ${lineAttrs(line, "paragraph", "md-source-form")}>${html}</div>`;
  }
  _livePaintOptions() {
    return {
      ...this._rendererOptions(),
      editable: this._lineEditable(),
      spellcheck: "false",
      indentString: this.indentString,
      placeholder: this.placeholder,
      emptyDocument: this._value.length === 0,
      disabled: this.disabled,
      readonly: this.readonly,
    };
  }
  _renderLiveBlock(block) {
    if (block.type !== "table" && this._blockIsSourceForm(block)) return this._renderSourceFormBlock(block);
    if (block.type === "table") return this._renderTable(block);
    return liveBlockHtml(block, this._livePaintOptions());
  }
  _lineEditable() { return (!this.disabled && !this.readonly) ? "true" : "false"; }
  _renderTable(block, rowWindow = null) {
    const cols = Math.max(block.header.cells.length, ...block.rows.map(r => r.cells.length), 1);
    const alignments = Array.from({ length: cols }, (_, i) => tableAlignmentFromDelimiter(block.delimiter.cells[i]?.text));
    // Hover controls (add/remove rows and columns), rendered inline so they are
    // recreated on every render. They carry no data-editable/data-from and are
    // contenteditable="false", so the DOM-to-source mapping ignores them.
    const editable = this._lineEditable() === "true";
    const icon = inner => `<svg viewBox="0 0 16 16" aria-hidden="true">${inner}</svg>`;
    const plus = icon('<line x1="8" y1="3" x2="8" y2="13"/><line x1="3" y1="8" x2="13" y2="8"/>');
    const minus = icon('<line x1="3" y1="8" x2="13" y2="8"/>');
    const control = (kind, label, glyph, extra = "") =>
      `<button type="button" class="md-tbl-ctl" data-table-control="${kind}"${extra} contenteditable="false" tabindex="-1" aria-label="${label}" title="${label}">${glyph}</button>`;
    // Column-delete tabs sit above each header cell; the single column-add tab
    // and each row-delete tab overlap the table's right edge (last cell); row-add
    // sits below. Delete tabs are hidden while only two columns / one row remain,
    // keeping those as the minimums.
    const addColBtn = editable ? control("add-col", "Add column", plus) : "";
    const addRowBtn = editable ? control("add-row", "Add row", plus) : "";
    const delColBtn = i => (editable && cols > 2) ? control("del-col", "Delete column", minus, ` data-index="${i}"`) : "";
    const delRowBtn = r => (editable && block.rows.length > 1) ? control("del-row", "Delete row", minus, ` data-index="${r}"`) : "";
    const lastCol = cols - 1;
    const renderCell = (cell, tag, row, col, extra = "") => {
      const from = cell?.from ?? block.to;
      const to = cell?.to ?? block.to;
      const sourceForm = this._selectionFullyInside(from, to);
      const unescaped = unescapeTableCellText(cell?.text ?? "");
      const body = sourceForm ? (unescaped ? escapeHtml(unescaped) : "<br>") : this._inlineHtml(unescaped);
      const form = sourceForm ? ` data-source-form="true"` : "";
      return `<${tag}${tableAlignmentClass(alignments[col])}>${extra}<div class="md-cell${sourceForm ? " md-source-form" : ""}" part="table-cell" data-editable="cell" data-row="${row}" data-col="${col}" data-from="${from}" data-to="${to}"${form} contenteditable="${this._lineEditable()}" spellcheck="false">${body}</div></${tag}>`;
    };
    const header = `<thead><tr>${Array.from({ length: cols }, (_, i) => renderCell(block.header.cells[i] ?? { text: "", from: block.header.end, to: block.header.end }, "th", -1, i, `${delColBtn(i)}${i === lastCol ? addColBtn : ""}`)).join("")}</tr></thead>`;
    const hasRealRows = block.rows.length > 0;
    const sourceRows = hasRealRows ? block.rows : [{ cells: Array.from({ length: cols }, () => ({ text: "", from: block.delimiter.end, to: block.delimiter.end })) }];
    let indexed = sourceRows.map((row, r) => ({ row, r }));
    let topGap = "";
    let bottomGap = "";
    if (rowWindow && hasRealRows) {
      const start = clamp(rowWindow.start, 0, sourceRows.length);
      const end = clamp(rowWindow.end, start, sourceRows.length);
      const line = Math.max(1, rowWindow.lineHeight || this._metrics().tableRow);
      if (start > 0) topGap = `<tr class="md-virtual-spacer"><td colspan="${cols}">${virtualGapHtml(start * line)}</td></tr>`;
      if (end < sourceRows.length) bottomGap = `<tr class="md-virtual-spacer"><td colspan="${cols}">${virtualGapHtml((sourceRows.length - end) * line)}</td></tr>`;
      indexed = sourceRows.slice(start, end).map((row, i) => ({ row, r: start + i }));
    }
    const body = `<tbody>${topGap}${indexed.map(({ row, r }) => `<tr>${Array.from({ length: cols }, (_, i) => renderCell(row.cells[i] ?? { text: "", from: row.end, to: row.end }, "td", r, i, i === lastCol && hasRealRows ? delRowBtn(r) : "")).join("")}</tr>`).join("")}${bottomGap}</tbody>`;
    const afterAnchor = block.newlineEnd === block.to
      ? `<div class="md-line md-table-after" part="line" data-editable="virtual-table-after" data-kind="blank" data-from="${block.to}" data-to="${block.to}" contenteditable="${this._lineEditable()}" spellcheck="false" aria-label="After table"><br></div>`
      : "";
    return `<div class="md-table-block${editable ? " md-has-controls" : ""}" part="table" data-kind="table" data-from="${block.from}" data-to="${block.to}"><table class="md-table">${header}${body}</table>${addRowBtn}</div>${afterAnchor}`;
  }

  _onSourceInput(event) {
    if (this._isComposing || event?.isComposing) return;
    const before = this._beforeInputSnapshot;
    const previousValue = this._value;
    this._value = normalizeLineEndings(this._sourceTextarea.value);
    if (this._sourceTextarea.value !== this._value) this._sourceTextarea.value = this._value;
    this._selection = { start: this._sourceTextarea.selectionStart, end: this._sourceTextarea.selectionEnd, direction: this._sourceTextarea.selectionDirection || "none" };
    const after = this._snapshot();
    if (before) this._recordUndo(before, after, this._undoGroupForInput(event?.inputType), { coalesce: event?.inputType === "insertText" || event?.inputType === "deleteContentBackward" });
    this._beforeInputSnapshot = null;
    this._afterValueChanged({ source: "user", inputType: event?.inputType, restoreSelection: false, previousValue, changes: diffTextChange(previousValue, this._value) });
    if (!this._isComposing) this._scheduleCompletionUpdate();
  }
  _onLiveBeforeInput(event) {
    if (this._isComposing || event?.isComposing || String(event?.inputType || "").includes("Composition")) return;
    if (this._editableTextDiffers(this._closestEditable(event?.target))) {
      if (!this._beforeInputSnapshot) {
        const sel = this._modelSelection();
        this._beforeInputSnapshot = makeSnapshot(this._value, sel.start, sel.end, sel.direction || "none");
      }
      return;
    }
    this._ignoreSelectionChangeCount = 0;
    this._structuredSelection = null;
    const inputType = event?.inputType || "";
    const inputTarget = this._inputTargetFromBeforeInput(event);
    this._beforeInputTarget = inputTarget;
    if (this._liveSelectionAPI === false && inputTarget) {
      const targetHasSelection = inputTarget.selection.start !== inputTarget.selection.end;
      const modelHasSelection = this._selection.start !== this._selection.end;
      if (!this._fallbackSelectionPending || (targetHasSelection && !modelHasSelection)) {
        this._selection = { ...inputTarget.selection };
      }
      this._fallbackEditable = inputTarget.editable;
    }
    if (this._value.length === 0 && inputType.startsWith("delete")) {
      event.preventDefault();
      this._beforeInputTarget = null;
      this._ensureEmptyLiveEditable();
      return;
    }
    if (!this._isSourceActive() && !this.disabled && !this.readonly) {
      const ctx = this._getContext();
      const hasSelection = ctx.selectionStart !== ctx.selectionEnd;
      const editable = this._closestEditable(event.target) || this._activeEditableFromSelection();
      const editableRange = this._editableSourceRange(editable);
      const selectionInsideTableCell = editable?.dataset.editable === "cell"
        && editableRange
        && ctx.selectionStart >= editableRange.from
        && ctx.selectionEnd <= editableRange.to;
      if (hasSelection && inputType.startsWith("delete") && !selectionInsideTableCell) {
        event.preventDefault();
        this._beforeInputTarget = null;
        this._applyActionResult("editor.deleteSelection", this._deleteSelectionResult(ctx, "editor.deleteSelection"), { source: "user" });
        return;
      }
      if (hasSelection && inputType === "insertText" && event.data != null && !selectionInsideTableCell) {
        event.preventDefault();
        this._beforeInputTarget = null;
        const text = normalizeLineEndings(event.data);
        this._applyActionResult("editor.replaceSelection", insertionTransaction(ctx, "editor.replaceSelection", text, text.length, "typing"), { source: "user" });
        return;
      }
    }
    if (this._liveSelectionAPI === false && this._applyFallbackBeforeInput(event, inputTarget)) {
      this._beforeInputTarget = null;
      return;
    }
    this._beforeInputSnapshot = this._snapshot();
  }
  _onLiveInput(event) {
    if (this.disabled || this.readonly) return;
    if (this._isComposing || event?.isComposing) return;
    if (event?.target && event.target.isConnected === false) return;
    this._ignoreSelectionChangeCount = 0;
    this._structuredSelection = null;
    const inputTarget = this._beforeInputTarget;
    const editable = this._closestEditable(event.target) || inputTarget?.editable || this._activeEditableFromSelection();
    this._beforeInputTarget = null;
    if (!editable) return;
    const before = this._beforeInputSnapshot;
    const from = this._liveFrom(editable); const to = this._liveTo(editable);
    const sourceSlice = this._value.slice(from, to);
    const sourceForm = editable.dataset.sourceForm === "true";
    const literalLine = sourceForm || ["image", "comment", "code-fence"].includes(editable.dataset.kind);
    let raw = this._plainText(editable);
    if (!this._keepsSourceNewlines(editable)) raw = raw.replace(/\n/g, "");
    let mappedCursor = null;
    if (!literalLine) {
      const inlinePaint = this._usesInlinePaint(editable);
      const inlineSource = editable.dataset.editable === "cell" ? unescapeTableCellText(sourceSlice) : sourceSlice;
      const compareSource = inlinePaint ? inlineSource : sourceSlice;
      const rendered = pageModule
        ? (inlinePaint ? renderInline(compareSource, this._rendererOptions()) : renderMarkdown(sourceSlice, this._rendererOptions()))
        : "";
      const oldDisplay = rendered ? this._textFromHtml(rendered).replace(/\n/g, "") : null;
      if (oldDisplay != null && oldDisplay !== compareSource && (inlinePaint || editable.dataset.editable !== "cell")) {
        const edit = sourceEditFromDisplay(compareSource, oldDisplay, raw);
        raw = edit.value;
        mappedCursor = edit.cursor;
      }
    }
    const liveSelection = this._getLiveSelection(editable);
    const changedDisplay = inputTarget
      ? diffTextChange(inputTarget.text, raw)[0]
      : null;
    const inferredDisplayCursor = changedDisplay
      ? changedDisplay.from + changedDisplay.insert.length
      : null;
    const compositionCommit = event?.inputType === "insertCompositionText";
    const tableDisplayCursor = editable.dataset.editable === "cell"
      ? (compositionCommit
        ? (this._displayOffsetFromSelection(editable) ?? raw.length)
        : (mappedCursor != null ? mappedCursor : (this._displayOffsetFromSelection(editable) ?? inferredDisplayCursor)))
      : null;
    const tableEdit = editable.dataset.editable === "cell" ? this._tableCellInputEdit(editable, raw, tableDisplayCursor) : null;
    if (tableEdit) {
      const previousValue = this._value;
      this._value = tableEdit.nextValue;
      this._selection = { start: tableEdit.cursor, end: tableEdit.cursor, direction: "none" };
      const after = makeSnapshot(this._value, this._selection.start, this._selection.end, this._selection.direction);
      if (before) this._recordUndo(before, after, this._undoGroupForInput(event?.inputType), { coalesce: event?.inputType === "insertText" || event?.inputType === "deleteContentBackward" });
      this._beforeInputSnapshot = null;
      this._finishLiveInput({ source: "user", inputType: event?.inputType, restoreSelection: true, previousValue, changes: diffTextChange(previousValue, this._value), editable });
      if (!this._isComposing) this._scheduleCompletionUpdate();
      return;
    }
    let insert = raw;
    let virtualPrefixLength = 0;
    if (editable.dataset.editable === "virtual-code" || editable.dataset.editable === "virtual-code-after" || editable.dataset.editable === "virtual-hr-after" || editable.dataset.editable === "virtual-setext-after" || editable.dataset.editable === "virtual-table-after") {
      const beforeSource = this._value.slice(0, from);
      if (!beforeSource.endsWith("\n")) {
        insert = `\n${raw}`;
        virtualPrefixLength = 1;
      }
    }
    const previousValue = this._value;
    const nextValue = previousValue.slice(0, from) + insert + previousValue.slice(to);
    const displayCursor = this._displayOffsetFromSelection(editable);
    const sourceCursor = liveSelection?.end != null ? liveSelection.end - from : null;
    const liveCursor = compositionCommit
      ? (displayCursor != null ? displayCursor : insert.length - virtualPrefixLength)
      : cursorAfterLineInput(
        displayCursor,
        mappedCursor,
        sourceCursor,
        inferredDisplayCursor ?? insert.length - virtualPrefixLength,
      );
    const cursor = clamp(from + virtualPrefixLength + liveCursor, from, from + insert.length);
    this._value = nextValue;
    this._selection = { start: cursor, end: cursor, direction: "none" };
    const after = makeSnapshot(this._value, this._selection.start, this._selection.end, this._selection.direction);
    if (before) this._recordUndo(before, after, this._undoGroupForInput(event?.inputType), { coalesce: event?.inputType === "insertText" || event?.inputType === "deleteContentBackward" });
    this._beforeInputSnapshot = null;
    this._finishLiveInput({ source: "user", inputType: event?.inputType, restoreSelection: true, previousValue, changes: [{ from, to, insert }], editable });
    if (!this._isComposing) this._scheduleCompletionUpdate();
  }
  _finishLiveInput(details) {
    const hold = details.inputType !== "insertCompositionText" && this._accentMenuHoldingTarget();
    this._holdAccentTarget = hold;
    try {
      this._afterValueChanged({ ...details, defer: !hold });
    } finally {
      this._holdAccentTarget = false;
    }
    if (!hold && !this._isComposing) this._scheduleActiveSourceFormat();
  }
  _accentMenuHoldingTarget() {
    if (now() - (this._keyRepeatAt || 0) > 4000) return false;
    if (!this._domSelectionIsSingleGrapheme()) return false;
    const editable = this._activeEditableFromSelection();
    if (!editable || editable.dataset.sourceForm !== "true") return false;
    if (editable.dataset.kind === "code-fence" || editable.dataset.kind === "image") return false;
    if (editable.querySelector?.("img, video, span")) return false;
    return true;
  }
  _syncAccentMetadata(previousValue, changes) {
    this._clearOffsetMap();
    const blocks = this._blocksForRender(previousValue, changes);
    this._liveBlocks = blocks;
    this._syncLiveMetadata(blocks);
    this._rebuildLiveIndex();
    this._sourceFormKeyShown = this._sourceFormKey();
  }
  _inputTargetFromBeforeInput(event) {
    const range = event?.getTargetRanges?.()[0];
    if (!range) return null;
    const startElement = range.startContainer?.nodeType === Node.ELEMENT_NODE
      ? range.startContainer
      : range.startContainer?.parentElement;
    const endElement = range.endContainer?.nodeType === Node.ELEMENT_NODE
      ? range.endContainer
      : range.endContainer?.parentElement;
    const editable = this._closestEditable(startElement);
    if (!editable || this._closestEditable(endElement) !== editable) return null;
    const start = this._sourceOffsetFromDom(editable, range.startContainer, range.startOffset);
    const end = this._sourceOffsetFromDom(editable, range.endContainer, range.endOffset);
    if (start == null || end == null) return null;
    return {
      editable,
      selection: {
        start: Math.min(start, end),
        end: Math.max(start, end),
        direction: start <= end ? "forward" : "backward"
      },
      text: this._plainText(editable)
    };
  }
  _applyFallbackBeforeInput(event, inputTarget) {
    const inputType = event?.inputType || "";
    const selection = this._selection;
    const editable = inputTarget?.editable || this._fallbackEditable;
    let start = Math.min(selection.start, selection.end);
    let end = Math.max(selection.start, selection.end);
    let insert = "";
    if (inputType.startsWith("insert") && event.data != null) {
      insert = normalizeLineEndings(event.data);
    } else if (inputType.startsWith("delete")) {
      if (start === end && inputTarget?.selection.start !== inputTarget?.selection.end) {
        start = inputTarget.selection.start;
        end = inputTarget.selection.end;
      } else if (start === end && inputType.includes("Backward") && start > 0) {
        start -= Array.from(this._value.slice(0, start)).at(-1)?.length || 1;
      } else if (start === end && inputType.includes("Forward") && end < this._value.length) {
        end += Array.from(this._value.slice(end))[0]?.length || 1;
      }
      if (start === end) return false;
    } else {
      return false;
    }
    if (editable?.dataset.editable === "cell") {
      const raw = this._plainText(editable);
      const displayStart = this._displayOffsetFromSourceOffset(editable, start);
      const displayEnd = this._displayOffsetFromSourceOffset(editable, end);
      const nextRaw = raw.slice(0, displayStart) + insert + raw.slice(displayEnd);
      const cellRange = this._editableSourceRange(editable);
      const inlineSource = cellRange ? unescapeTableCellText(this._value.slice(cellRange.from, cellRange.to)) : "";
      const rendered = editable.dataset.sourceForm === "true" || this._keepsActiveSource(editable)
        ? ""
        : renderInline(inlineSource, this._rendererOptions());
      const oldDisplay = rendered ? this._textFromHtml(rendered).replace(/\n/g, "") : inlineSource;
      let cellText = nextRaw;
      let cellCursor = displayStart + insert.length;
      if (editable.dataset.sourceForm !== "true" && !this._keepsActiveSource(editable) && oldDisplay !== inlineSource) {
        const edit = sourceEditFromDisplay(inlineSource, oldDisplay, nextRaw);
        cellText = edit.value;
        cellCursor = edit.cursor;
      }
      const tableEdit = this._tableCellInputEdit(editable, cellText, cellCursor);
      if (!tableEdit) return false;
      event.preventDefault();
      this._applyFallbackInput(
        inputType,
        diffTextChange(this._value, tableEdit.nextValue),
        { start: tableEdit.cursor, end: tableEdit.cursor, direction: "none" }
      );
      return true;
    }
    if (inputType.startsWith("insert")
      && ["virtual-code", "virtual-code-after", "virtual-hr-after", "virtual-setext-after", "virtual-table-after"].includes(editable?.dataset.editable)
      && !this._value.slice(0, start).endsWith("\n")) {
      insert = `\n${insert}`;
    }
    event.preventDefault();
    this._applyFallbackInput(inputType, [{ from: start, to: end, insert }], {
      start: start + insert.length,
      end: start + insert.length,
      direction: "none"
    });
    return true;
  }
  _applyFallbackInput(inputType, changes, selectionAfter) {
    const before = this._snapshot();
    const previousValue = this._value;
    this._value = applyTextChanges(previousValue, changes);
    this._selection = {
      start: clamp(selectionAfter.start, 0, this._value.length),
      end: clamp(selectionAfter.end, 0, this._value.length),
      direction: selectionAfter.direction || "none"
    };
    const after = makeSnapshot(
      this._value,
      this._selection.start,
      this._selection.end,
      this._selection.direction
    );
    this._recordUndo(before, after, this._undoGroupForInput(inputType), {
      coalesce: inputType === "insertText"
        || inputType === "deleteContentBackward"
        || inputType === "deleteContentForward"
    });
    this._redoStack.length = 0;
    this._afterValueChanged({
      source: "user",
      inputType,
      restoreSelection: true,
      previousValue,
      changes
    });
    if (!this._isComposing) this._scheduleCompletionUpdate();
  }
  _plainText(el) {
    if (!el) return "";
    let text = "";
    if (el.querySelector?.("button, .code-header, .line-numbers, img, video, [contenteditable='false']")) {
      const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, {
        acceptNode: (node) => this._isEditingText(node) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT,
      });
      while (walker.nextNode()) text += walker.currentNode.nodeValue;
    } else {
      text = el.textContent ?? "";
    }
    return text.replace(/\u00a0/g, " ").replace(/\n+$/g, "");
  }
  _editableSourceRange(editable) {
    if (!editable) return null;
    const from = this._liveFrom(editable);
    const to = this._liveTo(editable);
    if (!Number.isFinite(from) || !Number.isFinite(to)) return null;
    return { from, to: Math.max(from, to) };
  }
  _cellRawSource(editable) {
    const range = this._editableSourceRange(editable);
    return range && editable?.dataset?.editable === "cell" ? this._value.slice(range.from, range.to) : null;
  }
  _displayOffsetFromSourceOffset(editable, sourceOffset) {
    const range = this._editableSourceRange(editable);
    if (!range) return 0;
    const raw = this._cellRawSource(editable);
    const rel = clamp(Number(sourceOffset) - range.from, 0, range.to - range.from);
    if (raw == null) {
      const source = this._value.slice(range.from, range.to);
      const display = this._visibleText(editable);
      if (display === source) return clamp(rel, 0, display.length);
      return sourceOffsetToDisplayOffset(source, display, rel);
    }
    const unescaped = unescapeTableCellText(raw);
    const unescapedOffset = unescapedCellOffset(raw, rel);
    const display = this._visibleText(editable);
    if (display === unescaped) return clamp(unescapedOffset, 0, display.length);
    return sourceOffsetToDisplayOffset(unescaped, display, unescapedOffset);
  }
  _sourceOffsetFromDisplayOffset(editable, displayOffset) {
    const range = this._editableSourceRange(editable);
    if (!range) return null;
    const raw = this._cellRawSource(editable);
    const plain = this._visibleText(editable);
    const target = clamp(Number(displayOffset) || 0, 0, plain.length);
    if (raw == null) {
      const source = this._value.slice(range.from, range.to);
      if (plain === source) return clamp(range.from + target, range.from, Math.max(range.to, range.from + plain.length));
      return range.from + displayOffsetToSourceOffset(source, plain, target);
    }
    const unescaped = unescapeTableCellText(raw);
    const unescapedOffset = plain === unescaped ? target : displayOffsetToSourceOffset(unescaped, plain, target);
    return range.from + sourceIndexForUnescapedOffset(raw, unescapedOffset);
  }
  _closestEditable(target) { return target?.closest?.("[data-editable]") ?? null; }
  _fragmentIdForLink(link) {
    const href = link?.getAttribute?.("href")?.trim() ?? "";
    if (!href.startsWith("#") || href.length === 1) return "";
    try { return decodeURIComponent(href.slice(1)); } catch { return href.slice(1); }
  }
  _headingElementForId(surface, id) {
    return [...surface?.querySelectorAll?.(".md-heading[id], h1[id], h2[id], h3[id], h4[id]") || []]
      .find(heading => heading.id === id) || null;
  }
  _navigateFragmentLink(event, surface) {
    const link = event.target?.closest?.("a[href]");
    const id = this._fragmentIdForLink(link);
    if (!link || !surface?.contains(link) || !id) return false;
    let target = this._headingElementForId(surface, id);
    let label = target?.textContent?.trim() || "";
    if (surface === this._liveEditor) {
      const block = this._getBlocks().find(candidate => candidate.type === "heading" && candidate.heading?.id === id);
      if (!block) return false;
      label = block.heading.content;
      event.preventDefault();
      this.setSelectionRange(block.from, block.from, "none");
      target = this._headingElementForId(surface, id);
    } else {
      if (!target) return false;
      event.preventDefault();
    }
    target?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
    this._announce(`Navigated to ${label || id}.`);
    return true;
  }

  _onLiveClick(event) {
    if (this._suppressLiveClick) {
      this._suppressLiveClick = false;
      event.preventDefault();
      event.stopPropagation();
      return;
    }
    if (this._navigateFragmentLink(event, this._liveEditor)) return;
    this._structuredSelection = null;
    const tableControl = event.target.closest?.("[data-table-control]");
    if (tableControl) {
      event.preventDefault();
      this._handleTableControlClick(tableControl);
      return;
    }
    const checkbox = event.target.closest?.("[data-task-checkbox]");
    if (checkbox) {
      event.preventDefault();
      const offset = this._liveCheckOffset(checkbox);
      const current = this._value[offset] || " ";
      const next = current.toLowerCase() === "x" ? " " : "x";
      const source = event.detail === 0 ? "keyboard" : "pointer";
      const ctx = this._getContext();
      const result = ok(
        tx(
          ctx,
          "block.taskDone",
          [{ from: offset, to: offset + 1, insert: next }],
          this._selection,
          "block"
        ),
        next === "x" ? "Task checked." : "Task unchecked."
      );
      this._applyActionResult("block.taskDone", result, { source });
      if (source === "keyboard") [...this._liveEditor.querySelectorAll("[data-task-checkbox]")].find(node => this._liveCheckOffset(node) === offset)?.focus();
      return;
    }
    this._onSelectionChanged();
  }

  _onLiveMouseDown(event) {
    if (this.disabled || this.readonly || this.mode === "source" || event.button !== 0 || event.detail > 1) return;
    // Keep a control click from moving the caret or stealing focus.
    if (event.target.closest?.("[data-table-control]")) { event.preventDefault(); return; }
    if (event.target.closest?.("[data-task-checkbox]")) return;
    const anchor = this._sourceOffsetForClientPoint(event.clientX, event.clientY);
    if (anchor == null) return;
    const editable = this._liveEditableFromPoint(event.clientX, event.clientY);
    this._closeCompletion();
    if (this._liveSelectionAPI === false) {
      this._fallbackEditable = editable;
      this._fallbackSelectionPending = false;
      this._selection = { start: anchor, end: anchor, direction: "none" };
      this._pointerSelection = {
        anchor,
        focus: anchor,
        startX: event.clientX,
        startY: event.clientY,
        moved: false,
        native: true
      };
      this._bindPointerSelectionListeners();
      return;
    }
    event.preventDefault();
    this._pointerSelection = { anchor, focus: anchor, startX: event.clientX, startY: event.clientY, moved: false, native: false };
    this.setSelectionRange(anchor, anchor, "none");
    this._bindPointerSelectionListeners();
  }

  _bindPointerSelectionListeners() {
    this._boundLiveMouseMove ??= mouseEvent => this._onLiveMouseMove(mouseEvent);
    this._boundLiveMouseEnd ??= mouseEvent => this._onLiveMouseEnd(mouseEvent);
    const doc = this.ownerDocument || document;
    doc.addEventListener("mousemove", this._boundLiveMouseMove, true);
    doc.addEventListener("mouseup", this._boundLiveMouseEnd, true);
  }

  _onLiveMouseMove(event) {
    const state = this._pointerSelection;
    if (!state) return;
    const focus = this._sourceOffsetForClientPoint(event.clientX, event.clientY);
    if (focus == null) return;
    if (state.native) {
      state.focus = focus;
      state.moved = state.moved || Math.hypot(event.clientX - state.startX, event.clientY - state.startY) > 2;
      this._setFallbackPointerSelection(state.anchor, focus);
      this._updateSelectionToolbar();
      return;
    }
    event.preventDefault();
    state.focus = focus;
    state.moved = state.moved || Math.hypot(event.clientX - state.startX, event.clientY - state.startY) > 2;
    this._setLivePointerSelection(state.anchor, focus);
    this._updateSelectionToolbar();
  }

  _onLiveMouseEnd(event) {
    const state = this._pointerSelection;
    if (!state) return;
    const focus = this._sourceOffsetForClientPoint(event.clientX, event.clientY);
    if (focus != null) {
      if (state.native) this._setFallbackPointerSelection(state.anchor, focus);
      else this._setLivePointerSelection(state.anchor, focus);
    }
    this._suppressLiveClick = state.moved;
    if (this._suppressLiveClick) globalThis.setTimeout?.(() => { this._suppressLiveClick = false; }, 0);
    this._pointerSelection = null;
    const doc = this.ownerDocument || document;
    doc.removeEventListener("mousemove", this._boundLiveMouseMove, true);
    doc.removeEventListener("mouseup", this._boundLiveMouseEnd, true);
    if (!state.native) event.preventDefault();
    else {
      this._emitSelectionChange();
      if (!this._isComposing) this._scheduleCompletionUpdate();
    }
    this._syncSourceForm();
  }

  _setLivePointerSelection(anchor, focus) {
    const start = Math.min(anchor, focus);
    const end = Math.max(anchor, focus);
    const direction = anchor <= focus ? "forward" : "backward";
    this.setSelectionRange(start, end, direction);
  }

  _setFallbackPointerSelection(anchor, focus) {
    const start = Math.min(anchor, focus);
    const end = Math.max(anchor, focus);
    this._selection = {
      start,
      end,
      direction: anchor <= focus ? "forward" : "backward"
    };
  }

  _onNavigationKey(event) { if (NAVIGATION_KEYS.has(event.key)) this._onSelectionChanged(); }

  _maybeHandleLineBoundaryKey(event, activeCell = null, activeEditable = null) {
    if (event.defaultPrevented || event.altKey || event.ctrlKey || activeCell) return false;
    const isMac = /Mac|iPhone|iPad|iPod/.test(globalThis.navigator?.platform ?? "");
    let boundary = null;
    if (!event.metaKey && (event.key === "Home" || event.key === "End")) boundary = event.key === "Home" ? "start" : "end";
    else if (isMac && event.metaKey && (event.key === "ArrowLeft" || event.key === "ArrowRight")) boundary = event.key === "ArrowLeft" ? "start" : "end";
    if (!boundary) return false;
    const selection = this._getCurrentSelection();
    const focus = selection.direction === "backward" ? selection.start : selection.end;
    const anchor = selection.direction === "backward" ? selection.end : selection.start;
    const line = getLineRange(this._value, focus);
    const editableRange = !this._isSourceActive() ? this._editableSourceRange(activeEditable) : null;
    const target = boundary === "start" ? (editableRange?.from ?? line.start) : (editableRange?.to ?? line.end);
    event.preventDefault();
    if (event.shiftKey) this._setSourceBackedSelection(anchor, target);
    else this.setSelectionRange(target, target, "none");
    return true;
  }

  _maybeHandleLiveArrowKey(event, activeEditable = null) {
    if (this._isSourceActive() || event.defaultPrevented || event.altKey || event.metaKey || event.ctrlKey) return false;
    if (!LIVE_ARROW_KEYS.has(event.key)) return false;
    const editable = activeEditable || this._activeEditableFromEvent(event);
    if (!editable || editable.dataset.editable === "cell") return false;
    const selection = this._getLiveSelection() || this._getCurrentSelection();
    if (!selection) return false;
    if (selection.start !== selection.end && !event.shiftKey) {
      const target = (event.key === "ArrowLeft" || event.key === "ArrowUp") ? selection.start : selection.end;
      event.preventDefault();
      this.setSelectionRange(target, target, "none");
      return true;
    }
    if (event.shiftKey) return this._maybeExtendLiveArrowSelection(event, selection);
    if (selection.start !== selection.end) return false;
    const direction = (event.key === "ArrowLeft" || event.key === "ArrowUp") ? -1 : 1;
    let target = (event.key === "ArrowLeft" || event.key === "ArrowRight")
      ? this._horizontalArrowTarget(editable, selection.start, direction)
      : this._verticalArrowTarget(editable, selection.start, direction);
    if (target == null && this._liveSelectionAPI === false) {
      target = (event.key === "ArrowLeft" || event.key === "ArrowRight")
        ? this._fallbackHorizontalArrowTarget(editable, selection.start, direction)
        : this._fallbackVerticalArrowTarget(editable, selection.start, direction);
    }
    if (target == null || target === selection.start) return false;
    event.preventDefault();
    this.setSelectionRange(target, target, "none");
    return true;
  }

  _maybeExtendLiveArrowSelection(event, selection = this._getCurrentSelection()) {
    const focus = selection.direction === "backward" ? selection.start : selection.end;
    const anchor = selection.direction === "backward" ? selection.end : selection.start;
    const pos = this._domPositionFromSource(focus);
    const editable = pos?.editable;
    if (!editable || editable.dataset.editable === "cell") return false;
    const direction = (event.key === "ArrowLeft" || event.key === "ArrowUp") ? -1 : 1;
    let target = (event.key === "ArrowLeft" || event.key === "ArrowRight")
      ? this._horizontalArrowTarget(editable, focus, direction)
      : this._verticalArrowTarget(editable, focus, direction);
    if (target == null && this._liveSelectionAPI === false) {
      target = (event.key === "ArrowLeft" || event.key === "ArrowRight")
        ? this._fallbackHorizontalArrowTarget(editable, focus, direction)
        : this._fallbackVerticalArrowTarget(editable, focus, direction);
    }
    if (target == null || target === focus) return false;
    event.preventDefault();
    this._setSourceBackedSelection(anchor, target);
    return true;
  }

  _setSourceBackedSelection(anchor, focus) {
    const start = Math.min(anchor, focus);
    const end = Math.max(anchor, focus);
    const direction = anchor <= focus ? "forward" : "backward";
    this.setSelectionRange(start, end, direction);
  }

  _horizontalArrowTarget(editable, offset, direction) {
    const from = this._liveFrom(editable);
    const to = this._liveTo(editable);
    if (!Number.isFinite(from) || !Number.isFinite(to)) return null;
    if (direction < 0 && offset <= from) {
      const previous = this._adjacentLiveEditable(editable, -1);
      if (previous) return this._liveTo(previous);
      return this._virtualState.active ? this._sourceNeighborOffset(offset, -1) : null;
    }
    if (direction > 0 && offset >= to) {
      const next = this._adjacentLiveEditable(editable, 1);
      if (next) return this._liveFrom(next);
      return this._virtualState.active ? this._sourceNeighborOffset(offset, 1) : null;
    }
    return null;
  }

  _fallbackHorizontalArrowTarget(editable, offset, direction) {
    const from = this._liveFrom(editable);
    const to = this._liveTo(editable);
    if (!Number.isFinite(from) || !Number.isFinite(to)) return null;
    if (direction < 0 && offset > from) {
      return offset - (Array.from(this._value.slice(from, offset)).at(-1)?.length || 1);
    }
    if (direction > 0 && offset < to) {
      return offset + (Array.from(this._value.slice(offset, to))[0]?.length || 1);
    }
    return null;
  }

  _fallbackVerticalArrowTarget(editable, offset, direction) {
    const caret = this._caretRectForSourceOffset(offset, editable);
    const box = editable.getBoundingClientRect();
    if (!caret || !box || box.height === 0) return null;
    const lineHeight = this._computedLineHeight(editable);
    const clientX = caret.left;
    const clientY = ((caret.top + caret.bottom) / 2) + direction * lineHeight;
    if (clientY <= box.top || clientY >= box.bottom) return null;
    const fromPoint = this._sourceOffsetFromPoint(editable, clientX, clientY);
    if (fromPoint != null) return fromPoint;
    return this._nearestSourceOffsetInEditable(editable, clientX, clientY);
  }

  _sourceNeighborOffset(offset, direction) {
    const blocks = this._liveBlocks?.length ? this._liveBlocks : this._getBlocks();
    const index = this._blockIndexForOffset(blocks, offset);
    const block = blocks[index];
    if (!block) return null;
    if (block.type === "code-fence") {
      const rows = this._fenceRows(block);
      let row = rows.findIndex(line => offset >= line.start && offset <= line.end);
      if (row < 0) row = direction > 0 ? -1 : rows.length;
      const next = rows[row + direction];
      if (next) return direction < 0 ? next.end : next.start;
    }
    const neighbor = blocks[index + direction];
    if (!neighbor) return null;
    if (direction < 0) return neighbor.line?.end ?? neighbor.to ?? neighbor.from ?? 0;
    return neighbor.line?.start ?? neighbor.from ?? 0;
  }
  _virtualVerticalNeighbor(editable, offset, direction) {
    if (!this._virtualState.active) return null;
    const neighbor = this._sourceNeighborOffset(offset, direction);
    if (neighbor == null) return null;
    const blocks = this._liveBlocks?.length ? this._liveBlocks : this._getBlocks();
    if (!this._isSourceOffsetRendered(neighbor)) {
      this._renderLiveVirtual(blocks, { anchorOffset: neighbor, force: true });
      this._rebuildLiveIndex();
    }
    const target = this._domPositionFromSource(neighbor)?.editable;
    if (!target) return neighbor;
    if (this._isSingleVisualRow(editable) && this._isSingleVisualRow(target)) {
      const sourceColumn = clamp(offset - this._liveFrom(editable), 0, this._plainText(editable).length);
      return this._liveFrom(target) + Math.min(sourceColumn, this._plainText(target).length);
    }
    const caret = this._caretRectForSourceOffset(offset, editable);
    return this._sourceOffsetInEditableAtX(target, caret?.left ?? target.getBoundingClientRect().left, direction);
  }
  _verticalArrowTarget(editable, offset, direction) {
    if (!this._isCaretOnVisualBoundary(editable, offset, direction)) return null;
    const targetEditable = this._adjacentLiveEditable(editable, direction);
    if (!targetEditable) return this._virtualVerticalNeighbor(editable, offset, direction);
    if (this._isSingleVisualRow(editable) && this._isSingleVisualRow(targetEditable)) {
      const sourceColumn = clamp(offset - this._liveFrom(editable), 0, this._plainText(editable).length);
      return this._liveFrom(targetEditable) + Math.min(sourceColumn, this._plainText(targetEditable).length);
    }
    const caretRect = this._caretRectForSourceOffset(offset, editable);
    const fallbackRect = editable.getBoundingClientRect();
    const clientX = caretRect?.left ?? fallbackRect.left;
    return this._sourceOffsetInEditableAtX(targetEditable, clientX, direction);
  }

  _isSingleVisualRow(editable) {
    const box = editable.getBoundingClientRect();
    if (!box || box.height === 0) return true;
    return box.height <= this._computedLineHeight(editable) * 1.65;
  }

  _rebuildLiveIndex() {
    const editables = [...this._liveEditor.querySelectorAll("[data-editable]")]
      .filter(el => Number.isFinite(Number(el.dataset.from)) && Number.isFinite(Number(el.dataset.to)))
      .sort((a, b) => this._liveFrom(a) - this._liveFrom(b));
    this._liveEditablesCache = editables;
    this._liveNavigationCache = editables.filter(el => el.dataset.editable !== "cell");
    this._liveIndexDirty = false;
  }
  _liveEditables() {
    if (this._liveIndexDirty) this._rebuildLiveIndex();
    return this._liveEditablesCache || [];
  }
  _liveNavigationEditables() {
    if (this._liveIndexDirty) this._rebuildLiveIndex();
    return this._liveNavigationCache || [];
  }

  _adjacentLiveEditable(editable, direction) {
    const editables = this._liveNavigationEditables();
    const index = editables.indexOf(editable);
    return index === -1 ? null : editables[index + direction] || null;
  }

  _computedLineHeight(el) {
    const style = globalThis.getComputedStyle?.(el);
    const parsed = Number.parseFloat(style?.lineHeight || "");
    if (Number.isFinite(parsed)) return parsed;
    const fontSize = Number.parseFloat(style?.fontSize || "");
    return Number.isFinite(fontSize) ? fontSize * 1.2 : 18;
  }

  _isCaretOnVisualBoundary(editable, offset, direction) {
    const from = this._liveFrom(editable);
    const to = this._liveTo(editable);
    const rect = this._caretRectForSourceOffset(offset, editable);
    const box = editable.getBoundingClientRect();
    if (!rect || !box || box.height === 0) return direction < 0 ? offset <= from : offset >= to;
    const tolerance = this._computedLineHeight(editable) * 0.65;
    return direction < 0
      ? rect.top <= box.top + tolerance
      : rect.bottom >= box.bottom - tolerance;
  }

  _caretRectForSourceOffset(offset, preferredEditable = null) {
    const pos = preferredEditable
      ? this._textPositionInElement(preferredEditable, this._displayOffsetFromSourceOffset(preferredEditable, offset))
      : this._domPositionFromSource(offset);
    if (!pos) return null;
    return this._caretRectFromDomPosition(pos.node, pos.offset);
  }

  _caretRectFromDomPosition(node, offset) {
    const range = document.createRange();
    try { range.setStart(node, offset); } catch { return null; }
    range.collapse(true);
    const rect = range.getClientRects()[0] || range.getBoundingClientRect();
    return rect && Number.isFinite(rect.left) && (rect.height > 0 || rect.width > 0) ? rect : null;
  }

  _sourceOffsetInEditableAtX(editable, clientX, direction) {
    const from = this._liveFrom(editable);
    const to = this._liveTo(editable);
    if (!Number.isFinite(from) || !Number.isFinite(to)) return null;
    const box = editable.getBoundingClientRect();
    if (!box || box.width === 0 || box.height === 0) return direction < 0 ? to : from;
    const lineHeight = this._computedLineHeight(editable);
    const x = clamp(clientX, box.left + 1, box.right - 1);
    const rowOffset = Math.min(Math.max(lineHeight / 2, 1), Math.max(box.height / 2, 1));
    const y = direction < 0 ? box.bottom - rowOffset : box.top + rowOffset;
    const fromPoint = this._sourceOffsetFromPoint(editable, x, y);
    if (fromPoint != null) return clamp(fromPoint, from, to);
    return this._nearestSourceOffsetInEditable(editable, x, y);
  }

  _sourceOffsetFromPoint(editable, clientX, clientY) {
    const doc = editable.ownerDocument || document;
    let node = null;
    let offset = 0;
    if (doc.caretPositionFromPoint) {
      try {
        const pos = doc.caretPositionFromPoint(clientX, clientY, { shadowRoots: [this._shadow] });
        if (pos) { node = pos.offsetNode; offset = pos.offset; }
      } catch {
        const pos = doc.caretPositionFromPoint(clientX, clientY);
        if (pos) { node = pos.offsetNode; offset = pos.offset; }
      }
    }
    if (!node && doc.caretRangeFromPoint) {
      const range = doc.caretRangeFromPoint(clientX, clientY);
      if (range) { node = range.startContainer; offset = range.startOffset; }
    }
    if (!node || (node !== editable && !editable.contains(node))) return null;
    return this._sourceOffsetFromDom(editable, node, offset);
  }

  _liveEditableFromPoint(clientX, clientY) {
    const direct = this._shadow.elementFromPoint?.(clientX, clientY);
    const directEditable = this._closestEditable(direct);
    if (directEditable) return directEditable;
    const editables = this._liveEditables();
    let best = null;
    let bestScore = Infinity;
    for (const el of editables) {
      const rect = el.getBoundingClientRect();
      if (!rect || rect.width === 0 || rect.height === 0) continue;
      const dx = clientX < rect.left ? rect.left - clientX : clientX > rect.right ? clientX - rect.right : 0;
      const dy = clientY < rect.top ? rect.top - clientY : clientY > rect.bottom ? clientY - rect.bottom : 0;
      const score = dy * 10000 + dx;
      if (score < bestScore) { bestScore = score; best = el; }
    }
    return best;
  }

  _sourceOffsetForClientPoint(clientX, clientY) {
    if (this._virtualState.active) {
      const virtual = this._sourceOffsetForVirtualPoint(clientX, clientY);
      if (virtual != null) return virtual;
    }
    const editable = this._liveEditableFromPoint(clientX, clientY);
    if (!editable) return null;
    return this._sourceOffsetForEditablePoint(editable, clientX, clientY);
  }

  _nearestSourceOffsetInEditable(editable, clientX, clientY) {
    const from = this._liveFrom(editable);
    const length = this._plainText(editable).length;
    if (!length) return from;
    let bestOffset = 0;
    let bestScore = Infinity;
    for (let offset = 0; offset <= length; offset += 1) {
      const pos = this._textPositionInElement(editable, offset);
      const rect = this._caretRectFromDomPosition(pos.node, pos.offset);
      if (!rect) continue;
      const rowDistance = Math.abs(((rect.top + rect.bottom) / 2) - clientY);
      const columnDistance = Math.abs(rect.left - clientX);
      const score = (rowDistance * 1000) + columnDistance;
      if (score < bestScore) { bestScore = score; bestOffset = offset; }
    }
    return this._sourceOffsetFromDisplayOffset(editable, bestOffset) ?? from;
  }
  _sourceOffsetForEditablePoint(editable, clientX, clientY) {
    if (!editable) return null;
    const rect = editable.getBoundingClientRect();
    if (!rect || rect.width === 0 || rect.height === 0) return this._liveFrom(editable);
    const x = clamp(clientX, rect.left + 1, rect.right - 1);
    const y = clamp(clientY, rect.top + 1, rect.bottom - 1);
    const fromPoint = this._sourceOffsetFromPoint(editable, x, y);
    if (fromPoint != null) return clamp(fromPoint, this._liveFrom(editable), this._liveTo(editable));
    return this._nearestSourceOffsetInEditable(editable, x, y);
  }
  _sourceOffsetForVirtualPoint(clientX, clientY) {
    const editor = this._liveEditor;
    const blocks = this._liveBlocks?.length ? this._liveBlocks : this._getBlocks();
    if (!editor || !blocks?.length) return null;
    const direct = this._shadow.elementFromPoint?.(clientX, clientY);
    if (this._closestEditable(direct)) return null;
    if (direct && direct !== editor && !editor.contains(direct)) return null;
    const y = this._contentYFromClient(clientY);
    const tops = this._blockTops(blocks);
    const index = this._indexAtContentY(tops, Math.max(0, y));
    const offset = this._offsetForContentY(blocks, index, tops, y);
    if (!this._isSourceOffsetRendered(offset)) {
      this._renderLiveVirtual(blocks, { anchorOffset: offset, anchorIndex: index, force: true });
      this._liveBlocks = blocks;
      this._rebuildLiveIndex();
    }
    const hit = this._closestEditable(this._shadow.elementFromPoint?.(clientX, clientY));
    if (hit) return this._sourceOffsetForEditablePoint(hit, clientX, clientY);
    const nodes = [...editor.querySelectorAll(`[data-virtual-block="${index}"] [data-editable], [data-virtual-block="${index}"][data-editable]`)];
    if (!nodes.length) return blocks[index]?.line?.start ?? blocks[index]?.from ?? 0;
    let best = nodes[0];
    let bestDist = Infinity;
    for (const node of nodes) {
      const rect = node.getBoundingClientRect();
      const dist = Math.abs(((rect.top + rect.bottom) / 2) - clientY);
      if (dist < bestDist) { bestDist = dist; best = node; }
    }
    const rect = best.getBoundingClientRect();
    return this._sourceOffsetForEditablePoint(
      best,
      clamp(clientX, rect.left + 1, Math.max(rect.left + 1, rect.right - 1)),
      clamp(clientY, rect.top + 1, Math.max(rect.top + 1, rect.bottom - 1)),
    );
  }

  _onSelectionChanged() {
    if (this._virtualPainting) return;
    if (this._sourceFormLock) {
      if (this._ignoreSelectionChangeCount > 0) this._ignoreSelectionChangeCount -= 1;
      return;
    }
    // The accent menu selects the candidate in the DOM before it is in the
    // model. Mapping that range back turns it into the rest of the line.
    if (this._isComposing || this._sourceFormDomDirty()) {
      if (this._ignoreSelectionChangeCount > 0) this._ignoreSelectionChangeCount -= 1;
      return;
    }
    if (this._ignoreSelectionChangeCount > 0) {
      this._ignoreSelectionChangeCount -= 1;
      this._emitSelectionChange();
      if (!this._isComposing) this._scheduleCompletionUpdate();
      this._updateSelectionToolbar();
      return;
    }
    this._structuredSelection = null;
    this._selection = this._getCurrentSelection();
    if (!this._isComposing) this._syncSourceForm();
    this._emitSelectionChange();
    if (!this._isComposing) this._scheduleCompletionUpdate();
    this._updateSelectionToolbar();
    this._armSpellcheck();
    if (this._virtualState.active) this._markVirtualCaret();
  }

  _hideSelectionToolbar() {
    if (this._selectionToolbar && !this._selectionToolbar.hidden) this._selectionToolbar.hidden = true;
  }

  // Show the floating formatting toolbar above a non-empty selection in the live
  // editor. Hidden in source mode, while composing, when the slash
  // completion popup is open, when the editor isn't focused, or with no selection.
  _updateSelectionToolbar() {
    if (!this._selectionToolbar) return;
    const sel = this._selection;
    const hasRange = sel && sel.start !== sel.end;
    const canShow = hasRange && !this.disabled && !this._isComposing
      && this.mode !== "source"
      && !this._completion.open && this._focusWithin;
    if (!canShow) { this._hideSelectionToolbar(); return; }
    const wasHidden = this._selectionToolbar.hidden;
    this._selectionToolbar.hidden = false;
    // First appearance jumps to place; later moves glide via CSS transition.
    if (wasHidden) {
      this._selectionToolbar.setAttribute("data-instant", "");
      this._positionSelectionToolbar();
      requestAnimationFrame(() => this._selectionToolbar?.removeAttribute("data-instant"));
      return;
    }
    this._positionSelectionToolbar();
  }

  _positionSelectionToolbar() {
    const shell = this._shadow.querySelector(".editor-shell");
    if (!shell || !this._selectionToolbar || this._selectionToolbar.hidden) return;
    const shellRect = shell.getBoundingClientRect();
    let rect = null;
    try { const sel = this._shadow.getSelection?.() || globalThis.getSelection?.(); if (sel?.rangeCount) rect = sel.getRangeAt(0).getBoundingClientRect(); } catch {}
    if (!rect || (!rect.width && !rect.height)) {
      const startRect = this._caretRectForSourceOffset(Math.min(this._selection.start, this._selection.end));
      const endRect = this._caretRectForSourceOffset(Math.max(this._selection.start, this._selection.end));
      if (startRect && endRect) rect = { left: Math.min(startRect.left, endRect.left), right: Math.max(startRect.right, endRect.right), top: Math.min(startRect.top, endRect.top), bottom: Math.max(startRect.bottom, endRect.bottom), width: 1, height: startRect.height };
    }
    if (!rect) { this._hideSelectionToolbar(); return; }
    const toolbarRect = this._selectionToolbar.getBoundingClientRect();
    const centerX = (rect.left + rect.right) / 2 - shellRect.left;
    let left = clamp(centerX - toolbarRect.width / 2, 4, Math.max(4, shellRect.width - toolbarRect.width - 4));
    // Prefer above the selection; drop below if there isn't room.
    let top = rect.top - shellRect.top - toolbarRect.height - 8;
    if (top < 4) top = rect.bottom - shellRect.top + 8;
    top = clamp(top, 4, Math.max(4, shellRect.height - toolbarRect.height - 4));
    this._selectionToolbar.style.left = `${left}px`;
    this._selectionToolbar.style.top = `${top}px`;
  }

  _isSourceActive() { return this._shadow.activeElement === this._sourceTextarea || this.mode === "source"; }
  _focusEditable(options) {
    if (this.disabled) return;
    if (this.mode === "source") { this._sourceTextarea?.focus(options); this._sourceTextarea?.setSelectionRange(this._selection.start, this._selection.end, this._selection.direction); return; }
    this._liveEditor?.focus(options); this._restoreLiveSelection(this._selection);
  }
  _modelSelection() {
    const sel = this._selection || { start: 0, end: 0, direction: "none" };
    return {
      start: clamp(sel.start, 0, this._value.length),
      end: clamp(sel.end, 0, this._value.length),
      direction: sel.direction || "none",
    };
  }
  _getCurrentSelection() {
    if (this._isSourceActive() && this._sourceTextarea) return { start: this._sourceTextarea.selectionStart, end: this._sourceTextarea.selectionEnd, direction: this._sourceTextarea.selectionDirection || "none" };
    if (this._isComposing || this._sourceFormDomDirty()) return this._modelSelection();
    if (this._structuredSelection) return { ...this._structuredSelection };
    const live = this._getLiveSelection();
    return live || this._modelSelection();
  }
  _getLiveSelection(preferredEditable = null) {
    const exposed = this._readLiveSelection(preferredEditable);
    if (exposed) return exposed;
    if (this._liveSelectionAPI !== false || !this._selection) return null;
    if (preferredEditable) {
      const range = this._editableSourceRange(preferredEditable);
      if (!range
        || this._selection.start < range.from
        || this._selection.end > range.to) return null;
    }
    return { ...this._selection };
  }
  _readLiveSelection(preferredEditable = null) {
    const sel = this._exposedLiveSelection();
    if (!sel) return null;
    const anchorEditable = preferredEditable || this._closestEditable(sel.anchorNode?.nodeType === Node.ELEMENT_NODE ? sel.anchorNode : sel.anchorNode?.parentElement);
    const focusEditable = preferredEditable || this._closestEditable(sel.focusNode?.nodeType === Node.ELEMENT_NODE ? sel.focusNode : sel.focusNode?.parentElement);
    if (!anchorEditable || !focusEditable) return null;
    if (preferredEditable
      && ((!preferredEditable.contains(sel.anchorNode) && preferredEditable !== sel.anchorNode)
        || (!preferredEditable.contains(sel.focusNode) && preferredEditable !== sel.focusNode))) return null;
    const start = this._sourceOffsetFromDom(anchorEditable, sel.anchorNode, sel.anchorOffset);
    const end = this._sourceOffsetFromDom(focusEditable, sel.focusNode, sel.focusOffset);
    if (start == null || end == null) return null;
    return { start: Math.min(start, end), end: Math.max(start, end), direction: start <= end ? "forward" : "backward" };
  }
  _exposedLiveSelection() {
    if (!this._liveEditor) return null;
    const selections = [];
    try {
      if (typeof this._shadow?.getSelection === "function") {
        selections.push(this._shadow.getSelection());
      }
    } catch {}
    try {
      const selection = globalThis.getSelection?.();
      if (selection && !selections.includes(selection)) selections.push(selection);
    } catch {}
    return selections.find(selection => {
      if (!selection || selection.rangeCount === 0) return false;
      const anchor = selection.anchorNode;
      const focus = selection.focusNode;
      return Boolean(
        anchor
        && focus
        && (anchor === this._liveEditor || this._liveEditor.contains(anchor))
        && (focus === this._liveEditor || this._liveEditor.contains(focus))
      );
    }) || null;
  }
  _displayOffsetFromSelection(editable) {
    const sel = this._exposedLiveSelection();
    const node = sel?.focusNode;
    if (!sel || sel.rangeCount === 0 || !node || (node !== editable && !editable.contains(node))) return null;
    const range = document.createRange();
    range.selectNodeContents(editable);
    try { range.setEnd(node, sel.focusOffset); } catch { return null; }
    const text = range.toString().replace(/\u00a0/g, " ");
    return this._keepsSourceNewlines(editable) ? text.length : text.replace(/\n/g, "").length;
  }
  _editingOffset(editable, endNode, endOffset) {
    const walker = document.createTreeWalker(editable, NodeFilter.SHOW_TEXT, {
      acceptNode: (node) => this._isEditingText(node) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT,
    });
    let count = 0;
    while (walker.nextNode()) {
      const node = walker.currentNode;
      const raw = node.nodeValue.replace(/\u00a0/g, " ");
      const value = this._keepsSourceNewlines(editable) ? raw : raw.replace(/\n/g, "");
      if (node === endNode) return count + Math.min(endOffset, value.length);
      count += value.length;
    }
    return count;
  }
  _sourceOffsetFromDom(editable, node, offset) {
    const from = this._liveFrom(editable); if (!Number.isFinite(from)) return null;
    if (node?.nodeType === Node.TEXT_NODE) {
      return this._sourceOffsetFromDisplayOffset(editable, this._editingOffset(editable, node, offset)) ?? from;
    }
    const range = document.createRange(); range.selectNodeContents(editable);
    try { range.setEnd(node, offset); } catch { return from; }
    let text = range.toString().replace(/\u00a0/g, " ");
    if (!this._keepsSourceNewlines(editable)) text = text.replace(/\n/g, "");
    return this._sourceOffsetFromDisplayOffset(editable, text.length) ?? from;
  }
  _restoreLiveSelection(selection = this._selection) {
    if (!this._liveEditor || this.mode === "source") return;
    if (this._virtualState.active && !this._ensureVirtualSelectionVisible(selection)) { this._liveEditor.focus(); return; }
    const startPos = this._domPositionFromSource(selection.start);
    const endPos = this._domPositionFromSource(selection.end);
    if (!startPos || !endPos) { this._liveEditor.focus(); return; }
    if (!this._isLiveDomPositionConnected(startPos) || !this._isLiveDomPositionConnected(endPos)) { this._liveEditor.focus(); return; }
    const focusEditable = selection.direction === "backward" ? startPos.editable : endPos.editable;
    const range = document.createRange();
    try {
      range.setStart(startPos.node, startPos.offset);
      range.setEnd(endPos.node, endPos.offset);
    } catch {
      this._liveEditor.focus();
      return;
    }
    let sel = null;
    try {
      sel = typeof this._shadow?.getSelection === "function"
        ? this._shadow.getSelection()
        : globalThis.getSelection?.();
    } catch {}
    if (!sel) {
      this._useFallbackLiveSelection(focusEditable || startPos.editable);
      return;
    }
    sel.removeAllRanges();
    try {
      if (selection.start !== selection.end && typeof sel.setBaseAndExtent === "function") {
        if (selection.direction === "backward") sel.setBaseAndExtent(endPos.node, endPos.offset, startPos.node, startPos.offset);
        else sel.setBaseAndExtent(startPos.node, startPos.offset, endPos.node, endPos.offset);
      } else {
        sel.addRange(range);
      }
    } catch {
      sel.removeAllRanges();
      try {
        if (this._isLiveDomPositionConnected(startPos) && this._isLiveDomPositionConnected(endPos)) sel.addRange(range);
      } catch {}
    }
    if (!this._readLiveSelection()) {
      this._useFallbackLiveSelection(focusEditable || startPos.editable);
      return;
    }
    this._liveSelectionAPI = true;
    this._fallbackEditable = null;
    this._fallbackSelectionPending = false;
    (focusEditable || startPos.editable)?.focus?.();
  }
  _useFallbackLiveSelection(editable) {
    this._liveSelectionAPI = false;
    this._fallbackEditable = editable || this._domPositionFromSource(this._selection.end)?.editable || null;
    this._fallbackSelectionPending = true;
    if (this._liveEditor) this._liveEditor.contentEditable = "false";
    const focusTarget = this._fallbackEditable || this._liveEditor;
    focusTarget?.blur();
    focusTarget?.focus();
  }
  _isLiveDomPositionConnected(pos) {
    return Boolean(pos?.node?.isConnected && pos?.editable?.isConnected && this._liveEditor?.contains(pos.editable));
  }
  _domPositionFromSource(offset) {
    const safe = clamp(offset, 0, this._value.length);
    if (this._virtualState.active && !this._isSourceOffsetRendered(safe)) {
      const blocks = this._liveBlocks?.length ? this._liveBlocks : this._getBlocks();
      this._renderLiveVirtual(blocks, { anchorOffset: safe, force: true });
      this._rebuildLiveIndex();
    }
    const editables = this._liveEditables();
    if (!editables.length) return null;
    let previous = null;
    for (const el of editables) {
      const from = this._liveFrom(el);
      const to = this._liveTo(el);
      if (safe < from) {
        if (!previous) return this._textPositionInElement(el, 0);
        const prevTo = this._liveTo(previous);
        return (safe - prevTo <= from - safe)
          ? this._textPositionInElement(previous, this._plainText(previous).length)
          : this._textPositionInElement(el, 0);
      }
      if (safe >= from && safe <= to) return this._textPositionInElement(el, this._displayOffsetFromSourceOffset(el, safe));
      previous = el;
    }
    return this._textPositionInElement(previous, this._plainText(previous).length);
  }
  _textPositionInElement(el, offset) {
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, {
      acceptNode: (node) => this._isEditingText(node) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT,
    });
    let remaining = clamp(offset, 0, this._plainText(el).length);
    let lastText = null;
    while (walker.nextNode()) {
      const node = walker.currentNode; const len = node.nodeValue.length; lastText = node;
      if (remaining <= len) return { node, offset: remaining, editable: el };
      remaining -= len;
    }
    if (lastText) return { node: lastText, offset: lastText.nodeValue.length, editable: el };
    if (el.firstChild && el.firstChild.nodeName === "BR") return { node: el, offset: 0, editable: el };
    const text = document.createTextNode(""); el.appendChild(text); return { node: text, offset: 0, editable: el };
  }
  _ensureEmptyLiveEditable() {
    if (this._value.length !== 0 || !this._liveEditor) return;
    if (!this._liveEditor.querySelector("[data-editable]")) {
      const blocks = this._getBlocks();
      this._renderLiveFull(blocks);
      this._liveBlocks = blocks;
      this._rebuildLiveIndex();
    }
    this._selection = { start: 0, end: 0, direction: "none" };
    this._restoreLiveSelection(this._selection);
  }

  _snapshot() { const sel = this._getCurrentSelection(); this._selection = sel; return makeSnapshot(this._value, sel.start, sel.end, sel.direction || "none"); }
  _recordUndo(before, after, group, { coalesce = false } = {}) {
    if (!before || !after) return; if (before.value === after.value && sameSelection(before.selection, after.selection)) return;
    const latest = this._undoStack[this._undoStack.length - 1]; const timestamp = now();
    if (coalesce && latest && latest.group === group && timestamp - latest.timestamp < 900) { latest.after = after; latest.timestamp = timestamp; return; }
    this._undoStack.push({ before, after, group, timestamp }); if (this._undoStack.length > this._maxUndo) this._undoStack.shift();
  }
  _undoGroupForInput(inputType) { if (inputType?.startsWith("insert")) return "typing"; if (inputType?.startsWith("delete")) return "delete"; return inputType || "input"; }
  _undo() { const entry = this._undoStack.pop(); if (!entry) return false; const current = this._snapshot(); this._redoStack.push({ before: entry.before, after: current, group: entry.group, timestamp: now() }); this._restoreSnapshot(entry.before, "undo"); this._dispatch("md-action", { actionId: "history.undo", source: "keyboard", before: current, after: this._snapshot() }); return true; }
  _redo() { const entry = this._redoStack.pop(); if (!entry) return false; const current = this._snapshot(); this._undoStack.push({ before: current, after: entry.after, group: entry.group, timestamp: now() }); this._restoreSnapshot(entry.after, "redo"); this._dispatch("md-action", { actionId: "history.redo", source: "keyboard", before: current, after: this._snapshot() }); return true; }
  _restoreSnapshot(snapshot, source) {
    const previousValue = this._value;
    this._value = snapshot.value;
    this._selection = { ...snapshot.selection };
    this._redoStack = this._redoStack;
    this._afterValueChanged({ source, restoreSelection: true, previousValue, changes: diffTextChange(previousValue, this._value) });
  }

  _maybeHandleMarkerPair(event) {
    if (this.readonly || this.disabled || this._isComposing || this._completion.open) return false;
    if (event.ctrlKey || event.metaKey || event.altKey) return false;
    let action = "";
    let key = "";
    if (event.key === "Backspace") action = "backspace";
    else if (event.key === "Delete") action = "delete";
    else if (event.key === " ") action = "space";
    else if (event.key.length === 1) { action = "insert"; key = event.key; }
    else return false;
    const selection = this._getCurrentSelection();
    const edit = markerPairEdit(this._value, selection.start, selection.end, key, action);
    if (!edit) return false;
    event.preventDefault();
    if (edit.insert === "" && edit.from === edit.to) {
      this.setSelectionRange(edit.start, edit.end);
      return true;
    }
    const ctx = this._getContext();
    const direction = edit.start === edit.end ? "none" : "forward";
    const result = ok(tx(
      ctx,
      "editor.markerPair",
      [{ from: edit.from, to: edit.to, insert: edit.insert }],
      { start: edit.start, end: edit.end, direction },
      action === "backspace" || action === "delete" ? "delete" : "typing"
    ));
    this._applyActionResult("editor.markerPair", result, { source: "keyboard" });
    return true;
  }

  _onCompositionStart() {
    if (this._compositionEndTimer) {
      clearTimeout(this._compositionEndTimer);
      this._compositionEndTimer = 0;
    }
    this._compositionStamp += 1;
    this._compositionEnding = false;
    this._isComposing = true;
    this._closeCompletion();
    this._beforeInputTarget = null;
    const sel = this._modelSelection();
    this._beforeInputSnapshot = makeSnapshot(this._value, sel.start, sel.end, sel.direction || "none");
  }

  _onCompositionEnd(event) {
    const stamp = this._compositionStamp;
    this._compositionEnding = true;
    this._suppressEnterUntil = now() + 40;
    if (!this._isComposing) {
      this._isComposing = true;
      if (!this._beforeInputSnapshot) {
        const sel = this._modelSelection();
        this._beforeInputSnapshot = makeSnapshot(this._value, sel.start, sel.end, sel.direction || "none");
      }
    }
    if (this._compositionEndTimer) clearTimeout(this._compositionEndTimer);
    this._compositionEndTimer = setTimeout(() => {
      this._compositionEndTimer = 0;
      this._compositionEnding = false;
      if (stamp !== this._compositionStamp) return;
      const pending = this._isComposing;
      this._isComposing = false;
      if (pending) this._commitComposition(event);
    }, 0);
  }

  _commitComposition(event) {
    const target = event?.target;
    if (target === this._sourceTextarea || (this._isSourceActive() && this._sourceTextarea)) {
      this._onSourceInput({ inputType: "insertCompositionText" });
      return;
    }
    let editable = null;
    if (target && target.isConnected !== false) {
      editable = this._closestEditable(target) || (target.matches?.("[data-editable]") ? target : null);
    }
    if (!editable) editable = this._activeEditableFromSelection();
    if (!editable) {
      const sel = this._exposedLiveSelection();
      const node = sel?.focusNode;
      editable = this._closestEditable(node?.nodeType === Node.ELEMENT_NODE ? node : node?.parentElement);
    }
    if (!editable || editable.isConnected === false) return;
    this._onLiveInput({ inputType: "insertCompositionText", target: editable });
  }

  _pressAndHoldOwnsKey(event) {
    if (event.altKey || event.ctrlKey || event.metaKey) return false;
    const key = event.key;
    const menuKey = key === "ArrowLeft" || key === "ArrowRight" || key === "ArrowUp" || key === "ArrowDown" || key === "Enter" || key === "Escape";
    if (!menuKey || now() - (this._keyRepeatAt || 0) > 4000) return false;
    return this._domSelectionIsSingleGrapheme();
  }

  _domSelectionIsSingleGrapheme() {
    if (this._isSourceActive() && this._sourceTextarea) {
      const field = this._sourceTextarea;
      if (field.selectionStart === field.selectionEnd) return false;
      return [...field.value.slice(field.selectionStart, field.selectionEnd)].length === 1;
    }
    const sel = this._exposedLiveSelection();
    if (!sel || sel.isCollapsed || !sel.rangeCount) return false;
    let text = "";
    try { text = sel.getRangeAt(0).toString(); } catch { return false; }
    return [...text.replace(/\u00a0/g, " ")].length === 1;
  }

  _sourceFormDomDirty(event) {
    if (this._isSourceActive() && this._sourceTextarea) {
      return normalizeLineEndings(this._sourceTextarea.value) !== this._value;
    }
    const editable = this._closestEditable(event?.target) || this._activeEditableFromSelection();
    return this._editableTextDiffers(editable);
  }

  _editableTextDiffers(editable) {
    if (!editable || editable.dataset.sourceForm !== "true") return false;
    if (editable.dataset.kind === "code-fence" || editable.dataset.kind === "image") return false;
    if (editable.querySelector?.("img, video, span")) return false;
    const range = this._editableSourceRange(editable);
    if (!range) return false;
    let raw = this._plainText(editable);
    if (!this._keepsSourceNewlines(editable)) raw = raw.replace(/\n/g, "");
    const slice = this._value.slice(range.from, range.to);
    const expected = editable.dataset.editable === "cell" ? unescapeTableCellText(slice) : slice;
    return raw !== expected;
  }

  _onKeyDown(event) {
    if (event.repeat && event.key.length === 1 && !event.altKey && !event.ctrlKey && !event.metaKey) this._keyRepeatAt = now();
    const menuConfirmingEnter = event.key === "Enter" && !event.shiftKey && !event.altKey && !event.metaKey && !event.ctrlKey
      && (this._compositionEnding || now() < (this._suppressEnterUntil || 0));
    if (menuConfirmingEnter) {
      event.preventDefault();
      return;
    }
    if (this._isComposing || event.isComposing || event.keyCode === 229 || event.which === 229) return;
    if (!this._isSourceActive() && !event.altKey && !event.metaKey && !event.ctrlKey && (event.key.length === 1 || event.key === "Backspace" || event.key === "Delete")) {
      const sourceEditable = this._activeEditableFromEvent(event);
      if (this._keepsActiveSource(sourceEditable) && sourceEditable.dataset.sourceForm !== "true") {
        this._idleFormattedKey = "";
        this._syncSourceForm();
      }
    }
    if (this._pressAndHoldOwnsKey(event)) return;
    if (this._sourceFormDomDirty(event)) return;
    const isMac = /Mac|iPhone|iPad|iPod/.test(globalThis.navigator?.platform ?? "");
    const mod = isMac ? event.metaKey : event.ctrlKey;
    const taskCheckbox = event.target.closest?.("[data-task-checkbox]");
    if (!this._isSourceActive() && taskCheckbox && (event.key === " " || event.key === "Spacebar")) {
      event.preventDefault();
      taskCheckbox.click();
      return;
    }
    const activeEditable = this._activeEditableFromEvent(event);
    const activeCell = activeEditable?.dataset.editable === "cell" ? activeEditable : null;
    if (!this._isSourceActive() && NAVIGATION_KEYS.has(event.key)) this._ignoreSelectionChangeCount = 0;
    if (!this._isSourceActive() && this._value.length === 0 && (event.key === "Backspace" || event.key === "Delete")) {
      event.preventDefault();
      this._ensureEmptyLiveEditable();
      return;
    }

    if (mod && !event.altKey && event.key.toLowerCase() === "z") {
      if (this.readonly || this.disabled) return;
      event.preventDefault();
      event.shiftKey ? this._redo() : this._undo();
      return;
    }

    if (mod && !event.altKey && event.key.toLowerCase() === "a" && !this._isSourceActive()) {
      event.preventDefault();
      this._expandSelection();
      return;
    }

    if (this._isComposing) return;

    if (this._completion.open) {
      const map = { ArrowDown: "completion.next", ArrowUp: "completion.previous", Home: "completion.first", End: "completion.last", Escape: "completion.close" };
      if (map[event.key]) { event.preventDefault(); this._runAction(map[event.key], undefined, { source: "keyboard", apply: true }); return; }
      if (event.key === "PageDown") { event.preventDefault(); this._moveCompletion(5); return; }
      if (event.key === "PageUp") { event.preventDefault(); this._moveCompletion(-5); return; }
      if (event.key === "Enter" || (event.key === "Tab" && !event.shiftKey)) { event.preventDefault(); this._runAction("completion.accept", undefined, { source: "keyboard", apply: true }); return; }
    }

    if (this._maybeHandleMarkerPair(event)) return;

    if (this._maybeHandleTableLineBoundaryKey(event, activeCell)) return;
    if (this._maybeHandleLineBoundaryKey(event, activeCell, activeEditable)) return;
    if (activeCell && this._maybeHandleTableArrowKey(event, activeCell)) return;
    if (this._maybeHandleLiveArrowKey(event, activeEditable)) return;

    if ((event.key === "Backspace" || event.key === "Delete") && !this._isSourceActive()) {
      if (this.readonly || this.disabled) return;
      const selection = this._getCurrentSelection();
      if (selection.start !== selection.end) {
        event.preventDefault();
        const result = this._deleteSelectionResult(this._getContext(), event.key === "Delete" ? "editor.smartDelete" : "editor.smartBackspace");
        this._applyActionResult(event.key === "Delete" ? "editor.smartDelete" : "editor.smartBackspace", result, { source: "keyboard" });
        return;
      }
    }

    if (activeCell && (event.key === "Backspace" || event.key === "Delete")) {
      if (this.readonly || this.disabled) return;
      const result = this._deleteEmptyTableRowFromCellResult(activeCell);
      if (result?.ok && result.transaction) {
        event.preventDefault();
        this._applyActionResult("table.deleteRow", result, { source: "keyboard" });
        return;
      }
    }

    if (event.key === "Escape") {
      if (activeCell && !this._completion.open) {
        event.preventDefault();
        this._exitTable(activeCell, "after");
        return;
      }
      this._closeCompletion();
      return;
    }

    if (activeCell && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
      const direction = event.key === "ArrowDown" ? 1 : -1;
      if (this._maybeExitTableWithArrow(activeCell, direction)) {
        event.preventDefault();
        return;
      }
    }

    if (mod && !event.shiftKey && !event.altKey) {
      const key = event.key.toLowerCase();
      const map = { b: "inline.bold", i: "inline.italic", u: "inline.underline", k: "inline.link", e: "inline.code" };
      if (map[key]) { event.preventDefault(); this._runAction(map[key], undefined, { source: "keyboard", apply: true }); return; }
    }
    if (mod && event.shiftKey && !event.altKey && event.key.toLowerCase() === "x") { event.preventDefault(); this._runAction("inline.strikethrough", undefined, { source: "keyboard", apply: true }); return; }
    if (mod && event.altKey && /^[1-6]$/.test(event.key)) { event.preventDefault(); this._runAction(`block.heading.${event.key}`, undefined, { source: "keyboard", apply: true }); return; }

    if (event.key === " " && !event.shiftKey && !event.altKey && !mod) {
      if (this.readonly || this.disabled) return;
      const result = this._runAction("editor.markdownShortcut", undefined, { source: "keyboard", apply: false });
      if (result?.ok && result.transaction) { event.preventDefault(); this._applyActionResult("editor.markdownShortcut", result, { source: "keyboard" }); return; }
    }

    if (event.key === "Delete") {
      if (this.readonly || this.disabled) return;
      const result = this._runAction("editor.smartDelete", undefined, { source: "keyboard", apply: false });
      if (result?.ok && result.transaction) { event.preventDefault(); this._applyActionResult("editor.smartDelete", result, { source: "keyboard" }); return; }
    }

    if (event.key === "Enter") {
      if (this.readonly || this.disabled) return;
      event.preventDefault();
      if (activeCell) {
        if (event.shiftKey || mod || event.altKey) this._exitTable(activeCell, "after");
        else this._insertTableRowAfterCell(activeCell);
        return;
      }
      const activeEditableType = activeEditable?.dataset.editable;
      if (activeEditableType === "virtual-code-after" || activeEditableType === "virtual-table-after") {
        this._runAction("editor.insertParagraph", undefined, { source: "keyboard", apply: true });
        return;
      }
      this._runAction(event.shiftKey ? "editor.insertSoftBreak" : "editor.smartEnter", undefined, { source: "keyboard", apply: true });
      return;
    }

    if (event.key === "Tab") {
      if (this.readonly || this.disabled) return;
      if (!event.shiftKey && !event.altKey && !mod) {
        const selection = this._getCurrentSelection();
        const edit = markerPairEdit(this._value, selection.start, selection.end, "", "tab");
        if (edit) {
          event.preventDefault();
          this.setSelectionRange(edit.start, edit.end);
          return;
        }
      }
      if (activeCell) { event.preventDefault(); this._handleTableTab(activeCell, event.shiftKey ? -1 : 1); return; }
      const id = event.shiftKey ? "editor.smartOutdent" : "editor.smartTab";
      const result = this._runAction(id, undefined, { source: "keyboard", apply: false });
      if (result?.ok && (result.transaction || result.preventDefault)) { event.preventDefault(); this._applyActionResult(id, result, { source: "keyboard" }); }
      return;
    }

    if (event.key === "Backspace") {
      if (this.readonly || this.disabled) return;
      const result = this._runAction("editor.smartBackspace", undefined, { source: "keyboard", apply: false });
      if (result?.ok && result.transaction) { event.preventDefault(); this._applyActionResult("editor.smartBackspace", result, { source: "keyboard" }); }
    }
  }

  _activeEditableFromEvent(event) {
    return this._closestEditable(event?.target) || this._activeEditableFromSelection();
  }

  _activeEditableFromSelection() {
    const sel = this._exposedLiveSelection();
    if (sel) {
      const node = sel.focusNode || sel.anchorNode;
      const editable = this._closestEditable(node?.nodeType === Node.ELEMENT_NODE ? node : node?.parentElement);
      if (editable) return editable;
    }
    if (this._liveSelectionAPI !== false) return null;
    const focus = this._selection.direction === "backward"
      ? this._selection.start
      : this._selection.end;
    return this._domPositionFromSource(focus)?.editable || this._fallbackEditable;
  }

  _findBlockAtOffset(offset, type = null) {
    const safe = clamp(Number(offset) || 0, 0, this._value.length);
    return this._getBlocks().find(block => (!type || block.type === type) && safe >= block.from && safe <= Math.max(block.to, block.from)) || null;
  }

  _findTableBlockForCell(cell) {
    const tableEl = cell?.closest?.(".md-table-block");
    if (!tableEl) return null;
    const from = this._liveFrom(tableEl);
    const to = this._liveTo(tableEl);
    return this._getBlocks().find(block => block.type === "table" && block.from === from && block.to === to) || null;
  }

  // Dispatch a hover-control click to the matching table mutation primitive.
  // Adds append after the last column/row; deletes target the index on the
  // control. The primitive stamps its own actionId onto the transaction.
  _handleTableControlClick(control) {
    if (this.disabled || this.readonly) return;
    const block = this._findTableBlockForCell(control);
    if (!block) return;
    const ctx = this._getContext();
    const cols = Math.max(block.header.cells.length, ...block.rows.map(row => row.cells.length), 1);
    const index = Number(control.dataset.index);
    const result = {
      "add-col": () => this._tableColumnResult(ctx, block, cols - 1, "insert"),
      "del-col": () => this._tableColumnResult(ctx, block, index, "delete"),
      "add-row": () => this._tableRowInsertionResult(ctx, block, block.rows.at(-1) || block.delimiter, block.rows.length ? "after-row" : "after-delimiter"),
      "del-row": () => this._tableDeleteRowResult(ctx, block, index),
    }[control.dataset.tableControl]?.();
    if (!result) return;
    if (!result.ok) { if (result.message) this._announce(result.message); return; }
    this._applyActionResult(result.transaction.actionId, result, { source: "pointer" });
  }

  _tableInfoForCell(cell) {
    const block = this._findTableBlockForCell(cell);
    if (!block) return null;
    const row = Number(cell.dataset.row);
    const col = Number(cell.dataset.col);
    const line = row < 0 ? block.header : block.rows[row];
    const cols = Math.max(block.header.cells.length, ...block.rows.map(r => r.cells.length), 1);
    return { block, row, col, line, cols };
  }

  _moveTableCell(cell, delta) {
    const cells = [...cell.closest(".md-table-block")?.querySelectorAll('[data-editable="cell"]') || []];
    const i = cells.indexOf(cell);
    if (i === -1) return false;
    const next = cells[i + delta];
    if (!next) return false;
    const from = this._liveFrom(next);
    const text = this._plainText(next);
    this._selection = { start: from, end: from + text.length, direction: "none" };
    this._restoreLiveSelection(this._selection);
    this._announce("Table cell.");
    return true;
  }

  _handleTableTab(cell, delta) {
    if (this._moveTableCell(cell, delta)) return;
    if (delta < 0) {
      this._exitTable(cell, "before");
      return;
    }
    const info = this._tableInfoForCell(cell);
    if (!info) return;
    if (info.row >= 0 && this._isTableRowEmpty(info.line)) this._exitTable(cell, "after");
    else this._insertTableRowAfterCell(cell);
  }

  _maybeHandleTableLineBoundaryKey(event, activeCell = null) {
    if (this._isSourceActive() || event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey) return false;
    if (event.key !== "Home" && event.key !== "End") return false;
    const selectedEditable = activeCell || this._activeEditableFromSelection();
    const cell = selectedEditable?.dataset?.editable === "cell" ? selectedEditable : null;
    if (!cell) return false;
    const range = this._editableSourceRange(cell);
    if (!range) return false;
    const selection = this._getLiveSelection() || this._getCurrentSelection();
    const focus = selection.direction === "backward" ? selection.start : selection.end;
    const anchor = selection.direction === "backward" ? selection.end : selection.start;
    const target = event.key === "Home" ? range.from : range.to;
    if (target === focus && !event.shiftKey) {
      event.preventDefault();
      return true;
    }
    event.preventDefault();
    if (event.shiftKey) this._setSourceBackedSelection(anchor, target);
    else this.setSelectionRange(target, target, "none");
    return true;
  }

  _maybeHandleTableArrowKey(event, cell) {
    if (this._isSourceActive() || event.defaultPrevented || event.altKey || event.metaKey || event.ctrlKey) return false;
    if (!LIVE_ARROW_KEYS.has(event.key)) return false;
    const selection = this._getLiveSelection() || this._getCurrentSelection();
    if (!selection) return false;
    if (selection.start !== selection.end && !event.shiftKey) {
      const target = (event.key === "ArrowLeft" || event.key === "ArrowUp") ? selection.start : selection.end;
      event.preventDefault();
      this.setSelectionRange(target, target, "none");
      return true;
    }
    const focus = selection.direction === "backward" ? selection.start : selection.end;
    const anchor = selection.direction === "backward" ? selection.end : selection.start;
    const direction = (event.key === "ArrowLeft" || event.key === "ArrowUp") ? -1 : 1;
    const target = (event.key === "ArrowLeft" || event.key === "ArrowRight")
      ? this._horizontalTableArrowTarget(cell, focus, direction)
      : this._verticalTableArrowTarget(cell, focus, direction);
    if (!target) return false;
    event.preventDefault();
    if (target.exit) {
      if (event.shiftKey) this._setSourceBackedSelection(anchor, target.offset);
      else this._exitTable(cell, target.exit);
      return true;
    }
    if (event.shiftKey) this._setSourceBackedSelection(anchor, target.offset);
    else this.setSelectionRange(target.offset, target.offset, "none");
    return true;
  }

  _tableCellsForCell(cell) {
    return [...cell.closest(".md-table-block")?.querySelectorAll('[data-editable="cell"]') || []];
  }

  _tableCellElement(block, row, col) {
    const tables = this._liveEditor?.querySelectorAll(".md-table-block") || [];
    for (const table of tables) {
      if (this._liveFrom(table) !== block.from || this._liveTo(table) !== block.to) continue;
      return table.querySelector(`.md-cell[data-row="${row}"][data-col="${col}"]`);
    }
    return null;
  }

  _horizontalTableArrowTarget(cell, offset, direction) {
    const range = this._editableSourceRange(cell);
    if (!range) return null;
    if (direction < 0 && offset <= range.from) {
      const cells = this._tableCellsForCell(cell);
      const previous = cells[cells.indexOf(cell) - 1];
      if (previous) return { offset: this._liveTo(previous) };
      const info = this._tableInfoForCell(cell);
      const model = info && this._virtualState.active ? this._adjacentTableCell(info, -1) : null;
      if (model) return { offset: model.to };
      return info?.block ? { exit: "before", offset: info.block.from } : null;
    }
    if (direction > 0 && offset >= range.to) {
      const cells = this._tableCellsForCell(cell);
      const next = cells[cells.indexOf(cell) + 1];
      if (next) return { offset: this._liveFrom(next) };
      const info = this._tableInfoForCell(cell);
      const model = info && this._virtualState.active ? this._adjacentTableCell(info, 1) : null;
      if (model) return { offset: model.from };
      return info?.block ? { exit: "after", offset: info.block.to } : null;
    }
    return null;
  }

  _verticalTableArrowTarget(cell, offset, direction) {
    const info = this._tableInfoForCell(cell);
    if (!info) return null;
    const nextRow = info.row + direction;
    if (nextRow < -1) return { exit: "before", offset: info.block.from };
    if (nextRow >= info.block.rows.length) return { exit: "after", offset: info.block.to };
    const targetCell = this._tableCellElement(info.block, nextRow, info.col);
    const displayOffset = this._displayOffsetFromSourceOffset(cell, offset);
    if (!targetCell) {
      const line = nextRow < 0 ? info.block.header : info.block.rows[nextRow];
      const sourceCell = line?.cells?.[info.col];
      if (!sourceCell) return null;
      const raw = sourceCell.text ?? "";
      const within = Math.min(Math.max(0, displayOffset || 0), unescapeTableCellText(raw).length);
      return { offset: sourceCell.from + sourceIndexForUnescapedOffset(raw, within) };
    }
    const targetDisplayOffset = Math.min(displayOffset, this._plainText(targetCell).length);
    return { offset: this._sourceOffsetFromDisplayOffset(targetCell, targetDisplayOffset) ?? this._liveFrom(targetCell) };
  }
  _adjacentTableCell(info, direction) {
    const cols = info.cols;
    let row = info.row;
    let col = info.col + direction;
    if (col >= cols) { col = 0; row += 1; }
    if (col < 0) { col = cols - 1; row -= 1; }
    if (row < -1 || row >= info.block.rows.length) return null;
    const line = row < 0 ? info.block.header : info.block.rows[row];
    return line?.cells?.[col] || null;
  }

  _maybeExitTableWithArrow(cell, direction) {
    const info = this._tableInfoForCell(cell);
    if (!info) return false;
    const atStart = this._isCellSelectionAtBoundary(cell, "start");
    const atEnd = this._isCellSelectionAtBoundary(cell, "end");
    if (direction < 0 && info.row < 0 && atStart) { this._exitTable(cell, "before"); return true; }
    if (direction > 0 && info.row === info.block.rows.length - 1 && atEnd) { this._exitTable(cell, "after"); return true; }
    return false;
  }

  _isCellSelectionAtBoundary(cell, boundary) {
    const sel = this._getLiveSelection(cell);
    if (!sel || sel.start !== sel.end) return false;
    const from = this._liveFrom(cell);
    const to = this._liveTo(cell);
    return boundary === "start" ? sel.start <= from : sel.start >= to;
  }

  _isTableRowEmpty(line) {
    if (!line) return true;
    return splitTableRow(line.text).every(cell => cell.trim() === "");
  }

  _escapeTableCellText(cell) {
    return String(cell ?? "").replace(/\|/g, "\\|");
  }

  _tableRowSourceParts(cells) {
    const escaped = cells.map(cell => this._escapeTableCellText(cell));
    const offsets = [];
    let text = "| ";
    escaped.forEach((cell, index) => {
      offsets[index] = { from: text.length, to: text.length + cell.length };
      text += cell;
      text += index === escaped.length - 1 ? " |" : " | ";
    });
    return { text, offsets };
  }

  _tableBlockSourceWithOffsets(headerCells, delimiterCells, rows) {
    const parts = [
      this._tableRowSourceParts(headerCells),
      this._tableRowSourceParts(delimiterCells),
      ...rows.map(row => this._tableRowSourceParts(row)),
    ];
    const lines = [];
    const lineStarts = [];
    let cursor = 0;
    for (const part of parts) {
      lineStarts.push(cursor);
      lines.push(part.text);
      cursor += part.text.length + 1;
    }
    return { source: lines.join("\n"), parts, lineStarts };
  }

  _tableCellInputEdit(cell, raw, displayCursor = null) {
    const info = this._tableInfoForCell(cell);
    if (!info) return null;
    const cols = info.cols;
    const header = this._tableCellTexts(info.block.header, cols);
    const delimiter = this._tableCellTexts(info.block.delimiter, cols);
    const rows = info.block.rows.map(row => this._tableCellTexts(row, cols));
    if (info.row < 0) {
      header[info.col] = raw;
    } else {
      while (rows.length <= info.row) rows.push(Array.from({ length: cols }, () => ""));
      rows[info.row][info.col] = raw;
    }
    const serialized = this._tableBlockSourceWithOffsets(header, delimiter, rows);
    const lineIndex = info.row < 0 ? 0 : info.row + 2;
    const cellOffsets = serialized.parts[lineIndex]?.offsets?.[info.col];
    const rawCursor = clamp(displayCursor ?? raw.length, 0, raw.length);
    const escapedCursor = this._escapeTableCellText(raw.slice(0, rawCursor)).length;
    const cursor = info.block.from + (serialized.lineStarts[lineIndex] ?? 0) + (cellOffsets?.from ?? 0) + escapedCursor;
    return {
      nextValue: this._value.slice(0, info.block.from) + serialized.source + this._value.slice(info.block.to),
      cursor,
    };
  }

  _deleteEmptyTableRowFromCellResult(cell) {
    const info = this._tableInfoForCell(cell);
    if (!info || info.row < 0 || !info.line) return fail("not-applicable");
    const selection = this._getLiveSelection(cell);
    if (!selection || selection.start !== selection.end) return fail("not-applicable");
    if (this._plainText(cell).trim() || !this._isTableRowEmpty(info.line)) return fail("not-applicable");
    return this._tableDeleteRowResult(this._getContext(), info.block, info.row);
  }

  _insertTableRowAfterCell(cell) {
    const info = this._tableInfoForCell(cell);
    if (!info) return;
    const ctx = this._getContext();
    const result = this._tableRowInsertionResult(ctx, info.block, info.line, info.row < 0 ? "after-delimiter" : "after-row");
    this._applyActionResult("table.insertRowAfter", result, { source: "keyboard" });
  }

  _tableRowInsertionResult(ctx, block, line, placement = "after-row") {
    if (!block) return fail("not-applicable");
    const cols = Math.max(block.header.cells.length, ...block.rows.map(r => r.cells.length), 1);
    const insert = `\n| ${Array.from({ length: cols }, () => "").join(" | ")} |`;
    const insertionLine = placement === "after-delimiter" ? block.delimiter : (line || block.rows.at(-1) || block.delimiter);
    const from = insertionLine.end;
    const cursor = from + 3;
    return ok(tx(ctx, "table.insertRowAfter", [{ from, to: from, insert }], { start: cursor, end: cursor, direction: "none" }, "table"), "Table row inserted.");
  }

  _exitTable(cell, direction = "after") {
    const info = this._tableInfoForCell(cell);
    if (!info) return;
    const before = this._snapshot();
    const block = info.block;
    let changes = [];
    let cursor;
    if (direction === "before") {
      if (block.from === 0 || this._value[block.from - 1] !== "\n") {
        changes = [{ from: block.from, to: block.from, insert: "\n" }];
        cursor = block.from;
      } else {
        cursor = block.from;
      }
    } else {
      cursor = block.to < this._value.length ? block.to + 1 : block.to;
    }
    if (changes.length) {
      this._applyTransaction({ changes, selectionAfter: { start: cursor, end: cursor, direction: "none" }, actionId: "table.exit", undoGroup: "table", source: "keyboard" }, { source: "keyboard" });
    } else {
      this._selection = { start: cursor, end: cursor, direction: "none" };
      this._restoreLiveSelection(this._selection);
      this._emitSelectionChange();
    }
    const after = this._snapshot();
    this._dispatch("md-action", { actionId: "table.exit", source: "keyboard", before, after });
    this._announce(direction === "before" ? "Before table." : "After table.");
  }

  _expandSelection() {
    const current = this._getCurrentSelection();
    const candidates = this._selectionExpansionCandidates(current);
    const normalized = { start: Math.min(current.start, current.end), end: Math.max(current.start, current.end) };
    let next = candidates.find(range => (range.end > range.start || this._value.length === 0) && range.start <= normalized.start && range.end >= normalized.end && !this._sameRange(range, normalized));
    if (!next) next = { start: 0, end: this._value.length, label: "document" };
    this.setSelectionRange(next.start, next.end, "forward");
    this._announce(`Selected ${next.label || "content"}.`);
  }

  _selectionExpansionCandidates(selection) {
    const point = clamp(selection.start, 0, this._value.length);
    const out = [];
    const push = (start, end, label) => {
      const range = { start: clamp(start, 0, this._value.length), end: clamp(end, 0, this._value.length), label };
      if (range.end < range.start) [range.start, range.end] = [range.end, range.start];
      if (!out.some(existing => this._sameRange(existing, range))) out.push(range);
    };
    const active = this._activeEditableFromSelection();
    if (active?.dataset.editable === "cell") {
      const info = this._tableInfoForCell(active);
      const cellFrom = this._liveFrom(active);
      const cellTo = this._liveTo(active);
      push(cellFrom, cellTo, "cell");
      if (info?.line) push(info.line.start, info.line.end, "row");
      if (info?.block) push(info.block.from, info.block.to, "table");
    }
    const block = this._findBlockAtOffset(point) || this._getBlocks()[0];
    if (block) push(block.from, block.to, block.type === "table" ? "table" : "block");
    const section = this._sectionRangeForOffset(point);
    if (section) push(section.start, section.end, "section");
    push(0, this._value.length, "document");
    return out.sort((a, b) => (a.end - a.start) - (b.end - b.start));
  }

  _sectionRangeForOffset(offset) {
    const blocks = this._getBlocks();
    let headingIndex = -1;
    let headingLevel = Infinity;
    for (let i = 0; i < blocks.length; i += 1) {
      const block = blocks[i];
      if (block.type === "heading" && block.from <= offset) {
        headingIndex = i;
        headingLevel = block.heading.level;
      }
      if (block.from > offset) break;
    }
    if (headingIndex === -1) return null;
    let end = this._value.length;
    for (let i = headingIndex + 1; i < blocks.length; i += 1) {
      const block = blocks[i];
      if (block.type === "heading" && block.heading.level <= headingLevel) { end = block.from; break; }
    }
    return { start: blocks[headingIndex].from, end, label: "section" };
  }

  _sameRange(a, b) {
    return a && b && a.start === b.start && a.end === b.end;
  }

  _clipboardRangeFromSelection(selection = this._getCurrentSelection()) {
    if (!selection || selection.start === selection.end) return null;
    const start = Math.min(selection.start, selection.end);
    const end = Math.max(selection.start, selection.end);
    return expandMarkdownFormattingRange(this._value, start, end);
  }
  _writeMarkdownClipboard(event, markdown) {
    event.clipboardData?.setData("text/plain", markdown);
    event.clipboardData?.setData("text/markdown", markdown);
    event.clipboardData?.setData("text/x-markdown", markdown);
    event.clipboardData?.setData("text/html", renderMarkdown(markdown, this._rendererOptions()));
  }
  _onLiveCopy(event) {
    if (this._isSourceActive()) return;
    const range = this._clipboardRangeFromSelection();
    if (!range) return;
    const markdown = this._value.slice(range.start, range.end);
    event.preventDefault();
    this._writeMarkdownClipboard(event, markdown);
    this._dispatch("md-copy", { markdown, start: range.start, end: range.end });
  }
  _onLiveCut(event) {
    if (this._isSourceActive() || this.disabled || this.readonly) return;
    const range = this._clipboardRangeFromSelection();
    if (!range) return;
    const markdown = this._value.slice(range.start, range.end);
    event.preventDefault();
    this._writeMarkdownClipboard(event, markdown);
    const ctx = this._getContext();
    const result = ok(tx(ctx, "editor.deleteSelection", [{ from: range.start, to: range.end, insert: "" }], { start: range.start, end: range.start, direction: "none" }, "cut"), "Cut.");
    this._applyActionResult("editor.deleteSelection", result, { source: "keyboard" });
    this._dispatch("md-cut", { markdown, start: range.start, end: range.end });
  }

  _serializeTableBlock(headerCells, delimiterCells, rows) {
    const serialize = cells => this._tableRowSourceParts(cells).text;
    return [serialize(headerCells), serialize(delimiterCells), ...rows.map(serialize)].join("\n");
  }
  _tableCellTexts(line, cols) {
    const cells = splitTableRow(line?.text ?? "");
    return Array.from({ length: cols }, (_, i) => unescapeTableCellText(cells[i] ?? ""));
  }
  _tableColumnResult(ctx, block, col, mode) {
    const cols = Math.max(block.header.cells.length, ...block.rows.map(r => r.cells.length), 1);
    const target = clamp(Number(col) || 0, 0, cols - 1);
    const header = this._tableCellTexts(block.header, cols);
    const delimiter = this._tableCellTexts(block.delimiter, cols);
    const rows = block.rows.map(row => this._tableCellTexts(row, cols));
    if (mode === "delete") {
      if (cols <= 1) return fail("not-applicable", "Cannot delete the only column.");
      for (const list of [header, delimiter, ...rows]) list.splice(target, 1);
    } else {
      const at = target + 1;
      header.splice(at, 0, `Column ${cols + 1}`);
      delimiter.splice(at, 0, "---");
      for (const row of rows) row.splice(at, 0, "");
    }
    const insert = this._serializeTableBlock(header, delimiter, rows.length ? rows : [Array.from({ length: header.length }, () => "")]);
    const cursor = block.from + insert.split("\n")[0].length;
    return ok(tx(ctx, mode === "delete" ? "table.deleteColumn" : "table.insertColumnAfter", [{ from: block.from, to: block.to, insert }], { start: cursor, end: cursor, direction: "none" }, "table"), mode === "delete" ? "Column deleted." : "Column inserted.");
  }
  _tableDeleteRowResult(ctx, block, row) {
    if (!block.rows.length) return fail("not-applicable");
    const index = clamp(Number(row) || 0, 0, block.rows.length - 1);
    const line = block.rows[index];
    let from = line.start; let to = line.newlineEnd;
    if (to <= line.end && line.start > block.delimiter.end && ctx.value[line.start - 1] === "\n") {
      from = line.start - 1;
      to = line.end;
    } else if (to <= from) {
      to = line.end;
    }
    const cursor = from;
    return ok(tx(ctx, "table.deleteRow", [{ from, to, insert: "" }], { start: cursor, end: cursor, direction: "none" }, "table"), "Row deleted.");
  }

  _preparePastedMarkdown(markdown, kind = "text") {
    let insert = normalizeLineEndings(markdown ?? "");
    if (!insert) return insert;
    const sel = this._getCurrentSelection();
    if (sel.start !== sel.end) return insert;
    const shouldSeparate = kind === "markdown" || kind === "html" || kind === "table" || insert.includes("\n") || looksLikeBlockMarkdown(insert);
    if (!shouldSeparate) return insert;
    const line = getLineRange(this._value, sel.start);
    const beforeLine = this._value.slice(line.start, sel.start);
    const afterLine = this._value.slice(sel.start, line.end);
    if (beforeLine.trim() && !insert.startsWith("\n")) insert = "\n" + insert;
    if (afterLine.trim() && !insert.endsWith("\n")) insert = insert + "\n";
    return insert;
  }
  _insertPastedMarkdown(markdown, kind = "text") {
    const prepared = this._preparePastedMarkdown(markdown, kind);
    if (!prepared) return false;
    this._runAction("editor.insertText", { text: prepared }, { source: "paste", apply: true });
    this._dispatch("md-paste", { markdown: prepared, kind });
    return true;
  }
  _onPaste(event) {
    if (this.disabled || this.readonly) return;
    const clipboard = event.clipboardData || event.dataTransfer; if (!clipboard) return;
    const files = Array.from(clipboard.files || []);
    if (files.length > 0) { event.preventDefault(); const insertionPoint = this.selectionStart; this._dispatch("md-file-paste", { files, insertionPoint, insertMarkdown: markdown => this._insertPastedMarkdown(markdown, "file") }); return; }
    const text = safeClipboardGet(clipboard, "text/plain");
    if (text && this.selectionStart !== this.selectionEnd && isProbablyUrl(text) && !safeClipboardGet(clipboard, "text/markdown") && !safeClipboardGet(clipboard, "text/html")) { event.preventDefault(); this._runAction("inline.link", { url: text.trim() }, { source: "paste", apply: true }); return; }
    const { markdown, kind } = markdownFromClipboardData(clipboard);
    if (!markdown) return;
    event.preventDefault();
    this._insertPastedMarkdown(markdown, kind);
  }
  _onDrop(event) { if (this.disabled || this.readonly) return; const files = Array.from(event.dataTransfer?.files || []); if (!files.length) return; event.preventDefault(); const insertionPoint = this.selectionStart; this._dispatch("md-file-drop", { files, insertionPoint, insertMarkdown: markdown => this._insertPastedMarkdown(markdown, "file") }); }

  _getContext() {
    const sel = this._getCurrentSelection(); this._selection = sel;
    const parseOptions = this._parseOptions();
    const value = this._value; const line = getLineRange(value, sel.start); const currentLine = makeLineInfo(line.start, line.end, line.text, parseOptions); const selectedLines = getSelectedLineRanges(value, sel.start, sel.end, parseOptions); const block = classifyLine(value, sel.start, currentLine, parseOptions); const lineBeforeCursor = currentLine.text.slice(0, sel.start - currentLine.start);
    return { value, selectionStart: sel.start, selectionEnd: sel.end, selectionDirection: sel.direction || "none", mode: this.disabled ? "disabled" : this.readonly ? "readonly" : this._isComposing ? "composing-ime" : this._completion.open ? (this._completion.providerId === "slash" ? "slash-open" : "completion-open") : "idle", currentLine, selectedLines, block, inline: { insideInlineCode: isInsideInlineCode(lineBeforeCursor) }, completion: { ...this._completion }, config: { mode: this.mode, markdownFlavor: this.markdownFlavor, tabBehavior: this.tabBehavior, indentString: this.indentString, disabled: this.disabled, readonly: this.readonly }, host: this };
  }
  _runAction(actionId, args, options = {}) {
    const action = this._actions.get(actionId); if (!action) return fail("not-applicable", `Unknown action: ${actionId}`);
    const ctx = this._getContext(); if (ctx.mode === "disabled" && !action.viewSafe) return fail("disabled"); if (ctx.mode === "readonly" && !action.readonlySafe && !action.viewSafe) return fail("readonly"); if (ctx.mode === "composing-ime" && action.structural !== false) return fail("composition-active"); if (action.when && !action.when(ctx, args)) return fail("not-applicable");
    try { const result = action.run(ctx, args); if (options.apply === false) return result; return this._applyActionResult(actionId, result, options); } catch (error) { this._emitError("action", error, true, { actionId }); return fail("provider-error", String(error?.message || error)); }
  }
  _applyActionResult(actionId, result, options = {}) {
    if (!result?.ok) return result; const before = this._snapshot();
    if (result.transaction) { const t = { ...result.transaction, source: options.source || result.transaction.source || "api", actionId, timestamp: now() }; this._applyTransaction(t, { source: t.source }); const after = this._snapshot(); this._dispatch("md-action", { actionId, args: t.args, source: t.source, before, after }); }
    else this._dispatch("md-action", { actionId, source: options.source || "api", before, after: this._snapshot() });
    if (result.announcement) this._announce(result.announcement); return result;
  }
  _applyTransaction(transaction, options = {}) {
    const before = this._snapshot();
    const nextValue = applyTextChanges(this._value, transaction.changes);
    const proposedSelection = transaction.selectionAfter || { start: nextValue.length, end: nextValue.length, direction: "none" };
    const beforeEvent = this._dispatch("md-before-change", { transaction, before, nextValue, selectionAfter: proposedSelection, source: options.source || transaction.source || "api" }, { cancelable: true });
    if (beforeEvent.defaultPrevented) { this._announce("Change blocked."); return false; }
    this._value = nextValue;
    const sel = proposedSelection;
    this._selection = { start: clamp(sel.start, 0, nextValue.length), end: clamp(sel.end, 0, nextValue.length), direction: sel.direction || "none" }; this._structuredSelection = null;
    const after = makeSnapshot(this._value, this._selection.start, this._selection.end, this._selection.direction); this._recordUndo(before, after, transaction.undoGroup || transaction.actionId, { coalesce: false }); this._redoStack.length = 0; this._afterValueChanged({ source: options.source || transaction.source || "api", restoreSelection: true, previousValue: before.value, changes: transaction.changes }); this._scheduleCompletionUpdate();
    return true;
  }

  _installBuiltInActions() {
    const r = a => this.registerAction(a);
    r({ id: "editor.insertText", label: "Insert text", group: "Editor", structural: false, run: (ctx, args = {}) => insertionTransaction(ctx, "editor.insertText", normalizeLineEndings(args.text ?? ""), normalizeLineEndings(args.text ?? "").length, "insertText") });
    r({ id: "editor.replaceSelection", label: "Replace selection", group: "Editor", structural: false, run: (ctx, args = {}) => insertionTransaction(ctx, "editor.replaceSelection", normalizeLineEndings(args.text ?? ""), normalizeLineEndings(args.text ?? "").length, "replaceSelection") });
    r({ id: "editor.insertParagraph", label: "Insert paragraph", group: "Editor", defaultShortcut: "Enter", run: ctx => insertionTransaction(ctx, "editor.insertParagraph", "\n", 1, "insertParagraph") });
    r({ id: "editor.insertSoftBreak", label: "Insert soft break", group: "Editor", defaultShortcut: "Shift+Enter", run: ctx => insertionTransaction(ctx, "editor.insertSoftBreak", "  \n", 3, "insertSoftBreak") });
    r({ id: "editor.smartEnter", label: "Smart enter", group: "Editor", defaultShortcut: "Enter", run: ctx => this._smartEnter(ctx) });
    r({ id: "editor.smartTab", label: "Indent", group: "Editor", defaultShortcut: "Tab", run: ctx => this._smartTab(ctx) });
    r({ id: "editor.smartOutdent", label: "Outdent", group: "Editor", defaultShortcut: "Shift+Tab", run: ctx => this._smartOutdent(ctx) });
    r({ id: "editor.smartBackspace", label: "Smart backspace", group: "Editor", defaultShortcut: "Backspace", run: ctx => this._smartBackspace(ctx) });
    r({ id: "editor.smartDelete", label: "Smart delete", group: "Editor", defaultShortcut: "Delete", run: ctx => this._smartDelete(ctx) });
    r({ id: "editor.markdownShortcut", label: "Markdown shortcut", group: "Editor", defaultShortcut: "Space", run: ctx => this._markdownShortcut(ctx) });
    r({ id: "editor.deleteSelection", label: "Delete selection", group: "Editor", run: ctx => this._deleteSelectionResult(ctx, "editor.deleteSelection") });
    r({ id: "editor.selectAllExpand", label: "Expand selection", group: "Editor", defaultShortcut: "Mod+A", viewSafe: true, readonlySafe: true, run: () => { this._expandSelection(); return okNoop("Selection expanded."); } });
    r({ id: "history.undo", label: "Undo", group: "History", defaultShortcut: "Mod+Z", run: () => this._undo() ? okNoop("Undo.") : fail("not-applicable") });
    r({ id: "history.redo", label: "Redo", group: "History", defaultShortcut: "Mod+Shift+Z", run: () => this._redo() ? okNoop("Redo.") : fail("not-applicable") });
    // Slash-menu actions, ordered to match nonogra.ph's menu priority:
    // headings, inline formatting, link/image, block containers, secret, divider.
    for (let level = 1; level <= 4; level += 1) r({ id: `block.heading.${level}`, label: `Heading ${level}`, group: "Blocks", aliases: [`h${level}`, `heading${level}`], keywords: ["title", "section"], defaultShortcut: level <= 3 ? `Mod+Alt+${level}` : undefined, visibleInSlash: true, run: ctx => this._toggleHeading(ctx, level) });
    r({ id: "inline.bold", label: "Bold", group: "Inline", aliases: ["bold", "strong"], defaultShortcut: "Mod+B", visibleInSlash: true, run: ctx => this._wrapInline(ctx, "**", "**", "Bold") });
    r({ id: "inline.italic", label: "Italic", group: "Inline", aliases: ["italic", "em"], defaultShortcut: "Mod+I", visibleInSlash: true, run: ctx => this._wrapInline(ctx, "*", "*", "Italic") });
    r({ id: "inline.underline", label: "Underline", group: "Inline", aliases: ["underline", "u"], defaultShortcut: "Mod+U", visibleInSlash: true, run: ctx => this._wrapInline(ctx, "_", "_", "Underline") });
    r({ id: "inline.strikethrough", label: "Strikethrough", group: "Inline", aliases: ["strike", "s"], defaultShortcut: "Mod+Shift+X", visibleInSlash: true, run: ctx => this._wrapInline(ctx, "~", "~", "Strikethrough") });
    r({ id: "inline.superscript", label: "Superscript", group: "Inline", aliases: ["superscript", "sup"], visibleInSlash: true, run: ctx => this._wrapInline(ctx, "^", "^", "Superscript") });
    r({ id: "inline.highlight", label: "Highlight", group: "Inline", aliases: ["highlight", "mark"], defaultShortcut: "Mod+Shift+H", visibleInSlash: true, run: ctx => this._wrapInline(ctx, "==", "==", "Highlight") });
    r({ id: "inline.code", label: "Inline code", group: "Inline", aliases: ["inline-code", "codespan"], defaultShortcut: "Mod+E", visibleInSlash: true, run: ctx => this._wrapInline(ctx, "`", "`", "Inline code") });
    r({ id: "block.codeFence", label: "Code block", group: "Insert", aliases: ["code", "pre", "fence"], visibleInSlash: true, run: (ctx, args) => this._toggleCodeFence(ctx, args) });
    r({ id: "inline.link", label: "Link", group: "Inline", aliases: ["link", "url"], defaultShortcut: "Mod+K", visibleInSlash: true, run: (ctx, args = {}) => this._insertLink(ctx, args) });
    r({ id: "inline.image", label: "Image", group: "Inline", aliases: ["image", "img", "picture"], visibleInSlash: true, run: (ctx, args = {}) => this._insertImage(ctx, args) });
    r({ id: "block.blockquote", label: "Blockquote", group: "Blocks", aliases: ["quote", "blockquote"], visibleInSlash: true, run: ctx => this._toggleBlockquote(ctx) });
    r({ id: "block.bulletList", label: "Bullet list", group: "Blocks", aliases: ["bullet", "ul", "list"], visibleInSlash: true, run: ctx => this._toggleList(ctx, "bullet") });
    r({ id: "block.orderedList", label: "Numbered list", group: "Blocks", aliases: ["number", "numbered", "ol"], visibleInSlash: true, run: ctx => this._toggleList(ctx, "ordered") });
    r({ id: "block.taskList", label: "Task list", group: "Blocks", aliases: ["todo", "task", "checkbox"], visibleInSlash: true, run: ctx => this._toggleList(ctx, "task") });
    r({ id: "block.table", label: "Table", group: "Insert", aliases: ["table", "grid"], visibleInSlash: true, run: (ctx, args = {}) => this._insertTable(ctx, args) });
    r({ id: "inline.secret", label: "Hidden text", group: "Inline", aliases: ["secret", "spoiler", "hidden"], visibleInSlash: true, run: ctx => this._wrapInline(ctx, "#", "#", "Hidden text") });
    r({ id: "block.horizontalRule", label: "Divider", group: "Insert", aliases: ["hr", "divider", "rule"], visibleInSlash: true, run: ctx => this._insertHorizontalRule(ctx) });
    // Available as actions/shortcuts but not surfaced in the slash menu (not part of nonograph's set).
    r({ id: "block.paragraph", label: "Paragraph", group: "Blocks", aliases: ["p", "text", "clear"], run: ctx => this._toggleParagraph(ctx) });
    r({ id: "block.taskDone", label: "Toggle task done", group: "Blocks", aliases: ["done", "check"], run: ctx => this._toggleTaskDone(ctx) });    r({ id: "view.live", label: "Live mode", group: "View", viewSafe: true, readonlySafe: true, run: () => { this.mode = "live"; return okNoop("Live mode."); } });
    r({ id: "view.source", label: "Source mode", group: "View", viewSafe: true, readonlySafe: true, run: () => { this.mode = "source"; return okNoop("Source mode."); } });
    r({ id: "completion.close", label: "Close completion", group: "Completion", viewSafe: true, run: () => { this._closeCompletion(); return okNoop("Completion closed."); } });
    r({ id: "completion.next", label: "Next completion", group: "Completion", viewSafe: true, run: () => { this._moveCompletion(1); return okNoop(); } });
    r({ id: "completion.previous", label: "Previous completion", group: "Completion", viewSafe: true, run: () => { this._moveCompletion(-1); return okNoop(); } });
    r({ id: "completion.first", label: "First completion", group: "Completion", viewSafe: true, run: () => { this._setCompletionIndex(0, 1); return okNoop(); } });
    r({ id: "completion.last", label: "Last completion", group: "Completion", viewSafe: true, run: () => { this._setCompletionIndex(this._completion.items.length - 1, -1); return okNoop(); } });
    r({ id: "completion.accept", label: "Accept completion", group: "Completion", viewSafe: true, run: () => this._acceptCompletion("action") });
  }

  _installBuiltInProviders() {
    this.registerCompletionProvider({ id: "slash", priority: 100, triggers: ["/"], match: ctx => this._matchSlash(ctx), getItems: match => this._getSlashItems(match), apply: (item, match, ctx) => this._applySlashItem(item, match, ctx) });
    this.registerCompletionProvider({ id: "code-language", priority: 60, triggers: ["```", "~~~"], match: ctx => this._matchCodeLanguage(ctx), getItems: match => this._getLanguageItems(match), apply: (item, match, ctx) => this._applyCodeLanguage(item, match, ctx) });
  }
  _getLanguageItems(match) {
    const q = match.query.toLowerCase(); const alias = ALIASES.get(q);
    return LANGUAGES.map(lang => ({ lang, score: !q ? 0 : lang === q || lang === alias ? -100 : lang.startsWith(q) ? -50 : lang.includes(q) ? -10 : 0 })).filter(x => !q || x.score < 0).sort((a, b) => a.score - b.score || a.lang.localeCompare(b.lang)).slice(0, 16).map(x => ({ id: x.lang, label: x.lang, detail: "code language", kind: "code-language" }));
  }

  _smartEnter(ctx) {
    if (ctx.selectionStart !== ctx.selectionEnd) return insertionTransaction(ctx, "editor.smartEnter", "\n", 1, "smartEnter");
    const currentTextBeforeCursor = ctx.currentLine.text.slice(0, ctx.selectionStart - ctx.currentLine.start);
    const fenceInfo = getFenceInfo(ctx.currentLine.text);
    if (fenceInfo && currentTextBeforeCursor.trim().startsWith(fenceInfo.sequence) && !isInsideFence(ctx.value, ctx.selectionStart) && !hasClosingFenceAfter(ctx.value, ctx.currentLine.end, fenceInfo)) {
      return insertionTransaction(ctx, "editor.smartEnter", `\n\n${fenceInfo.sequence}`, 1, "smartEnter");
    }
    if (ctx.block.kind === "fenced-code") return insertionTransaction(ctx, "editor.smartEnter", "\n", 1, "smartEnter");
    const list = ctx.block.list;
    if (list) {
      if (list.content.trim() === "") return removePrefixFromLine(ctx, "editor.smartEnter", list.contentStart, "Exited list.");
      if (list.kind === "task-list-item") { const insert = `\n${list.indent}${list.marker} [ ] `; return insertionTransaction(ctx, "editor.smartEnter", insert, insert.length, "smartEnter"); }
      if (list.kind === "ordered-list-item") { const next = Number.isFinite(list.number) ? list.number + 1 : 1; const insert = `\n${list.indent}${next}${list.delimiter || "."} `; return insertionTransaction(ctx, "editor.smartEnter", insert, insert.length, "smartEnter"); }
      const insert = `\n${list.indent}${list.marker} `; return insertionTransaction(ctx, "editor.smartEnter", insert, insert.length, "smartEnter");
    }
    const quote = ctx.block.blockquote;
    if (quote) { if (quote.content.trim() === "") return removePrefixFromLine(ctx, "editor.smartEnter", quote.contentStart, "Exited blockquote."); const insert = `\n${quote.markerText}`; return insertionTransaction(ctx, "editor.smartEnter", insert, insert.length, "smartEnter"); }
    if (ctx.block.kind === "heading") return insertionTransaction(ctx, "editor.smartEnter", "\n", 1, "smartEnter");
    if (ctx.block.kind === "table") {
      const table = this._findBlockAtOffset(ctx.selectionStart, "table");
      if (table) {
        const placement = ctx.currentLine.start === table.header.start ? "after-delimiter" : "after-row";
        return this._tableRowInsertionResult(ctx, table, ctx.currentLine, placement);
      }
    }
    return insertionTransaction(ctx, "editor.smartEnter", "\n", 1, "smartEnter");
  }
  _smartTab(ctx) {
    if (ctx.completion?.open) return this._acceptCompletion("tab");
    const anyList = ctx.selectedLines.some(line => parseListItem(line.text, ctx.config)); if (anyList) return this._indentLines(ctx, ctx.config.indentString);
    if (ctx.block.kind === "fenced-code" || ctx.config.tabBehavior === "editor-first") return insertionTransaction(ctx, "editor.smartTab", ctx.config.indentString, ctx.config.indentString.length, "indent");
    return fail("not-applicable", "Tab should move focus in accessibility-first mode.");
  }
  _smartOutdent(ctx) { const any = ctx.selectedLines.some(line => this._lineOutdentAmount(line.text) > 0); if (any) return this._outdentLines(ctx); return fail("not-applicable"); }
  _deleteSelectionResult(ctx, actionId = "editor.deleteSelection") {
    const start = Math.min(ctx.selectionStart, ctx.selectionEnd);
    const end = Math.max(ctx.selectionStart, ctx.selectionEnd);
    if (start === end) return fail("not-applicable");
    return ok(tx(ctx, actionId, [{ from: start, to: end, insert: "" }], { start, end: start, direction: "none" }, "delete"), "Deleted selection.");
  }
  _markdownShortcut(ctx) {
    if (ctx.selectionStart !== ctx.selectionEnd || ctx.inline.insideInlineCode || ctx.block.kind === "fenced-code") return fail("not-applicable");
    const before = ctx.currentLine.text.slice(0, ctx.selectionStart - ctx.currentLine.start);
    const after = ctx.currentLine.text.slice(ctx.selectionStart - ctx.currentLine.start);
    if (after.trim()) return fail("not-applicable");
    const task = /^(\s*)(\[\]|\[ \]|\[x\]|\[X\])$/.exec(before);
    if (task) {
      const checked = /x/i.test(task[2]) ? "x" : " ";
      const insert = `${task[1]}- [${checked}] `;
      const cursor = ctx.currentLine.start + insert.length;
      return ok(tx(ctx, "editor.markdownShortcut", [{ from: ctx.currentLine.start, to: ctx.selectionStart, insert }], { start: cursor, end: cursor, direction: "none" }, "markdownShortcut"), "Task list.");
    }
    const heading = /^(\s*)(#{1,4})$/.exec(before);
    if (heading) return insertionTransaction(ctx, "editor.markdownShortcut", " ", 1, "markdownShortcut");
    const bullet = /^(\s*)[-+*]$/.exec(before);
    if (bullet) return insertionTransaction(ctx, "editor.markdownShortcut", " ", 1, "markdownShortcut");
    const ordered = /^(\s*)\d+[.)]$/.exec(before);
    if (ordered) return insertionTransaction(ctx, "editor.markdownShortcut", " ", 1, "markdownShortcut");
    const quote = /^(\s*)>$/.exec(before);
    if (quote) return insertionTransaction(ctx, "editor.markdownShortcut", " ", 1, "markdownShortcut");
    return fail("not-applicable");
  }
  _smartDelete(ctx) {
    if (ctx.selectionStart !== ctx.selectionEnd) return this._deleteSelectionResult(ctx, "editor.smartDelete");
    const lineOffset = ctx.selectionStart - ctx.currentLine.start;
    if (lineOffset === ctx.currentLine.text.length && ctx.currentLine.end < ctx.value.length && ctx.value[ctx.currentLine.end] === "\n") {
      return ok(tx(ctx, "editor.smartDelete", [{ from: ctx.currentLine.end, to: ctx.currentLine.end + 1, insert: "" }], { start: ctx.currentLine.end, end: ctx.currentLine.end, direction: "none" }, "smartDelete"), "Joined line.");
    }
    return fail("not-applicable");
  }
  _smartBackspace(ctx) {
    if (ctx.selectionStart !== ctx.selectionEnd) return this._deleteSelectionResult(ctx, "editor.smartBackspace"); const lineOffset = ctx.selectionStart - ctx.currentLine.start; const list = ctx.block.list;
    if (list) { if (list.content.trim() === "" && lineOffset >= list.contentStart) return removePrefixFromLine(ctx, "editor.smartBackspace", list.contentStart, "Exited list."); if (lineOffset === list.contentStart) { const from = ctx.currentLine.start + list.fullMarkerStart; const to = ctx.currentLine.start + list.fullMarkerEnd; return ok(tx(ctx, "editor.smartBackspace", [{ from, to, insert: "" }], { start: from, end: from, direction: "none" }, "smartBackspace"), "Removed list marker."); } }
    const heading = ctx.block.heading; if (heading && lineOffset === heading.contentStart) return removePrefixFromLine(ctx, "editor.smartBackspace", heading.contentStart, "Converted to paragraph.");
    const quote = ctx.block.blockquote; if (quote && lineOffset === quote.contentStart) return removePrefixFromLine(ctx, "editor.smartBackspace", quote.contentStart, "Exited blockquote.");
    if (lineOffset === 0 && ctx.currentLine.start > 0 && ctx.value[ctx.currentLine.start - 1] === "\n") { const joinAt = ctx.currentLine.start - 1; return ok(tx(ctx, "editor.smartBackspace", [{ from: joinAt, to: ctx.currentLine.start, insert: "" }], { start: joinAt, end: joinAt, direction: "none" }, "smartBackspace"), "Joined line."); }
    if (lineOffset > 0 && /^\s+$/.test(ctx.currentLine.text.slice(0, lineOffset))) { const amount = this._lineOutdentAmount(ctx.currentLine.text.slice(0, lineOffset)); if (amount > 0) { const from = ctx.selectionStart - amount; return ok(tx(ctx, "editor.smartBackspace", [{ from, to: ctx.selectionStart, insert: "" }], { start: from, end: from, direction: "none" }, "smartBackspace")); } }
    return fail("not-applicable");
  }
  _lineOutdentAmount(text) { if (text.startsWith("\t")) return 1; const indent = (text.match(/^ +/) || [""])[0].length; if (indent >= this.indentString.length && this.indentString !== "\t") return this.indentString.length; if (indent >= 4) return 4; if (indent >= 2) return 2; if (indent >= 1) return 1; return 0; }
  _indentLines(ctx, indent) { const changes = []; let ds = 0; let de = 0; for (const line of ctx.selectedLines) { if (!parseListItem(line.text, ctx.config) && ctx.block.kind !== "fenced-code") continue; changes.push({ from: line.start, to: line.start, insert: indent }); if (line.start < ctx.selectionStart) ds += indent.length; if (line.start < ctx.selectionEnd || ctx.selectionStart === ctx.selectionEnd) de += indent.length; } if (!changes.length) return fail("not-applicable"); return ok(tx(ctx, "editor.smartTab", changes, { start: ctx.selectionStart + ds, end: ctx.selectionEnd + de, direction: ctx.selectionDirection || "none" }, "indent"), "Indented."); }
  _outdentLines(ctx) { const changes = []; let ds = 0; let de = 0; for (const line of ctx.selectedLines) { const amount = this._lineOutdentAmount(line.text); if (amount <= 0) continue; changes.push({ from: line.start, to: line.start + amount, insert: "" }); if (line.start < ctx.selectionStart) ds -= amount; if (line.start < ctx.selectionEnd || ctx.selectionStart === ctx.selectionEnd) de -= amount; } if (!changes.length) return fail("not-applicable"); const base = ctx.selectedLines[0]?.start ?? 0; return ok(tx(ctx, "editor.smartOutdent", changes, { start: Math.max(base, ctx.selectionStart + ds), end: Math.max(base, ctx.selectionEnd + de), direction: ctx.selectionDirection || "none" }, "outdent"), "Outdented."); }

  _toggleParagraph(ctx) { const changes = []; for (const line of ctx.selectedLines) { const list = parseListItem(line.text, ctx.config); const heading = parseHeading(line.text); const quote = parseBlockquote(line.text); if (list) changes.push({ from: line.start + list.fullMarkerStart, to: line.start + list.fullMarkerEnd, insert: "" }); else if (heading) changes.push({ from: line.start, to: line.start + heading.contentStart, insert: heading.indent }); else if (quote) changes.push({ from: line.start, to: line.start + quote.contentStart, insert: "" }); } if (!changes.length) return fail("not-applicable"); const d = changes.reduce((sum, c) => c.from < ctx.selectionStart ? sum + c.insert.length - (c.to - c.from) : sum, 0); return ok(tx(ctx, "block.paragraph", changes, { start: Math.max(0, ctx.selectionStart + d), end: Math.max(0, ctx.selectionEnd + d), direction: ctx.selectionDirection || "none" }, "block"), "Converted to paragraph."); }
  _toggleHeading(ctx, level) { const marker = `${"#".repeat(level)} `; const lines = ctx.selectedLines; const allSame = lines.every(line => { const h = parseHeading(line.text); return h && h.level === level; }); const changes = []; for (const line of lines) { const h = parseHeading(line.text); const list = parseListItem(line.text, ctx.config); const quote = parseBlockquote(line.text); if (allSame && h) changes.push({ from: line.start, to: line.start + h.contentStart, insert: h.indent }); else if (h) changes.push({ from: line.start, to: line.start + h.contentStart, insert: h.indent + marker }); else if (list) changes.push({ from: line.start + list.fullMarkerStart, to: line.start + list.fullMarkerEnd, insert: marker }); else if (quote) changes.push({ from: line.start, to: line.start + quote.contentStart, insert: marker }); else changes.push({ from: line.start, to: line.start, insert: marker }); } let ds = 0; let de = 0; for (const c of changes) { const diff = c.insert.length - (c.to - c.from); if (c.from < ctx.selectionStart) ds += diff; if (c.from < ctx.selectionEnd || ctx.selectionStart === ctx.selectionEnd) de += diff; } return ok(tx(ctx, `block.heading.${level}`, changes, { start: Math.max(0, ctx.selectionStart + ds), end: Math.max(0, ctx.selectionEnd + de), direction: ctx.selectionDirection || "none" }, "block"), allSame ? "Converted to paragraph." : `Heading level ${level}.`); }
  _toggleList(ctx, type) { const markerFor = i => type === "ordered" ? `${i + 1}. ` : type === "task" ? "- [ ] " : "- "; const lines = ctx.selectedLines; const allList = lines.every(line => parseListItem(line.text, ctx.config)); const changes = []; lines.forEach((line, i) => { const list = parseListItem(line.text, ctx.config); const h = parseHeading(line.text); const quote = parseBlockquote(line.text); let c; if (allList && list) c = { from: line.start + list.fullMarkerStart, to: line.start + list.fullMarkerEnd, insert: "" }; else if (list) c = { from: line.start + list.fullMarkerStart, to: line.start + list.fullMarkerEnd, insert: markerFor(i) }; else if (h) c = { from: line.start, to: line.start + h.contentStart, insert: h.indent + markerFor(i) }; else if (quote) c = { from: line.start, to: line.start + quote.contentStart, insert: markerFor(i) }; else { const indent = (line.text.match(/^\s*/) || [""])[0]; c = { from: line.start + indent.length, to: line.start + indent.length, insert: markerFor(i) }; } changes.push(c); }); let ds = 0; let de = 0; for (const c of changes) { const diff = c.insert.length - (c.to - c.from); if (c.from < ctx.selectionStart) ds += diff; if (c.from < ctx.selectionEnd || ctx.selectionStart === ctx.selectionEnd) de += diff; } const id = `block.${type === "bullet" ? "bulletList" : type === "ordered" ? "orderedList" : "taskList"}`; return ok(tx(ctx, id, changes, { start: Math.max(0, ctx.selectionStart + ds), end: Math.max(0, ctx.selectionEnd + de), direction: ctx.selectionDirection || "none" }, "block"), allList ? "Removed list." : type === "ordered" ? "Numbered list." : type === "task" ? "Task list." : "Bullet list."); }
  _toggleTaskDone(ctx) { const list = ctx.block.list; if (!list || list.kind !== "task-list-item") return fail("not-applicable"); const checkboxStart = ctx.currentLine.start + list.indent.length + `${list.marker} [`.length; const next = list.checked ? " " : "x"; return ok(tx(ctx, "block.taskDone", [{ from: checkboxStart, to: checkboxStart + 1, insert: next }], { start: ctx.selectionStart, end: ctx.selectionEnd, direction: ctx.selectionDirection || "none" }, "block"), next === "x" ? "Task checked." : "Task unchecked."); }
  _toggleBlockquote(ctx) { const lines = ctx.selectedLines; const allQuote = lines.every(line => parseBlockquote(line.text)); const changes = lines.map(line => { const quote = parseBlockquote(line.text); return allQuote && quote ? { from: line.start, to: line.start + quote.contentStart, insert: "" } : { from: line.start, to: line.start, insert: "> " }; }); let ds = 0; let de = 0; for (const c of changes) { const diff = c.insert.length - (c.to - c.from); if (c.from < ctx.selectionStart) ds += diff; if (c.from < ctx.selectionEnd || ctx.selectionStart === ctx.selectionEnd) de += diff; } return ok(tx(ctx, "block.blockquote", changes, { start: Math.max(0, ctx.selectionStart + ds), end: Math.max(0, ctx.selectionEnd + de), direction: ctx.selectionDirection || "none" }, "block"), allQuote ? "Removed blockquote." : "Blockquote."); }
  _setCodeLanguageResult(ctx, block, language) {
    const opening = block?.opening;
    if (!opening) return fail("not-applicable");
    const clean = String(language ?? "").replace(/[`~\\\r\n]/g, "").trim().replace(/\s+/g, "-");
    const parsed = /^(\s*)(`{3,}|~{3,})([ \t]*)(\S*)(.*)$/.exec(opening.text);
    if (!parsed) return fail("not-applicable");
    const gap = clean ? parsed[3] : "";
    const rest = clean ? (parsed[5] || "") : "";
    const insert = `${parsed[1]}${parsed[2]}${gap}${clean}${rest}`;
    const cursor = opening.start + parsed[1].length + parsed[2].length + gap.length + clean.length;
    return ok(tx(ctx, "code.setLanguage", [{ from: opening.start, to: opening.end, insert }], { start: cursor, end: cursor, direction: "none" }, "code"), clean ? `Language ${clean}.` : "Language cleared.");
  }
  _applyCodeLanguage(item, match, ctx) {
    const fence = this._findBlockAtOffset(match.from, "code-fence");
    if (fence?.opening && match.from >= fence.opening.start && match.to <= fence.opening.end) return this._setCodeLanguageResult(ctx, fence, item.label);
    const insert = `${match.sequence || "```"}${item.label}`;
    const cursor = match.from + insert.length;
    return ok(tx(ctx, "completion.accept", [{ from: match.from, to: match.to, insert }], { start: cursor, end: cursor, direction: "none" }, "completion"), `Language ${item.label}.`);
  }
  _toggleCodeFence(ctx, args = {}) { const language = String(args.language ?? "").trim(); const langPart = language ? language : ""; if (ctx.selectionStart !== ctx.selectionEnd) { const selected = ctx.value.slice(ctx.selectionStart, ctx.selectionEnd); const insert = `\`\`\`${langPart}\n${selected}\n\`\`\``; const cursor = ctx.selectionStart + 4 + langPart.length + selected.length; return ok(tx(ctx, "block.codeFence", [{ from: ctx.selectionStart, to: ctx.selectionEnd, insert }], { start: cursor, end: cursor, direction: "none" }, "block"), "Code block."); } const insert = `\`\`\`${langPart}\n\n\`\`\``; const cursor = ctx.selectionStart + 4 + langPart.length; return ok(tx(ctx, "block.codeFence", [{ from: ctx.selectionStart, to: ctx.selectionEnd, insert }], { start: cursor, end: cursor, direction: "none" }, "block"), "Code block."); }
  _insertHorizontalRule(ctx) { const lead = ctx.selectionStart > 0 && ctx.value[ctx.selectionStart - 1] !== "\n" ? "\n" : ""; const trail = ctx.selectionStart < ctx.value.length && ctx.value[ctx.selectionStart] !== "\n" ? "\n" : "\n"; const insert = `${lead}---${trail}`; return insertionTransaction(ctx, "block.horizontalRule", insert, insert.length, "block"); }
  _insertTable(ctx, args = {}) { const rows = clamp(Number(args.rows) || 2, 1, 20); const cols = clamp(Number(args.cols) || 3, 2, 12); const header = `| ${Array.from({ length: cols }, (_, i) => `Column ${i + 1}`).join(" | ")} |`; const delimiter = `| ${Array.from({ length: cols }, () => "---").join(" | ")} |`; const body = Array.from({ length: rows }, (_, r) => `| ${Array.from({ length: cols }, (_, c) => `Cell ${r * cols + c + 1}`).join(" | ")} |`); const insert = [header, delimiter, ...body].join("\n"); const cursor = ctx.selectionStart + header.indexOf("Column 1"); return ok(tx(ctx, "block.table", [{ from: ctx.selectionStart, to: ctx.selectionEnd, insert }], { start: cursor, end: cursor + "Column 1".length, direction: "none" }, "block"), "Table inserted."); }
  _wrapInline(ctx, prefix, suffix, label) { const selected = ctx.value.slice(ctx.selectionStart, ctx.selectionEnd); if (selected) { const insert = `${prefix}${selected}${suffix}`; const cursor = ctx.selectionStart + insert.length; return ok(tx(ctx, `inline.${label.toLowerCase().replace(/\s+/g, "")}`, [{ from: ctx.selectionStart, to: ctx.selectionEnd, insert }], { start: cursor, end: cursor, direction: "none" }, "inline"), `${label}.`); } const insert = `${prefix}${suffix}`; const cursor = ctx.selectionStart + prefix.length; return ok(tx(ctx, `inline.${label.toLowerCase().replace(/\s+/g, "")}`, [{ from: ctx.selectionStart, to: ctx.selectionEnd, insert }], { start: cursor, end: cursor, direction: "none" }, "inline"), `${label}.`); }
  _insertLink(ctx, args = {}) { const selected = ctx.value.slice(ctx.selectionStart, ctx.selectionEnd); const url = args.url ?? ""; if (selected) { const insert = `[${selected}](${url})`; const cursor = url ? ctx.selectionStart + insert.length : ctx.selectionStart + selected.length + 3; return ok(tx(ctx, "inline.link", [{ from: ctx.selectionStart, to: ctx.selectionEnd, insert }], { start: cursor, end: cursor, direction: "none" }, "inline"), "Link."); } const insert = url ? `[](${url})` : `[]()`; return ok(tx(ctx, "inline.link", [{ from: ctx.selectionStart, to: ctx.selectionEnd, insert }], { start: ctx.selectionStart + 1, end: ctx.selectionStart + 1, direction: "none" }, "inline"), "Link."); }
  _insertImage(ctx, args = {}) { const alt = args.alt ?? ""; const src = args.src ?? ""; const insert = `![${alt}](${src})`; const cursor = alt ? ctx.selectionStart + insert.length : ctx.selectionStart + 2; return ok(tx(ctx, "inline.image", [{ from: ctx.selectionStart, to: ctx.selectionEnd, insert }], { start: cursor, end: cursor, direction: "none" }, "inline"), "Image."); }

  _matchSlash(ctx) {
    if (ctx.block.kind === "fenced-code" || ctx.inline.insideInlineCode) return null;
    const before = ctx.currentLine.text.slice(0, ctx.selectionStart - ctx.currentLine.start);
    const matched = /(^|\s)\/([\w-]*)$/.exec(before);
    if (!matched) return null;
    const slashAt = before.length - matched[2].length - 1;
    return { from: ctx.currentLine.start + slashAt, to: ctx.selectionStart, trigger: "/", query: matched[2], providerId: "slash", midLine: before.slice(0, slashAt).trim().length > 0 };
  }
  _getSlashItems(match) { const q = match.query.toLowerCase(); const items = []; for (const action of this._actions.values()) { if (!action.visibleInSlash) continue; const hay = [action.label, action.description, ...(action.aliases || []), ...(action.keywords || [])].filter(Boolean).join(" ").toLowerCase(); if (q && !hay.includes(q)) continue; items.push({ id: action.id, label: action.label, detail: displayShortcut(action.defaultShortcut), description: "", kind: "slash-command", actionId: action.id }); } return items.slice(0, 24); }
  _applySlashItem(item, match, ctx) {
    const repl = this._slashReplacementForAction(item.actionId);
    if (!repl) return ok(tx(ctx, "completion.accept", [{ from: match.from, to: match.to, insert: "" }], { start: match.from, end: match.from, direction: "none" }, "slash"), item.label);
    let insert = typeof repl.insert === "function" ? repl.insert(ctx) : repl.insert;
    let off = typeof repl.selectionOffset === "number" ? repl.selectionOffset : insert.length;
    let from = match.from;
    if (match.midLine && String(item.actionId || "").startsWith("block.")) {
      insert = "\n" + insert;
      off += 1;
      if (from > ctx.currentLine.start && ctx.value[from - 1] === " ") from -= 1;
    }
    const len = repl.selectionLength || 0;
    return ok(tx(ctx, "completion.accept", [{ from, to: match.to, insert }], { start: from + off, end: from + off + len, direction: "none" }, "slash"), item.label);
  }
  _slashReplacementForAction(actionId) { return { "block.paragraph": { insert: "", selectionOffset: 0 }, "block.heading.1": { insert: "# ", selectionOffset: 2 }, "block.heading.2": { insert: "## ", selectionOffset: 3 }, "block.heading.3": { insert: "### ", selectionOffset: 4 }, "block.heading.4": { insert: "#### ", selectionOffset: 5 }, "block.bulletList": { insert: "- ", selectionOffset: 2 }, "block.orderedList": { insert: "1. ", selectionOffset: 3 }, "block.taskList": { insert: "- [ ] ", selectionOffset: 6 }, "block.blockquote": { insert: "> ", selectionOffset: 2 }, "block.codeFence": { insert: "```\n\n```", selectionOffset: 4 }, "block.horizontalRule": { insert: "---\n", selectionOffset: 4 }, "block.table": { insert: "| Column 1 | Column 2 | Column 3 |\n| --- | --- | --- |\n| Cell 1 | Cell 2 | Cell 3 |", selectionOffset: 2, selectionLength: "Column 1".length }, "inline.link": { insert: "[]()", selectionOffset: 3 }, "inline.image": { insert: "![]()", selectionOffset: 4 }, "inline.bold": { insert: "****", selectionOffset: 2 }, "inline.italic": { insert: "**", selectionOffset: 1 }, "inline.code": { insert: "``", selectionOffset: 1 }, "inline.underline": { insert: "__", selectionOffset: 1 }, "inline.strikethrough": { insert: "~~", selectionOffset: 1 }, "inline.superscript": { insert: "^^", selectionOffset: 1 }, "inline.highlight": { insert: "====", selectionOffset: 2 }, "inline.secret": { insert: "##", selectionOffset: 1 } }[actionId] || null; }
  _matchCodeLanguage(ctx) {
    const fence = this._findBlockAtOffset(ctx.selectionStart, "code-fence");
    if (fence?.opening && ctx.currentLine.start !== fence.opening.start) return null;
    const before = ctx.currentLine.text.slice(0, ctx.selectionStart - ctx.currentLine.start);
    const m = /^(\s*)(`{3,}|~{3,})([\w+-]*)$/.exec(before);
    if (!m || LANGUAGES.includes(m[3].toLowerCase())) return null;
    return { from: ctx.currentLine.start + m[1].length, to: ctx.selectionStart, trigger: m[2], sequence: m[2], query: m[3], providerId: "code-language" };
  }

  _scheduleCompletionUpdate({ immediate = false } = {}) {
    if (this.disabled || this.readonly || this._isComposing) return;
    if (immediate) {
      if (this._completionUpdateFrame) cancelAnimationFrame(this._completionUpdateFrame);
      this._completionUpdateFrame = 0;
      this._maybeUpdateCompletions();
      return;
    }
    if (this._completionUpdateFrame) return;
    this._completionUpdateFrame = requestAnimationFrame(() => {
      this._completionUpdateFrame = 0;
      this._maybeUpdateCompletions();
    });
  }
  _maybeUpdateCompletions() {
    if (this.disabled || this.readonly || this._isComposing) return; const ctx = this._getContext(); if (ctx.selectionStart !== ctx.selectionEnd) { this._closeCompletion(); return; } const providers = [...this._providers.values()].sort((a, b) => b.priority - a.priority); let selectedProvider = null; let selectedMatch = null;
    for (const provider of providers) { try { const match = provider.match(ctx); if (match) { selectedProvider = provider; selectedMatch = match; break; } } catch (error) { this._emitError("completion", error, true, { providerId: provider.id }); } }
    if (!selectedProvider || !selectedMatch) { this._closeCompletion(); return; }
    const requestId = this._completion.requestId + 1; this._completion.requestId = requestId; this._completion.abort?.abort(); const abort = new AbortController(); this._completion.abort = abort;
    try { Promise.resolve(selectedProvider.getItems(selectedMatch, ctx, abort.signal)).then(items => { if (abort.signal.aborted || this._completion.requestId !== requestId) return; const normalized = this._normalizeCompletionItems(items); if (!normalized.length) { this._closeCompletion(); return; } this._openCompletion(selectedProvider.id, selectedMatch, normalized); }).catch(error => { if (!abort.signal.aborted) { this._emitError("completion", error, true, { providerId: selectedProvider.id }); this._closeCompletion(); } }); } catch (error) { this._emitError("completion", error, true, { providerId: selectedProvider.id }); this._closeCompletion(); }
  }
  _normalizeCompletionItems(items) { const seen = new Set(); const out = []; for (const item of items || []) { if (!item?.id || !item?.label) continue; const key = `${item.kind}:${item.id}`; if (seen.has(key)) continue; seen.add(key); out.push(item); } return out; }
  _openCompletion(providerId, match, items) { const was = this._completion.open; this._completion.open = true; this._completion.providerId = providerId; this._completion.match = match; this._completion.items = items; const preferred = clamp(this._completion.activeIndex, 0, items.length - 1); this._completion.activeIndex = this._enabledCompletionIndex(preferred, 1); this._hideSelectionToolbar?.(); this._renderCompletion(); if (!was) this._dispatch("md-completion-open", { providerId, match, items }); }
  _closeCompletion() {
    const wasOpen = this._completion.open;
    const detail = { providerId: this._completion.providerId, match: this._completion.match };
    this._completion.abort?.abort();
    this._completion = { ...this._completion, open: false, providerId: null, match: null, items: [], activeIndex: 0, requestId: this._completion.requestId + 1, abort: null };
    this._renderCompletion();
    if (wasOpen) this._dispatch("md-completion-close", detail);
  }
  _renderCompletion() {
    if (!this._completionPopup) return; const open = this._completion.open && this._completion.items.length > 0; this._completionPopup.hidden = !open; const controller = this._isSourceActive() ? this._sourceTextarea : this._liveEditor; controller?.setAttribute("aria-expanded", open ? "true" : "false");
    if (!open) { this._completionPopup.innerHTML = ""; this._completionPopup.scrollTop = 0; this._sourceTextarea?.removeAttribute("aria-activedescendant"); this._liveEditor?.removeAttribute("aria-activedescendant"); return; }
    const previousScrollTop = this._completionPopup.scrollTop;
    const activeId = this._completion.activeIndex >= 0 ? `${this._ids.completion}-item-${this._completion.activeIndex}` : null;
    if (activeId) controller?.setAttribute("aria-activedescendant", activeId); else controller?.removeAttribute("aria-activedescendant");
    this._completionPopup.innerHTML = this._completion.items.map((item, index) => `<div id="${this._ids.completion}-item-${index}" class="completion-item" part="${index === this._completion.activeIndex ? "completion-item completion-item-active" : "completion-item"}" role="option" aria-selected="${index === this._completion.activeIndex ? "true" : "false"}" aria-disabled="${item.disabled ? "true" : "false"}" data-index="${index}"><div class="completion-label">${escapeHtml(item.label)}</div><div class="completion-detail">${escapeHtml(item.detail || "")}</div>${item.description ? `<div class="completion-description">${escapeHtml(item.description)}</div>` : ""}</div>`).join("");
    this._completionPopup.scrollTop = previousScrollTop;
    this._positionCompletionPopup();
    this._scrollActiveCompletionIntoView();
  }
  _scrollActiveCompletionIntoView() {
    const popup = this._completionPopup;
    const option = popup?.querySelector('[role="option"][aria-selected="true"]');
    if (!popup || !option) return;
    const optionTop = option.offsetTop;
    const optionBottom = optionTop + option.offsetHeight;
    const visibleTop = popup.scrollTop;
    const visibleBottom = visibleTop + popup.clientHeight;
    if (optionTop < visibleTop) popup.scrollTop = optionTop;
    else if (optionBottom > visibleBottom) popup.scrollTop = optionBottom - popup.clientHeight;
  }
  _positionCompletionPopup() {
    const shell = this._shadow.querySelector(".editor-shell"); if (!shell || !this._completionPopup) return; const shellRect = shell.getBoundingClientRect(); let rect = null;
    try { const sel = this._shadow.getSelection?.() || globalThis.getSelection?.(); if (sel?.rangeCount) rect = sel.getRangeAt(0).getBoundingClientRect(); } catch {}
    if (!rect || (!rect.width && !rect.height)) { const target = this._domPositionFromSource(this._selection.start)?.editable || this._sourceTextarea; rect = target?.getBoundingClientRect?.(); }
    const left = clamp((rect?.left ?? shellRect.left) - shellRect.left, 4, Math.max(4, shellRect.width - 260)); const top = clamp((rect?.bottom ?? shellRect.top) - shellRect.top + 6, 4, Math.max(4, shellRect.height - 16));
    this._completionPopup.style.left = `${left}px`; this._completionPopup.style.top = `${top}px`;
  }
  _enabledCompletionIndex(start, direction = 1) {
    const n = this._completion.items.length;
    if (!n) return -1;
    const step = direction < 0 ? -1 : 1;
    for (let distance = 0; distance < n; distance += 1) {
      const index = (start + (distance * step) + n) % n;
      if (!this._completion.items[index]?.disabled) return index;
    }
    return -1;
  }
  _moveCompletion(delta) {
    const n = this._completion.items.length;
    if (!n) return;
    const start = this._completion.activeIndex < 0 ? (delta < 0 ? n - 1 : 0) : this._completion.activeIndex + delta;
    const index = this._enabledCompletionIndex((start + n) % n, delta);
    if (index >= 0) this._setCompletionIndex(index, delta);
  }
  _setCompletionIndex(index, direction = 1) { const n = this._completion.items.length; if (!n) return; this._completion.activeIndex = this._enabledCompletionIndex(clamp(index, 0, n - 1), direction); this._renderCompletion(); }
  _acceptCompletion(source = "action") { if (!this._completion.open || !this._completion.items.length) return fail("not-applicable"); const provider = this._providers.get(this._completion.providerId); const item = this._completion.items[this._completion.activeIndex]; if (!provider || !item || item.disabled) return fail("not-applicable"); const ctx = this._getContext(); let result; try { const currentMatch = provider.match(ctx); if (!currentMatch) { this._closeCompletion(); return fail("not-applicable"); } result = provider.apply(item, currentMatch, ctx); } catch (error) { this._emitError("completion", error, true, { providerId: provider.id }); this._closeCompletion(); return fail("provider-error", String(error?.message || error)); } this._closeCompletion(); if (result?.ok && result.transaction) { const before = this._snapshot(); this._applyTransaction({ ...result.transaction, source: source === "pointer" ? "pointer" : "keyboard", actionId: "completion.accept" }, { source: source === "pointer" ? "pointer" : "keyboard" }); const after = this._snapshot(); this._dispatch("md-completion-accept", { providerId: provider.id, item, before, after }); if (result.announcement) this._announce(result.announcement); return okNoop(result.announcement); } return result || fail("not-applicable"); }

  _updateFormValue() { if (!this._internals) return; this.disabled ? this._internals.setFormValue(null) : this._internals.setFormValue(this._value); }
  _fallbackValidity() { const flags = this._computeValidityFlags(); return { valid: Object.keys(flags).length === 0, valueMissing: Boolean(flags.valueMissing), tooShort: Boolean(flags.tooShort), tooLong: Boolean(flags.tooLong), customError: Boolean(flags.customError) }; }
  _computeValidityFlags() { const flags = {}; const value = this._value; if (this._customValidityMessage) flags.customError = true; if (this.required) { const empty = DEFAULTS.emptyRequiredTrim ? value.trim().length === 0 : value.length === 0; if (empty) flags.valueMissing = true; } const min = parseLengthConstraint(this.getAttribute("minlength")); if (min != null && value.length > 0 && value.length < min) flags.tooShort = true; const max = parseLengthConstraint(this.getAttribute("maxlength")); if (max != null && value.length > max) flags.tooLong = true; return flags; }
  _updateValidity() { if (!this._sourceTextarea) return; const flags = this._computeValidityFlags(); const valid = Object.keys(flags).length === 0; if (valid) this._validationVisible = false; for (const el of [this._sourceTextarea, this._liveEditor]) el?.setAttribute("aria-invalid", valid ? "false" : "true"); this._validation.textContent = ""; this._validationMessage = this._customValidityMessage || ""; const anchor = this.mode === "source" ? this._sourceTextarea : this._liveEditor; const setValidityMessage = valid ? "" : (this._customValidityMessage || " "); this._internals?.setValidity(flags, setValidityMessage, anchor); }
  _emitSelectionChange() { this._dispatch("md-selection-change", { selectionStart: this._selection.start, selectionEnd: this._selection.end, selectionDirection: this._selection.direction || "none" }); }
  _announce(message) { if (!message || !this._status) return; this._status.textContent = ""; requestAnimationFrame(() => { this._status.textContent = message; }); }
  _dispatch(name, detail = {}, options = {}) { const event = new CustomEvent(name, { detail, bubbles: options.bubbles ?? true, composed: options.composed ?? true, cancelable: options.cancelable ?? false }); this.dispatchEvent(event); return event; }
  _emitError(phase, error, recoverable = true, extra = {}) { this._dispatch("md-error", { phase, error, recoverable, ...extra }); }
  formResetCallback() { this.reset(); }
  formDisabledCallback(disabled) {
    const next = Boolean(disabled);
    if (this._formDisabled === next) return;
    this._formDisabled = next;
    if (!this._hasConnected) return;
    if (next) this._closeCompletion();
    this._syncAttributesToControls();
    if (next) this.blur();
    this._renderAll({ restoreSelection: !next, force: true });
    this._updateFormValue();
    this._updateValidity();
  }
  formStateRestoreCallback(state) { if (typeof state === "string") this.value = state; }
}

class MdLiveEditorElement extends WritemarkEditorElement {}

if (globalThis.customElements) {
  if (!customElements.get(TAG_NAME)) customElements.define(TAG_NAME, WritemarkEditorElement);
  if (!customElements.get(LEGACY_TAG_NAME)) customElements.define(LEGACY_TAG_NAME, MdLiveEditorElement);
}

export { WritemarkEditorElement, MdLiveEditorElement, renderMarkdown, renderInlineMarkdown, parseBlocks, parseListItem, parseHeading, parseBlockquote, htmlToMarkdown, tsvToMarkdownTable };
