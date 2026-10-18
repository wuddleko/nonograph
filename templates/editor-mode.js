(function () {
    var key = "nostorgraph-editor-mode";
    var editor = document.querySelector('writemark-editor[name="content"]');
    if (!editor) return;

    function savedMode() {
        try {
            var saved = localStorage.getItem(key);
            return saved === "source" ? "source" : "live";
        } catch (e) {
            return "live";
        }
    }

    function shownMode() {
        return editor.getAttribute("mode") === "source" || editor.mode === "source" ? "source" : "live";
    }

    function paint(mode) {
        var buttons = document.querySelectorAll(".view-mode button");
        for (var i = 0; i < buttons.length; i++) {
            var on = buttons[i].getAttribute("data-editor-mode") === mode;
            buttons[i].setAttribute("aria-pressed", on ? "true" : "false");
        }
    }

    var initial = savedMode();
    if (initial === "source") editor.setAttribute("mode", "source");
    paint(initial);

    new MutationObserver(function () {
        paint(shownMode());
    }).observe(editor, { attributes: true, attributeFilter: ["mode"] });

    document.addEventListener("click", function (event) {
        var button = event.target && event.target.closest ? event.target.closest(".view-mode button") : null;
        if (!button) return;
        var next = button.getAttribute("data-editor-mode");
        if ((next !== "live" && next !== "source") || next === shownMode()) return;
        editor.mode = next;
        if (typeof editor.focus === "function") editor.focus();
        try { localStorage.setItem(key, next); } catch (e) {}
        paint(next);
    });
})();
