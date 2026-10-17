            // Secret text reveal functionality
            document.querySelectorAll(".secret").forEach(function (secret) {
                secret.addEventListener("click", function () {
                    this.classList.toggle("revealed");
                });
            });

            document.querySelectorAll("a.nostr-id").forEach(function (link) {
                if (!link.textContent) return;
                link.addEventListener("click", function (event) {
                    if (
                        event.button !== 0 ||
                        event.metaKey ||
                        event.ctrlKey ||
                        event.shiftKey ||
                        event.altKey
                    ) {
                        return;
                    }
                    var label = link.textContent;
                    if (!label || !navigator.clipboard || !navigator.clipboard.writeText) {
                        return;
                    }
                    event.preventDefault();
                    navigator.clipboard.writeText(label).then(function () {
                        link.textContent = "copied";
                        window.setTimeout(function () {
                            link.textContent = label;
                        }, 1200);
                    }).catch(function () {});
                });
            });

            // Walk all element/text nodes in #CONTENT, calling visitor(node, pos)
            // at each step. Uses the same positional encoding as the hash format.
            // Code blatantly stolen from archive.today; hey it works great!
            function walkContent(visitor) {
                var pos = 0;
                function recur(e) {
                    if (
                        e.nodeType == 1 &&
                        (e.tagName == "OLD-META" || e.tagName == "OLD-SCRIPT")
                    )
                        return;
                    if (e.nodeType == 1) pos = (pos & ~1) + 2;
                    if (e.nodeType == 3) pos = pos | 1;
                    visitor(e, pos);
                    var lastChild = null;
                    for (var i = 0; i < e.childNodes.length; i++)
                        if (
                            e.childNodes[i].nodeType == 1 ||
                            e.childNodes[i].nodeType == 3
                        )
                            recur((lastChild = e.childNodes[i]));
                    if (lastChild && lastChild.nodeType == 3)
                        pos = (pos & ~1) + 2;
                }
                recur(document.getElementById("CONTENT"));
            }

            var positionByNode = null;
            function contentPositions() {
                if (positionByNode) return positionByNode;
                var map = new Map();
                var root = document.getElementById("CONTENT");
                if (root) {
                    walkContent(function (node, pos) {
                        map.set(node, pos);
                    });
                }
                positionByNode = map;
                return map;
            }
            function forgetContentPositions() {
                positionByNode = null;
            }
            function textOffsets(node, range) {
                if (node.nodeType !== 3 || !node.length) return null;
                try {
                    if (!range.intersectsNode(node)) return null;
                } catch (e) {
                    return null;
                }
                var start = range.startContainer === node ? range.startOffset : 0;
                var end = range.endContainer === node ? range.endOffset : node.length;
                if (start < 0) start = 0;
                if (end > node.length) end = node.length;
                if (start >= end) return null;
                return [start, end];
            }
            // A selection that starts in the title or ends in the footer still
            // covers the article. Hash the article portion.
            function clipToContent(range, root) {
                try {
                    if (!range.intersectsNode(root)) return null;
                } catch (e) {
                    return null;
                }
                var startNode = null,
                    startOff = 0,
                    endNode = null,
                    endOff = 0;
                walkContent(function (node) {
                    var off = textOffsets(node, range);
                    if (!off) return;
                    if (!startNode) {
                        startNode = node;
                        startOff = off[0];
                    }
                    endNode = node;
                    endOff = off[1];
                });
                if (!startNode || !endNode) return null;
                return [startNode, startOff, endNode, endOff];
            }
            function computeSelectionHash() {
                if (!document.getSelection) return "";
                var sel = document.getSelection();
                if (!sel || sel.isCollapsed || sel.rangeCount === 0) return "";
                var range = sel.getRangeAt(0);
                var root = document.getElementById("CONTENT");
                if (!root) return "";
                var startNode, startOff, endNode, endOff;
                if (
                    root.contains(range.startContainer) &&
                    root.contains(range.endContainer)
                ) {
                    startNode = range.startContainer;
                    startOff = range.startOffset;
                    endNode = range.endContainer;
                    endOff = range.endOffset;
                } else {
                    var clipped = clipToContent(range, root);
                    if (!clipped) return "";
                    startNode = clipped[0];
                    startOff = clipped[1];
                    endNode = clipped[2];
                    endOff = clipped[3];
                }
                var map = contentPositions();
                var begin = map.get(startNode);
                var end = map.get(endNode);
                if (begin == null || end == null) {
                    forgetContentPositions();
                    map = contentPositions();
                    begin = map.get(startNode);
                    end = map.get(endNode);
                }
                if (!begin || !end) return "";
                return (
                    "selection-" +
                    begin +
                    "." +
                    startOff +
                    "-" +
                    end +
                    "." +
                    endOff
                );
            }

            function applySelectionHash(newhash) {
                var oldhash = location.hash.replace(/^#/, "");
                if (oldhash == newhash) return;
                if (history.replaceState) {
                    history.replaceState(
                        "",
                        document.title,
                        document.location.origin +
                            document.location.pathname +
                            document.location.search +
                            (newhash.length > 0 ? "#" + newhash : ""),
                    );
                } else if (newhash.length > 0) {
                    location.hash = newhash;
                }
            }

            var _initHash = location.hash;
            var _hl = false;
            function inHighlight(node) {
                var el = node && (node.nodeType === 1 ? node : node.parentNode);
                while (el && el !== document.documentElement) {
                    if (
                        el.classList &&
                        el.classList.contains("selection-highlight")
                    )
                        return true;
                    el = el.parentNode;
                }
                return false;
            }
            function publishSelectionHash() {
                window.clearTimeout(selectionTimer);
                selectionTimer = 0;
                if (_hl) return;
                var sel = document.getSelection && document.getSelection();
                if (sel && sel.rangeCount && !sel.isCollapsed) {
                    var range = sel.getRangeAt(0);
                    if (
                        inHighlight(range.startContainer) &&
                        inHighlight(range.endContainer)
                    )
                        return;
                }
                var h = computeSelectionHash();
                if (h.length > 0) applySelectionHash(h);
            }
            var selectionTimer = 0;
            function scheduleSelectionHash() {
                if (_hl) return;
                window.clearTimeout(selectionTimer);
                selectionTimer = window.setTimeout(publishSelectionHash, 200);
            }
            document.addEventListener("selectionchange", scheduleSelectionHash);
            document.addEventListener("mouseup", publishSelectionHash);
            document.addEventListener("keyup", publishSelectionHash);
            function wrapSlice(node, start, end) {
                var slice = document.createRange();
                slice.setStart(node, start);
                slice.setEnd(node, end);
                var mk = document.createElement("mark");
                mk.className = "selection-highlight";
                slice.surroundContents(mk);
                return mk;
            }
            function highlightRange(range) {
                var mk = document.createElement("mark");
                mk.className = "selection-highlight";
                try {
                    range.surroundContents(mk);
                    return mk;
                } catch (e) {
                    var slices = [];
                    walkContent(function (node) {
                        var off = textOffsets(node, range);
                        if (off) slices.push([node, off[0], off[1]]);
                    });
                    var first = null;
                    for (var i = 0; i < slices.length; i++) {
                        var wrapped = wrapSlice(
                            slices[i][0],
                            slices[i][1],
                            slices[i][2],
                        );
                        if (!first) first = wrapped;
                    }
                    return first;
                }
            }
            function _doHighlight() {
                var m = _initHash.match(
                    /^#selection-(\d+)\.(\d+)-(\d+)\.(\d+)$/,
                );
                if (!m) return;
                var sP = +m[1],
                    sO = +m[2],
                    eP = +m[3],
                    eO = +m[4],
                    sN,
                    eN;
                walkContent(function (e, p) {
                    if (p === sP && !sN && (e.nodeType !== 3 || sO <= e.length))
                        sN = e;
                    if (p === eP && !eN && (e.nodeType !== 3 || eO <= e.length))
                        eN = e;
                });
                if (!sN || !eN) return;
                if (sN.nodeType === 3) sO = Math.min(sO, sN.length);
                if (eN.nodeType === 3) eO = Math.min(eO, eN.length);
                try {
                    var r = document.createRange();
                    r.setStart(sN, sO);
                    r.setEnd(eN, eO);
                    _hl = true;
                    window.clearTimeout(selectionTimer);
                    var mk = highlightRange(r);
                    forgetContentPositions();
                    if (!mk) {
                        window.setTimeout(function () {
                            _hl = false;
                        }, 400);
                        return;
                    }
                    window.scrollTo({
                        top:
                            mk.getBoundingClientRect().top +
                            window.scrollY -
                            window.innerHeight * 0.2,
                        behavior: "smooth",
                    });
                    window.setTimeout(function () {
                        _hl = false;
                    }, 400);
                } catch (x) {
                    _hl = false;
                    forgetContentPositions();
                }
            }
            if (document.readyState === "loading")
                document.addEventListener("DOMContentLoaded", _doHighlight);
            else _doHighlight();

            // Process pre-rendered code blocks to add interactivity
            document.addEventListener("DOMContentLoaded", function () {
                document.querySelectorAll("pre").forEach(function (pre) {
                    // Only process pre elements that have our structure
                    const header = pre.querySelector(".code-header");
                    if (!header) return;

                    const wrapButton = pre.querySelector(".wrap-button");
                    const collapseButton =
                        pre.querySelector(".collapse-button");
                    const copyButton = pre.querySelector(".copy-button");
                    const codeBlock = pre.querySelector("code");

                    // Wrap button functionality
                    if (wrapButton && pre) {
                        const lineNumberEls = Array.from(
                            pre.querySelectorAll(".line-number"),
                        );
                        const codeLineEls = codeBlock
                            ? Array.from(
                                  codeBlock.querySelectorAll(".code-line"),
                              )
                            : [];
                        const baseLineHeight = parseFloat(
                            getComputedStyle(codeBlock || pre).lineHeight,
                        );

                        const updateLineHeights = function () {
                            const isWrapped = pre.classList.contains("wrap");
                            codeLineEls.forEach(function (codeLine, i) {
                                if (!lineNumberEls[i]) return;
                                if (isWrapped) {
                                    const h =
                                        codeLine.getBoundingClientRect().height;
                                    lineNumberEls[i].style.height = h + "px";
                                } else {
                                    lineNumberEls[i].style.height = "";
                                }
                            });
                        };

                        wrapButton.addEventListener("click", function () {
                            pre.classList.toggle("wrap");
                            wrapButton.classList.toggle("active");
                            wrapButton.querySelector(".btn-label").textContent =
                                pre.classList.contains("wrap")
                                    ? "Unwrap"
                                    : "Wrap";
                            updateLineHeights();
                        });
                    }

                    // Collapse button functionality
                    if (collapseButton && pre) {
                        collapseButton.addEventListener("click", function () {
                            pre.classList.toggle("collapsed");
                            collapseButton.classList.toggle("active");
                            collapseButton.querySelector(
                                ".btn-label",
                            ).textContent = pre.classList.contains("collapsed")
                                ? "Expand"
                                : "Collapse";
                        });
                    }

                    // Copy button functionality
                    if (copyButton && codeBlock) {
                        copyButton.addEventListener("click", function () {
                            const showCopiedState = function () {
                                copyButton.classList.add("copied");
                                copyButton.querySelector(
                                    ".btn-label",
                                ).textContent = "Copied!";
                                setTimeout(function () {
                                    copyButton.classList.remove("copied");
                                    copyButton.querySelector(
                                        ".btn-label",
                                    ).textContent = "Copy";
                                }, 2000);
                            };

                            if (navigator.clipboard) {
                                navigator.clipboard
                                    .writeText(codeBlock.textContent)
                                    .then(showCopiedState)
                                    .catch(() => {
                                        // Fallback for older browsers
                                        const textArea =
                                            document.createElement("textarea");
                                        textArea.value = codeBlock.textContent;
                                        document.body.appendChild(textArea);
                                        textArea.select();
                                        document.execCommand("copy");
                                        document.body.removeChild(textArea);
                                        showCopiedState();
                                    });
                            } else {
                                // Fallback for older browsers
                                const textArea =
                                    document.createElement("textarea");
                                textArea.value = codeBlock.textContent;
                                document.body.appendChild(textArea);
                                textArea.select();
                                document.execCommand("copy");
                                document.body.removeChild(textArea);
                                showCopiedState();
                            }
                        });
                    }
                });
            });
