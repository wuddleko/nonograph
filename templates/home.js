            const editor = document.querySelector('writemark-editor[name="content"]');
            const charCount = document.getElementById("charCount");
            const mobileCharCount = document.getElementById("mobileCharCount");
            const form = document.getElementById("publishForm");
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

            function updateCharCount() {
                const count = editor.value.length;
                const button = document.querySelector('button[type="submit"]');
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
                    if (button) button.disabled = true;
                } else {
                    if (charCount) charCount.classList.remove("over-limit");
                    if (mobileCharCount)
                        mobileCharCount.classList.remove("over-limit");
                    if (button) button.disabled = false;
                }
            }

            // The writemark editor emits input events as the document changes.
            editor.addEventListener("md-input", updateCharCount);
            editor.addEventListener("input", updateCharCount);
            editor.addEventListener("md-change", updateCharCount);

            form.addEventListener("submit", function (e) {
                if (editor.value.length > contentLimit) {
                    e.preventDefault();
                    alert(
                        "Content exceeds " +
                            contentLimit.toLocaleString() +
                            " character limit.",
                    );
                    return false;
                }
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
