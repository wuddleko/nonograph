(function () {
    var key = "nostorgraph-theme";
    var root = document.documentElement;
    var query = matchMedia("(prefers-color-scheme: dark)");
    root.classList.add("js");
    try {
        var saved = localStorage.getItem(key);
        if (saved === "dark" || saved === "light") root.dataset.theme = saved;
    } catch (e) {}
    function darkOn() {
        var chosen = root.dataset.theme;
        return chosen === "dark" || (chosen !== "light" && query.matches);
    }
    function mark() {
        var pressed = darkOn() ? "true" : "false";
        var buttons = document.getElementsByClassName("theme-toggle");
        for (var i = 0; i < buttons.length; i++) buttons[i].setAttribute("aria-pressed", pressed);
    }
    document.addEventListener("click", function (event) {
        var node = event.target;
        if (!node || !node.closest || !node.closest(".theme-toggle")) return;
        var next = darkOn() ? "light" : "dark";
        root.dataset.theme = next;
        try { localStorage.setItem(key, next); } catch (e) {}
        mark();
    });
    window.addEventListener("storage", function (event) {
        if (event.key !== key) return;
        if (event.newValue === "dark" || event.newValue === "light") root.dataset.theme = event.newValue;
        else delete root.dataset.theme;
        mark();
    });
    query.addEventListener("change", function () {
        if (!root.dataset.theme) mark();
    });
    if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", mark);
    else mark();
})();
