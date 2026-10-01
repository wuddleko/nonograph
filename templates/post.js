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

            function computeSelectionHash() {
                if (!document.getSelection) return "";
                var sel = document.getSelection();
                if (sel.isCollapsed) return "";
                var range = sel.getRangeAt(0);
                var begin = [0, 0],
                    end = [0, 0];
                walkContent(function (e, pos) {
                    if (range.startContainer === e)
                        begin = [pos, range.startOffset];
                    if (range.endContainer === e) end = [pos, range.endOffset];
                });
                if (begin[0] > 0 && end[0] > 0)
                    return (
                        "selection-" +
                        begin[0] +
                        "." +
                        begin[1] +
                        "-" +
                        end[0] +
                        "." +
                        end[1]
                    );
                return "";
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
            document.addEventListener("selectionchange", function () {
                if (!_hl) {
                    var h = computeSelectionHash();
                    if (h.length > 0) applySelectionHash(h);
                }
            });
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
                    var mk = document.createElement("mark");
                    mk.className = "selection-highlight";
                    r.surroundContents(mk);
                    _hl = false;
                    window.scrollTo({
                        top:
                            mk.getBoundingClientRect().top +
                            window.scrollY -
                            window.innerHeight * 0.2,
                        behavior: "smooth",
                    });
                } catch (x) {
                    _hl = false;
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
