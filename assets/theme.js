// Loaded before the page is drawn (not deferred), apart from system.js so
// the Content-Security-Policy can refuse every inline script.
// Tells the CSS JavaScript runs: the phone menu can be folded.
document.documentElement.classList.add("js");
// The theme chosen with the menu's button (see system.js), so the page never
// flashes in the other one.
try {
    const theme = localStorage.getItem("tison-theme");
    if (theme === "light" || theme === "dark") {
        document.documentElement.dataset.theme = theme;
    }
} catch {}
