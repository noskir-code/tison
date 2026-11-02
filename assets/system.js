// Tison client-side helpers, shared by every page (loaded in base.html).
//
// Times: any `<time datetime="..." data-relative>` gets a relative text
// ("5 minutes ago", "3 days ago"...) refreshed while the page is open, and
// its local date and hour as a tooltip. Without JavaScript the text written
// by the server (in UTC) is kept.
//
// Links: any `<button data-share-url="/path" hidden>` is shown and opens the
// device's share menu (phones), or copies the full link to the clipboard.
//
// Long posts: any `<p data-collapsible id="...">` taller than the limit set in
// style.css (`--collapsed-lines`) is cut with a t("Show more") button. Without
// JavaScript posts stay complete.
//
// Notifications: the unread count in the menu (`[data-unread]`) is kept up to
// date through a WebSocket to /notifications/live. On the notifications page
// new notifications appear at the top, read and deleted ones update at once,
// actions are sent without reloading, and older ones load when scrolling
// down. Without JavaScript the same links and forms reload the page.
// Browser notifications (`[data-push]`, settings and notifications page) are
// turned on per browser; assets/sw.js shows them when no page is in front.
//
// Articles: the formatting buttons (`[data-markdown-toolbar]`) write their
// Markdown around the selection or the word under the cursor, and remove it
// when pressed again; Ctrl+B, I, U and K work in the text too. Under the
// text (`[data-article-preview]`), the article as it will be shown.
//
// Dashboard: each `[data-chart]` is drawn as a line chart, with a tooltip
// under the pointer or the arrow keys. Videos count a view after 30 seconds.
//
// Replies: "Reply" links (`[data-reply-to]`) fill the comment form in place
// instead of reloading the page with `?reply_to=`.
//
// Deletions: forms with `data-confirm="Question?"` ask in a dialog of the
// site before being sent. Without JavaScript they are sent at once.
//
// Phone menu: the ☰ button (`[data-menu-toggle]`) opens and closes the menu
// (`.menu-open` on <html>, see style.css); the dimmed page behind it and
// Escape close it too.
//
// Messenger: the count next to "Messenger" follows the same WebSocket as the
// notifications; an open conversation fetches its new messages when told it
// changed, and Enter sends a message (Shift+Enter for a new line).
//
// Theme: the button under Settings (`[data-theme-toggle]`) switches between
// light and dark, remembered on this device. Until then the system's theme
// is followed. base.html applies the saved theme before the page is drawn.
//
// Search: from 3 letters typed in the search bar, the most relevant people
// and communities show under it (from /search/suggest), chosen with the
// mouse or the arrow keys and Enter. Without JavaScript, Enter opens the
// results page.

const Tison = (() => {
    // Same language as the interface, not the browser, so pages don't mix languages.
    const language = document.documentElement.lang || "en";

    // The texts of this file in the page's language, given by the page (see
    // src/i18n.rs): `t("Show more")`, `tf("Uploading {}: {}%", name, 40)`.
    const I18N = (() => {
        try {
            return JSON.parse(document.getElementById("i18n")?.textContent || "{}");
        } catch {
            return {};
        }
    })();
    const t = (text) => I18N[text] ?? text;
    const tf = (text, ...values) => values.reduce((result, value) => result.replace("{}", value), t(text));

    // From the largest unit to the smallest: the first one that fits is used.
    // Months and years are averages, precise enough for "x months ago".
    const UNITS = [
        ["year", 365.25 * 24 * 60 * 60],
        ["month", 30.44 * 24 * 60 * 60],
        ["week", 7 * 24 * 60 * 60],
        ["day", 24 * 60 * 60],
        ["hour", 60 * 60],
        ["minute", 60],
        ["second", 1],
    ];

    const relativeFormat = new Intl.RelativeTimeFormat(language, { numeric: "always" });
    const localFormat = new Intl.DateTimeFormat(language, {
        dateStyle: "full",
        timeStyle: "medium",
    });

    // "5 minutes ago". Dates slightly in the future (clocks not in sync) count as now.
    function formatRelative(date, now = new Date()) {
        const seconds = Math.max(0, (now - date) / 1000);
        for (const [unit, unitSeconds] of UNITS) {
            if (seconds >= unitSeconds || unit === "second") {
                return relativeFormat.format(-Math.floor(seconds / unitSeconds), unit);
            }
        }
    }

    // "Friday, October 30, 2026 at 6:45:12 PM", in the viewer's time zone.
    function formatLocal(date) {
        return localFormat.format(date);
    }

    // Updates every relative time inside `root`.
    function refreshTimes(root = document) {
        const now = new Date();
        for (const element of root.querySelectorAll("time[datetime][data-relative]")) {
            const date = new Date(element.getAttribute("datetime"));
            if (Number.isNaN(date.getTime())) {
                continue;
            }
            element.textContent = formatRelative(date, now);
            element.title = formatLocal(date);
        }
    }

    // Full URL of a path on this site.
    function absoluteUrl(path) {
        return new URL(path, window.location.origin).href;
    }

    async function shareLink(button) {
        const url = absoluteUrl(button.dataset.shareUrl);
        if (navigator.share) {
            try {
                await navigator.share({ url });
            } catch {
                // Closed by the user, nothing to do.
            }
            return;
        }
        const label = button.textContent;
        try {
            await navigator.clipboard.writeText(url);
            button.textContent = t("Link copied");
        } catch {
            // Clipboard refused (permissions, or not HTTPS): show the link to copy by hand.
            window.prompt("Copy this link:", url);
            return;
        }
        setTimeout(() => {
            button.textContent = label;
        }, 2000);
    }

    // Shows and activates the share buttons inside `root`, once each.
    function setupShareButtons(root = document) {
        for (const button of root.querySelectorAll("button[data-share-url]")) {
            if (button.dataset.shareReady) {
                continue;
            }
            button.dataset.shareReady = "true";
            if (navigator.share) {
                button.textContent = t("Share link");
            }
            button.hidden = false;
            button.addEventListener("click", () => shareLink(button));
        }
    }

    // Collapses the long texts inside `root`, once each.
    function setupCollapsible(root = document) {
        for (const text of root.querySelectorAll("[data-collapsible]")) {
            if (text.dataset.collapseReady) {
                continue;
            }
            text.dataset.collapseReady = "true";
            text.classList.add("collapsed");
            // Short enough: nothing hidden, no button.
            if (text.scrollHeight <= text.clientHeight + 1) {
                text.classList.remove("collapsed");
                continue;
            }

            const button = document.createElement("button");
            button.type = "button";
            button.className = "action show-more";
            button.textContent = t("Show more");
            button.setAttribute("aria-expanded", "false");
            button.setAttribute("aria-controls", text.id);
            button.addEventListener("click", () => {
                const collapsed = text.classList.toggle("collapsed");
                button.textContent = collapsed ? "Show more" : t("Show less");
                button.setAttribute("aria-expanded", String(!collapsed));
            });
            text.after(button);
        }
    }

    function showUnread(unread) {
        const badge = document.querySelector("[data-notifications] [data-unread]");
        if (badge) {
            badge.firstChild.textContent = unread > 99 ? "99+" : String(unread);
            badge.hidden = unread === 0;
        }
        const readAll = document.querySelector("[data-read-all]");
        if (readAll) {
            readAll.hidden = unread === 0;
        }
    }

    /** The notifications list, `null` on other pages. */
    function notificationList() {
        return document.querySelector("[data-notification-list]");
    }

    /** Shows the empty message and hides "Delete all" when the list is empty. */
    function refreshEmptyState() {
        const list = notificationList();
        if (!list) {
            return;
        }
        const empty = list.children.length === 0;
        const message = document.querySelector("[data-notifications-empty]");
        if (message) {
            message.hidden = !empty;
        }
        const deleteAll = document.querySelector("[data-delete-all]");
        if (deleteAll) {
            deleteAll.hidden = empty;
        }
    }

    /** Turns server HTML into elements, ready to insert. */
    function parseItems(html) {
        const template = document.createElement("template");
        template.innerHTML = html;
        return [...template.content.children];
    }

    function markItemRead(item) {
        item.classList.remove("unread");
        item.querySelector("[data-unread-label]")?.setAttribute("hidden", "");
    }

    /** Applies a change sent by the server (see handlers/notification.rs). */
    function applyNotificationChange(change) {
        if (typeof change.unread === "number") {
            showUnread(change.unread);
        }
        if (typeof change.unread_messages === "number") {
            showUnreadMessages(change.unread_messages);
        }
        if (change.message) {
            refreshConversation(change.message);
        }
        const list = notificationList();
        if (!list) {
            return;
        }
        if (change.added && list.hasAttribute("data-newest")) {
            // Already shown, from a reconnection: replaced by the fresh version.
            document.getElementById(`notification-${change.added.id}`)?.remove();
            for (const item of parseItems(change.added.html)) {
                item.classList.add("arriving");
                list.prepend(item);
                refreshTimes(item);
            }
        }
        if (change.removed === "all") {
            list.replaceChildren();
        } else if (change.removed) {
            document.getElementById(`notification-${change.removed}`)?.remove();
        }
        if (change.read === "all") {
            list.querySelectorAll(".notification.unread").forEach(markItemRead);
        } else if (change.read) {
            const item = document.getElementById(`notification-${change.read}`);
            if (item) {
                markItemRead(item);
            }
        }
        refreshEmptyState();
    }

    /**
     * Keeps notifications up to date: the server sends `{"unread": n}` when
     * connecting, then every change. Reconnects after a lost connection,
     * waiting longer each time (up to a minute).
     */
    function connectNotifications() {
        const badge = document.querySelector("[data-notifications] [data-unread]");
        // Only signed-in pages have a count.
        if (!badge || !("WebSocket" in window)) {
            return;
        }
        const address = new URL("/notifications/live", location.href);
        address.protocol = location.protocol === "https:" ? "wss:" : "ws:";
        let delay = 1000;
        let socket = null;

        function open() {
            socket = new WebSocket(address);
            socket.addEventListener("open", () => {
                delay = 1000;
            });
            socket.addEventListener("message", (event) => {
                let change;
                try {
                    change = JSON.parse(event.data);
                } catch {
                    return;
                }
                applyNotificationChange(change);
            });
            socket.addEventListener("close", () => {
                socket = null;
                // Hidden tabs reconnect when shown again, see below.
                if (!document.hidden) {
                    setTimeout(open, delay);
                    delay = Math.min(delay * 2, 60 * 1000);
                }
            });
        }

        document.addEventListener("visibilitychange", () => {
            if (!document.hidden && socket === null) {
                delay = 1000;
                open();
            }
        });
        open();
    }

    const pushSupported = "serviceWorker" in navigator && "PushManager" in window && "Notification" in window;
    /** Set when turned off with the button. */
    const PUSH_OFF = "tison-push-off";
    /** When "Not now" was chosen. */
    const PUSH_ASKED = "tison-push-asked";
    const PUSH_ASK_AGAIN_DAYS = 30;

    /** localStorage, which private windows may refuse: then nothing is remembered. */
    function remembered(key) {
        try {
            return localStorage.getItem(key);
        } catch {
            return null;
        }
    }

    function remember(key, value) {
        try {
            if (value === null) {
                localStorage.removeItem(key);
            } else {
                localStorage.setItem(key, value);
            }
        } catch {
            // Asked again next time.
        }
    }

    /** This browser's push subscription, `null` when there is none. */
    async function pushSubscription() {
        const registration = await navigator.serviceWorker.getRegistration("/");
        return registration ? registration.pushManager.getSubscription() : null;
    }

    function postJson(url, body) {
        return fetch(url, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(body),
        });
    }

    function base64UrlBytes(text) {
        const binary = atob(text.replace(/-/g, "+").replace(/_/g, "/"));
        return Uint8Array.from(binary, (letter) => letter.charCodeAt(0));
    }

    function sameBytes(buffer, bytes) {
        const other = new Uint8Array(buffer || []);
        return other.length === bytes.length && other.every((byte, index) => byte === bytes[index]);
    }

    /** Asks for the permission, subscribes with the server's key, and tells the server. */
    async function turnPushOn() {
        if ((await Notification.requestPermission()) !== "granted") {
            return;
        }
        const keyResponse = await fetch("/push/key");
        if (!keyResponse.ok) {
            throw new Error(`no server key (${keyResponse.status})`);
        }
        const key = base64UrlBytes(await keyResponse.text());
        await navigator.serviceWorker.register("/sw.js", { scope: "/" });
        const registration = await navigator.serviceWorker.ready;
        let subscription = await registration.pushManager.getSubscription();
        // Made with another key: a browser keeps one subscription per site.
        if (subscription && !sameBytes(subscription.options.applicationServerKey, key)) {
            await subscription.unsubscribe();
            subscription = null;
        }
        subscription ||= await registration.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: key });
        const response = await postJson("/push/subscribe", subscription.toJSON());
        if (!response.ok) {
            throw new Error(`refused by the server (${response.status})`);
        }
        remember(PUSH_OFF, null);
    }

    /** `byUser`: with the button, so they aren't turned back on by themselves. */
    async function turnPushOff(byUser) {
        if (byUser) {
            remember(PUSH_OFF, "1");
        }
        const subscription = await pushSubscription();
        if (subscription) {
            await postJson("/push/unsubscribe", { endpoint: subscription.endpoint });
            await subscription.unsubscribe();
        }
    }

    /** "on", "off", "blocked" (refused in the browser) or "unsupported". */
    async function pushState() {
        if (!pushSupported) {
            return "unsupported";
        }
        if (Notification.permission === "denied") {
            return "blocked";
        }
        const subscription = await pushSubscription();
        if (!subscription || Notification.permission !== "granted") {
            return "off";
        }
        // Another account may have turned them on in this browser since.
        const response = await postJson("/push/status", { endpoint: subscription.endpoint });
        return response.ok && (await response.json()).on ? "on" : "off";
    }

    const PUSH_STATUS = {
        on: t("On in this browser."),
        off: t("Off in this browser."),
        busy: "…",
        blocked: t("Blocked: allow notifications for this site in the browser's settings."),
        failed: t("This browser couldn't turn them on: its push service may be off."),
    };
    // Brave turns off the push service it borrows from Google.
    if (navigator.brave) {
        PUSH_STATUS.failed = t("Brave's push service is off: turn on \"Use Google services for push messaging\" in brave://settings/privacy, relaunch Brave, then try again.");
    }

    /**
     * Browser notifications (see src/push.rs and assets/sw.js): every
     * `[data-push]` gets a `[data-push-toggle]` button turning them on or off
     * in this browser (labels in its `data-on` and `data-off`), and
     * optionally a `[data-push-status]` text. Hidden when unsupported.
     *
     * Asked automatically: when the browser already allows them (after
     * signing out and in again), they're turned on without a question;
     * otherwise the `[data-push-ask]` banner asks, at most every 30 days
     * after "Not now". Never after "Turn off", or when the browser blocks
     * them. Signing out turns them off in this browser.
     */
    function setupPush() {
        const logout = document.querySelector('form[action="/logout"]');
        if (!pushSupported || !logout) {
            return;
        }
        logout.addEventListener("submit", async (event) => {
            event.preventDefault();
            // At most 3 seconds: signing out comes first.
            const timeout = new Promise((resolve) => setTimeout(resolve, 3000));
            await Promise.race([turnPushOff(false).catch(() => {}), timeout]);
            logout.submit();
        });
        const controls = document.querySelectorAll("[data-push]");
        const refresh = setupPushControls(controls);
        if (remembered(PUSH_OFF)) {
            return;
        }
        if (Notification.permission === "granted") {
            pushSubscription().then(async (subscription) => {
                // Not tried again in this tab after failing: a browser
                // without push service fails every time.
                let failed = false;
                try {
                    failed = sessionStorage.getItem(PUSH_ASKED) !== null;
                } catch {
                    // Tried on each page then.
                }
                if (subscription || failed) {
                    return;
                }
                try {
                    await turnPushOn();
                } catch (error) {
                    console.warn("browser notifications:", error);
                    try {
                        sessionStorage.setItem(PUSH_ASKED, "1");
                    } catch {
                        // Tried on each page then.
                    }
                }
                refresh();
            });
        } else if (Notification.permission === "default" && controls.length === 0) {
            const asked = Number(remembered(PUSH_ASKED)) || 0;
            if (Date.now() - asked > PUSH_ASK_AGAIN_DAYS * 24 * 3600 * 1000) {
                askForPush();
            }
        }
    }

    /** Shows the banner asking to turn them on. */
    function askForPush() {
        const banner = document.querySelector("[data-push-ask]");
        if (!banner) {
            return;
        }
        const yes = banner.querySelector("[data-push-ask-yes]");
        const later = banner.querySelector("[data-push-ask-later]");
        banner.hidden = false;
        later.addEventListener("click", () => {
            remember(PUSH_ASKED, String(Date.now()));
            banner.hidden = true;
        });
        yes.addEventListener("click", async () => {
            yes.disabled = later.disabled = true;
            try {
                await turnPushOn();
            } catch (error) {
                console.warn("browser notifications:", error);
                banner.querySelector("[data-push-ask-text]").textContent = PUSH_STATUS.failed;
                later.textContent = t("Close");
                later.disabled = false;
                return;
            }
            // Refused in the browser's question: not asked again for a while.
            if (Notification.permission !== "granted") {
                remember(PUSH_ASKED, String(Date.now()));
            }
            banner.hidden = true;
        });
    }

    /** The on/off buttons, returns what updates them. */
    function setupPushControls(controls) {
        if (controls.length === 0) {
            return () => {};
        }
        let current = "unsupported";
        const render = (state) => {
            controls.forEach((control) => {
                control.hidden = state === "unsupported";
                const button = control.querySelector("[data-push-toggle]");
                button.textContent = current === "on" ? button.dataset.on : button.dataset.off;
                button.disabled = state === "busy" || state === "blocked";
                const test = control.querySelector("[data-push-test]");
                if (test) {
                    test.hidden = current !== "on";
                }
                const status = control.querySelector("[data-push-status]");
                if (status) {
                    status.textContent = PUSH_STATUS[state] || "";
                } else {
                    button.title = PUSH_STATUS[state] || "";
                }
            });
        };
        const refresh = async () => {
            current = await pushState().catch(() => "off");
            render(current);
        };
        controls.forEach((control) => {
            // Goes through the push service like a real one, shown even with the page in front.
            const test = control.querySelector("[data-push-test]");
            const status = control.querySelector("[data-push-status]");
            test?.addEventListener("click", async () => {
                test.disabled = true;
                try {
                    const response = await fetch("/push/test", { method: "POST" });
                    const result = response.ok ? await response.json() : null;
                    status.textContent = !result
                        ? "Failed to send the test."
                        : result.accepted === result.browsers
                            ? "Test sent: it should appear in a few seconds."
                            : tf("Sent to {} of {} browsers: the others turned them off (removed now) or their push service refused it (see the server's log).", result.accepted, result.browsers);
                } catch {
                    status.textContent = t("Failed to send the test.");
                }
                test.disabled = false;
            });
            control.querySelector("[data-push-toggle]").addEventListener("click", async () => {
                const wasOn = current === "on";
                render("busy");
                let failed = false;
                try {
                    await (wasOn ? turnPushOff(true) : turnPushOn());
                } catch (error) {
                    console.warn("browser notifications:", error);
                    failed = true;
                }
                await refresh();
                if (failed) {
                    render("failed");
                }
            });
        });
        refresh();
        return refresh;
    }

    // --- Article formatting (see the `markdown_toolbar` macro and src/markdown.rs) ---

    /** Marks put around text: [marker, text written when nothing is selected]. */
    const INLINE_FORMATS = {
        bold: ["**", "bold"],
        italic: ["*", "italic"],
        underline: ["++", "underlined"],
        strike: ["~~", "struck"],
        code: ["`", "code"],
    };

    /**
     * Replaces `start`..`end` of the textarea with `text`, then selects
     * `selectStart`..`selectEnd` (offsets in `text`). Through `insertText`
     * when possible, so Ctrl+Z undoes it.
     */
    function replaceText(area, start, end, text, selectStart, selectEnd) {
        area.focus();
        area.setSelectionRange(start, end);
        let inserted = false;
        try {
            inserted = document.execCommand("insertText", false, text);
        } catch {
            // Not supported: replaced directly below.
        }
        if (!inserted || area.value.slice(start, start + text.length) !== text) {
            area.setRangeText(text, start, end, "end");
            area.dispatchEvent(new Event("input", { bubbles: true }));
        }
        area.setSelectionRange(start + selectStart, start + selectEnd);
    }

    /** How many times `letter` repeats going left from `index` (or right with `step` 1). */
    function run(value, index, letter, step) {
        let count = 0;
        let at = step < 0 ? index - 1 : index;
        while (at >= 0 && at < value.length && value[at] === letter) {
            count += 1;
            at += step;
        }
        return count;
    }

    /** Whether runs of `marker`'s letter around text are this marker (`*` vs `**`: 3 is both). */
    function marks(marker, before, after) {
        const [length, both] = [marker.length, marker[0] === "*"];
        const fits = (count) => (both ? count === length || count === 3 : count >= length);
        return fits(before) && fits(after);
    }

    /** Bold, italic...: around the selection, the word under the cursor, or new text. Again to remove. */
    function toggleInline(area, format) {
        const [marker, placeholder] = INLINE_FORMATS[format];
        const value = area.value;
        let [start, end] = [area.selectionStart, area.selectionEnd];
        // Spaces stay outside the marks: `** bold**` isn't bold.
        while (start < end && /\s/.test(value[start])) start += 1;
        while (end > start && /\s/.test(value[end - 1])) end -= 1;
        if (start === end) {
            // The word around the cursor.
            while (start > 0 && /[\p{L}\p{N}_'-]/u.test(value[start - 1])) start -= 1;
            while (end < value.length && /[\p{L}\p{N}_'-]/u.test(value[end])) end += 1;
        }
        const letter = marker[0];
        // Already marked around the selection: removed.
        if (start !== end && marks(marker, run(value, start, letter, -1), run(value, end, letter, 1))) {
            const text = value.slice(start, end);
            replaceText(area, start - marker.length, end + marker.length, text, 0, text.length);
            return;
        }
        // The marks are inside the selection: removed.
        const selected = value.slice(start, end);
        if (selected.length > 2 * marker.length && marks(marker, run(selected, 0, letter, 1), run(selected, selected.length, letter, -1))) {
            const text = selected.slice(marker.length, -marker.length);
            replaceText(area, start, end, text, 0, text.length);
            return;
        }
        // Several lines of code: a block.
        if (format === "code" && selected.includes("\n")) {
            const [from, to] = lineBounds(value, start, end);
            const block = value.slice(from, to);
            const text = `\`\`\`\n${block}\n\`\`\``;
            replaceText(area, from, to, text, 4, 4 + block.length);
            return;
        }
        const text = selected || placeholder;
        replaceText(area, start, end, marker + text + marker, marker.length, marker.length + text.length);
    }

    /** Start and end of the whole lines from `start` to `end`. */
    function lineBounds(value, start, end) {
        const from = value.lastIndexOf("\n", start - 1) + 1;
        let to = value.indexOf("\n", end > start && value[end - 1] === "\n" ? end - 1 : end);
        if (to === -1) to = value.length;
        return [from, to];
    }

    /** Headings, quotes and lists: on each line of the selection. Again to remove. */
    function toggleLines(area, format) {
        const value = area.value;
        const [from, to] = lineBounds(value, area.selectionStart, area.selectionEnd);
        const lines = value.slice(from, to).split("\n");
        const strip = {
            heading: /^#{1,6}\s+/,
            quote: /^>\s?/,
            bullets: /^\s*[-*+]\s+/,
            numbers: /^\s*\d{1,9}[.)]\s+/,
        };
        let changed;
        if (format === "heading") {
            // ## → ### → none: the title is the article's biggest heading.
            const level = (lines[0].match(/^(#{1,6})\s/)?.[1] || "").length;
            const next = level === 0 ? "## " : level === 2 ? "### " : "";
            changed = lines.map((line) => (line.trim() ? next + line.replace(strip.heading, "") : line));
        } else {
            const filled = lines.filter((line) => line.trim());
            const all = filled.length > 0 && filled.every((line) => strip[format].test(line));
            let number = 0;
            changed = lines.map((line) => {
                if (!line.trim()) {
                    return line;
                }
                // Another kind of line becomes this one.
                const bare = Object.values(strip).reduce((text, mark) => text.replace(mark, ""), line);
                if (all) {
                    return bare;
                }
                number += 1;
                return { quote: "> ", bullets: "- ", numbers: `${number}. ` }[format] + bare;
            });
        }
        const text = changed.join("\n");
        // A single line keeps its cursor after the mark; several are selected.
        if (lines.length === 1) {
            const shift = text.length - lines[0].length;
            const cursor = Math.max(0, area.selectionEnd - from + shift);
            replaceText(area, from, to, text, Math.min(cursor, text.length), Math.min(cursor, text.length));
        } else {
            replaceText(area, from, to, text, 0, text.length);
        }
    }

    /** `[words](https://)`, or `![description](https://)`, the part to write next selected. */
    function insertLink(area, picture) {
        const value = area.value;
        const [start, end] = [area.selectionStart, area.selectionEnd];
        const selected = value.slice(start, end).trim();
        const prefix = picture ? "!" : "";
        if (/^https?:\/\/\S+$/i.test(selected)) {
            const words = picture ? "description" : "words";
            replaceText(area, start, end, `${prefix}[${words}](${selected})`, prefix.length + 1, prefix.length + 1 + words.length);
            return;
        }
        const words = selected || (picture ? "description" : "words");
        const text = `${prefix}[${words}](https://)`;
        const address = text.length - "https://)".length;
        replaceText(area, start, end, text, address, address + "https://".length);
    }

    /** `---` on its own line after the cursor's line. */
    function insertLine(area) {
        const value = area.value;
        let at = value.indexOf("\n", area.selectionEnd);
        if (at === -1) at = value.length;
        const before = value.slice(0, at).trim() ? "\n\n" : "";
        const text = `${before}---\n\n`;
        replaceText(area, at, at, text, text.length, text.length);
    }

    function applyFormat(area, format) {
        if (format in INLINE_FORMATS) {
            toggleInline(area, format);
        } else if (format === "link" || format === "picture") {
            insertLink(area, format === "picture");
        } else if (format === "line") {
            insertLine(area);
        } else {
            toggleLines(area, format);
        }
    }

    const PICTURE_TYPES = ["image/jpeg", "image/png", "image/gif", "image/webp"];
    let uploadCount = 0;

    /** Sends one picture to /articles/pictures: its address, or throws the message to show. */
    function sendPicture(file, onProgress) {
        return new Promise((resolve, reject) => {
            const form = new FormData();
            form.append("image", file);
            const request = new XMLHttpRequest();
            request.open("POST", "/articles/pictures");
            request.responseType = "json";
            request.upload.addEventListener("progress", (event) => {
                if (event.lengthComputable) {
                    onProgress(Math.round((event.loaded / event.total) * 100));
                }
            });
            request.addEventListener("load", () => {
                const answer = request.response || {};
                if (request.status === 200 && answer.url) {
                    resolve(answer.url);
                } else {
                    reject(new Error(answer.error || (request.status === 413 ? "This picture is too big: 10 MB at most." : "Failed to upload the picture.")));
                }
            });
            request.addEventListener("error", () => reject(new Error("Lost the connection while uploading the picture.")));
            request.send(form);
        });
    }

    /**
     * Uploads pictures into the article: each gets a line where the cursor
     * is, `![Uploading…]()` until its address is known, then
     * `![description](address)` with "description" selected to write it.
     */
    async function uploadPictures(area, files, status) {
        const pictures = [...files].filter((file) => PICTURE_TYPES.includes(file.type));
        if (pictures.length === 0) {
            status.textContent = t("Only JPEG, PNG, GIF and WebP pictures can be added.");
            return;
        }
        const marks = pictures.map((file) => {
            uploadCount += 1;
            return `![Uploading ${file.name.replace(/[[\]]/g, "")} (${uploadCount})…]()`;
        });
        // On their own lines: a picture is a paragraph.
        const value = area.value;
        const at = area.selectionEnd;
        const before = at > 0 && value[at - 1] !== "\n" ? "\n\n" : "";
        const after = at < value.length && value[at] !== "\n" ? "\n\n" : "";
        const text = before + marks.join("\n\n") + after;
        replaceText(area, area.selectionStart, at, text, text.length, text.length);

        // The first error stays shown, progress goes when all are done.
        let failed = false;
        await Promise.all(pictures.map(async (file, index) => {
            const mark = marks[index];
            let replacement = "";
            try {
                const url = await sendPicture(file, (percent) => {
                    if (!failed) {
                        status.textContent = tf("Uploading {}: {}%", file.name, percent);
                    }
                });
                replacement = `![description](${url})`;
            } catch (error) {
                if (!failed) {
                    status.textContent = `${file.name}: ${error.message}`;
                }
                failed = true;
            }
            // Found again: the text may have changed meanwhile.
            const start = area.value.indexOf(mark);
            if (start === -1) {
                return;
            }
            const select = replacement ? [2, 2 + "description".length] : [0, 0];
            replaceText(area, start, start + mark.length, replacement, ...select);
        }));
        if (!failed) {
            status.textContent = "";
        }
    }

    /**
     * Each `[data-article-preview]`: the article as it will be shown, made
     * by the server (/articles/preview) a moment after each change.
     */
    function setupArticlePreviews() {
        document.querySelectorAll("[data-article-preview]").forEach((preview) => {
            const area = document.getElementById(preview.dataset.articlePreview);
            const title = document.getElementById(preview.dataset.title);
            if (!area) {
                return;
            }
            const body = preview.querySelector("[data-preview-body]");
            const empty = preview.querySelector("[data-preview-empty]");
            const heading = preview.querySelector("[data-preview-title]");
            preview.hidden = false;
            let timer = null;
            let pending = null;
            let shown = null;
            const render = async () => {
                const text = area.value;
                if (text === shown) {
                    return;
                }
                pending?.abort();
                if (text.trim() === "") {
                    body.innerHTML = "";
                    empty.hidden = false;
                    shown = text;
                    return;
                }
                pending = new AbortController();
                try {
                    // The pages of legal/ have their own, see handlers/moderation.rs.
                    const response = await fetch(preview.dataset.previewUrl || "/articles/preview", {
                        method: "POST",
                        headers: { "Content-Type": "application/json" },
                        body: JSON.stringify({ body: text }),
                        signal: pending.signal,
                    });
                    if (!response.ok) {
                        return;
                    }
                    // Made by src/markdown.rs, which escapes everything.
                    body.innerHTML = (await response.json()).html;
                    shown = text;
                    empty.hidden = text.trim() !== "";
                } catch {
                    // Aborted by a newer change, or offline: the next change retries.
                }
            };
            const showTitle = () => {
                heading.textContent = title?.value.trim() || "";
                heading.hidden = heading.textContent === "";
            };
            area.addEventListener("input", () => {
                clearTimeout(timer);
                timer = setTimeout(render, 300);
            });
            title?.addEventListener("input", showTitle);
            showTitle();
            render();
        });
    }

    /** Shows each `[data-markdown-toolbar]` and connects it to its textarea, with Ctrl+B, I, U and K. */
    function setupMarkdownToolbars() {
        document.querySelectorAll("[data-markdown-toolbar]").forEach((toolbar) => {
            const area = document.getElementById(toolbar.dataset.markdownToolbar);
            if (!area) {
                return;
            }
            toolbar.hidden = false;
            // Uploaded pictures: the button, or pasted or dropped in the text.
            const file = toolbar.querySelector("[data-picture-file]");
            const status = toolbar.querySelector("[data-picture-status]");
            if (file) {
                toolbar.querySelector('[data-format="upload"]').addEventListener("click", () => file.click());
                file.addEventListener("change", () => {
                    uploadPictures(area, file.files, status);
                    file.value = "";
                });
                area.addEventListener("paste", (event) => {
                    const pasted = [...(event.clipboardData?.files || [])];
                    if (pasted.some((item) => item.type.startsWith("image/"))) {
                        event.preventDefault();
                        uploadPictures(area, pasted, status);
                    }
                });
                area.addEventListener("dragover", (event) => {
                    if (event.dataTransfer?.types.includes("Files")) {
                        event.preventDefault();
                    }
                });
                area.addEventListener("drop", (event) => {
                    const dropped = [...(event.dataTransfer?.files || [])];
                    if (dropped.length > 0) {
                        event.preventDefault();
                        uploadPictures(area, dropped, status);
                    }
                });
            }
            // Keeps the selection in the textarea while clicking.
            toolbar.addEventListener("mousedown", (event) => {
                if (event.target.closest("button")) {
                    event.preventDefault();
                }
            });
            toolbar.addEventListener("click", (event) => {
                const button = event.target.closest("[data-format]");
                if (button) {
                    applyFormat(area, button.dataset.format);
                }
            });
            area.addEventListener("keydown", (event) => {
                if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) {
                    return;
                }
                const format = { b: "bold", i: "italic", u: "underline", k: "link" }[event.key.toLowerCase()];
                if (format) {
                    event.preventDefault();
                    applyFormat(area, format);
                }
            });
        });
    }

    /**
     * Notifications page: actions sent in the background, applied at once
     * (the server confirms through the WebSocket), and older notifications
     * loaded when "Older notifications" comes into view.
     */
    function setupNotificationsPage() {
        const page = document.querySelector("[data-notifications-page]");
        if (!page) {
            return;
        }

        page.addEventListener("submit", async (event) => {
            const form = event.target.closest("[data-notification-action]");
            if (!form) {
                return;
            }
            event.preventDefault();
            const button = form.querySelector("button");
            button.disabled = true;
            try {
                const response = await fetch(form.action, { method: "POST", credentials: "same-origin" });
                if (!response.ok) {
                    throw new Error(String(response.status));
                }
                // Applied here too, in case the WebSocket is down.
                const action = new URL(form.action).pathname;
                if (action === "/notifications/read-all") {
                    applyNotificationChange({ read: "all", unread: 0 });
                } else if (action === "/notifications/delete-all") {
                    applyNotificationChange({ removed: "all", unread: 0 });
                } else {
                    form.closest("[data-notification-id]")?.remove();
                    refreshEmptyState();
                }
            } catch {
                // The normal way: reloads the page with the result or an error.
                form.submit();
            } finally {
                button.disabled = false;
            }
        });

        // Opening one marks it read at once; the link itself does it on the server.
        page.addEventListener("click", (event) => {
            const item = event.target.closest(".notification.unread");
            if (item && event.target.closest(".notification-link")) {
                markItemRead(item);
            }
        });

        const more = page.querySelector("[data-load-older]");
        if (!more || !("IntersectionObserver" in window)) {
            return;
        }
        let loading = false;
        const observer = new IntersectionObserver(async (entries) => {
            if (loading || !entries.some((entry) => entry.isIntersecting)) {
                return;
            }
            loading = true;
            try {
                const response = await fetch(`/notifications/older?before=${more.dataset.loadOlder}`, {
                    credentials: "same-origin",
                });
                if (!response.ok) {
                    throw new Error(String(response.status));
                }
                const { html, next_before: next } = await response.json();
                const list = notificationList();
                for (const item of parseItems(html)) {
                    // May already be there if it was added live meanwhile.
                    if (!document.getElementById(item.id)) {
                        list.append(item);
                        refreshTimes(item);
                    }
                }
                if (next) {
                    more.dataset.loadOlder = next;
                    more.href = `/notifications?before=${next}`;
                } else {
                    observer.disconnect();
                    more.remove();
                }
                refreshEmptyState();
            } catch {
                // The link stays: clicking it loads the next page.
                observer.disconnect();
            } finally {
                loading = false;
            }
        });
        observer.observe(more);
    }

    /** "Reply" links fill the comment form instead of reloading the page. */
    function setupReplies(root = document) {
        const form = root.querySelector("#comment-form");
        if (!form) {
            return;
        }
        const field = form.querySelector("[data-reply-field]");
        const banner = form.querySelector("[data-reply-banner]");
        const name = form.querySelector("[data-reply-name]");
        const text = form.querySelector("textarea");

        for (const link of root.querySelectorAll("[data-reply-to]")) {
            link.addEventListener("click", (event) => {
                event.preventDefault();
                field.value = link.dataset.replyTo;
                name.textContent = "@" + link.dataset.replyAuthor;
                banner.hidden = false;
                form.scrollIntoView({ behavior: "smooth", block: "center" });
                text.focus({ preventScroll: true });
            });
        }
        form.querySelector("[data-reply-cancel]")?.addEventListener("click", (event) => {
            event.preventDefault();
            field.value = "";
            banner.hidden = true;
            text.focus();
        });
    }

    /** The ☰ button of the top bar on phones. */
    function setupMenu() {
        const button = document.querySelector("[data-menu-toggle]");
        const menu = document.getElementById("site-menu");
        if (!button || !menu) {
            return;
        }
        const root = document.documentElement;
        const isOpen = () => root.classList.contains("menu-open");

        function toggle(open) {
            root.classList.toggle("menu-open", open);
            button.setAttribute("aria-expanded", String(open));
            button.setAttribute("aria-label", open ? "Close the menu" : t("Open the menu"));
            if (open) {
                menu.querySelector("nav a")?.focus();
            }
        }

        button.addEventListener("click", () => toggle(!isOpen()));
        document.querySelector("[data-menu-close]")?.addEventListener("click", () => toggle(false));
        document.addEventListener("keydown", (event) => {
            if (event.key === "Escape" && isOpen()) {
                toggle(false);
                button.focus();
            }
        });
        // Back to a wide screen: the menu is a column again, nothing to close.
        window.matchMedia("(min-width: 50.01em)").addEventListener("change", (event) => {
            if (event.matches) {
                toggle(false);
            }
        });
    }

    /** The count next to "Messenger", hidden at 0. */
    function showUnreadMessages(count) {
        const badge = document.querySelector("[data-unread-messages]");
        if (!badge) {
            return;
        }
        badge.hidden = count === 0;
        badge.firstChild.textContent = count > 99 ? "99+" : String(count);
    }

    /**
     * A conversation changed (`id`): the open conversation gets its new
     * messages, edits and deletions; the list of conversations is reloaded.
     */
    async function refreshConversation(id) {
        const page = document.querySelector(`[data-conversation="${id}"]`);
        if (!page) {
            const list = document.querySelector(".conversation-list");
            if (list) {
                try {
                    const response = await fetch("/messenger", { credentials: "same-origin" });
                    const fresh = new DOMParser().parseFromString(await response.text(), "text/html").querySelector(".conversation-list");
                    if (fresh) {
                        list.replaceWith(fresh);
                        refreshTimes(fresh);
                    }
                } catch {
                    // Up to date at the next change or reload.
                }
            }
            return;
        }
        const messages = page.querySelector("[data-message-list]");
        const shown = [...messages.querySelectorAll("[data-message-id]")];
        const last = shown.length ? shown[shown.length - 1].dataset.messageId : 0;
        let found;
        try {
            const response = await fetch(`/messenger/${id}/after/${last}`, { credentials: "same-origin" });
            if (!response.ok) {
                return;
            }
            found = await response.json();
        } catch {
            return;
        }
        // Read before the changes: at the bottom, the new ones stay in view.
        const scroller = conversationScroller(messages);
        const atBottom = scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 80;
        const kept = new Set(found.ids.map(String));
        for (const item of shown) {
            if (!kept.has(item.dataset.messageId)) {
                item.remove();
            } else if (found.bodies[item.dataset.messageId] !== undefined) {
                // Edited on the other server.
                const body = item.querySelector("[data-message-body]");
                if (body.innerHTML !== found.bodies[item.dataset.messageId]) {
                    body.innerHTML = found.bodies[item.dataset.messageId];
                }
            }
        }
        const template = document.createElement("template");
        template.innerHTML = found.html;
        for (const item of [...template.content.children]) {
            if (!document.getElementById(item.id)) {
                item.classList.add("arriving");
                messages.append(item);
                refreshTimes(item);
            }
        }
        page.querySelector("[data-messages-empty]").hidden = messages.children.length > 0;
        if (atBottom) {
            scroller.scrollTop = scroller.scrollHeight;
        }
    }

    /**
     * What scrolls in a conversation: its message list, which fills the
     * window (see style.css), else the page (browsers without `:has`).
     */
    function conversationScroller(messages) {
        return messages.scrollHeight > messages.clientHeight ? messages : document.scrollingElement;
    }

    /** The conversation page: shows the latest messages; Enter sends. */
    function setupConversation() {
        const page = document.querySelector("[data-conversation]");
        if (!page) {
            return;
        }
        // The latest messages in view.
        const messages = page.querySelector("[data-message-list]");
        const scroller = conversationScroller(messages);
        scroller.scrollTop = scroller.scrollHeight;
        const input = page.querySelector("[data-message-input]");
        input?.addEventListener("keydown", (event) => {
            // Shift+Enter (or a phone's keyboard) makes a new line.
            if (event.key === "Enter" && !event.shiftKey && !event.isComposing && input.value.trim()) {
                event.preventDefault();
                input.form.requestSubmit();
            }
        });
        if (location.hash === "#composer" || location.hash.startsWith("#message-")) {
            input?.focus();
        }
    }

    /** The light / dark button of the menu. */
    function setupTheme() {
        const button = document.querySelector("[data-theme-toggle]");
        if (!button) {
            return;
        }
        const root = document.documentElement;
        const system = window.matchMedia("(prefers-color-scheme: dark)");
        const KEY = "tison-theme";
        const saved = () => {
            try {
                return localStorage.getItem(KEY);
            } catch {
                return null;
            }
        };
        const current = () => root.dataset.theme || (system.matches ? "dark" : "light");

        function show() {
            const dark = current() === "dark";
            // The button names the theme it switches to.
            button.querySelector("[data-theme-label]").textContent = dark ? t("Light mode") : t("Dark mode");
            button.setAttribute("aria-pressed", String(dark));
            button.title = dark ? t("Switch to the light theme") : t("Switch to the dark theme");
        }

        function apply(theme) {
            root.dataset.theme = theme;
            show();
        }

        button.addEventListener("click", () => {
            const theme = current() === "dark" ? "light" : "dark";
            try {
                localStorage.setItem(KEY, theme);
            } catch {
                // Not remembered (private browsing...), applied anyway.
            }
            apply(theme);
        });
        // Other tabs follow, and the system's theme while none is chosen.
        window.addEventListener("storage", (event) => {
            if (event.key === KEY && (event.newValue === "light" || event.newValue === "dark")) {
                apply(event.newValue);
            }
        });
        system.addEventListener("change", () => {
            if (!saved()) {
                show();
            }
        });
        // Without a choice, `data-theme` stays unset: the CSS follows the system.
        show();
        button.hidden = false;
    }

    /** Suggestions under the search bar while typing. */
    function setupSearch() {
        const form = document.querySelector(".site-search");
        const input = form?.querySelector("input[name=q]");
        if (!input) {
            return;
        }
        const list = document.createElement("ul");
        list.className = "search-suggestions";
        list.id = "search-suggestions";
        list.setAttribute("role", "listbox");
        list.hidden = true;
        form.append(list);
        input.setAttribute("autocomplete", "off");
        input.setAttribute("aria-autocomplete", "list");
        input.setAttribute("aria-controls", list.id);
        input.setAttribute("aria-expanded", "false");

        // Only the answer to the latest text is shown.
        let asked = 0;
        let timer = null;

        const options = () => [...list.querySelectorAll("a")];
        const close = () => {
            list.hidden = true;
            input.setAttribute("aria-expanded", "false");
        };

        function select(index) {
            const all = options();
            all.forEach((option, position) => option.setAttribute("aria-selected", String(position === index)));
            all[index]?.scrollIntoView({ block: "nearest" });
        }

        function entry(item) {
            const link = document.createElement("a");
            link.href = item.url;
            link.setAttribute("role", "option");
            link.setAttribute("aria-selected", "false");
            if (item.photo) {
                const image = document.createElement("img");
                image.src = item.photo;
                image.alt = "";
                image.className = "nav-avatar";
                link.append(image);
            } else {
                const initial = document.createElement("span");
                initial.className = "nav-avatar placeholder";
                initial.setAttribute("aria-hidden", "true");
                initial.textContent = item.initial;
                link.append(initial);
            }
            const name = document.createElement("strong");
            name.textContent = item.name;
            const detail = document.createElement("span");
            detail.className = "detail";
            detail.textContent = item.detail;
            link.append(name, detail);
            const row = document.createElement("li");
            row.append(link);
            return row;
        }

        function show(text, found) {
            list.replaceChildren();
            for (const [label, items] of [[t("People"), found.users], [t("Communities"), found.communities]]) {
                if (!items.length) {
                    continue;
                }
                const group = document.createElement("li");
                group.className = "group";
                group.setAttribute("role", "presentation");
                group.textContent = label;
                list.append(group, ...items.map(entry));
            }
            if (!list.children.length) {
                close();
                return;
            }
            const all = document.createElement("a");
            all.href = "/search?q=" + encodeURIComponent(text);
            all.className = "all";
            all.setAttribute("role", "option");
            all.textContent = tf("All results for “{}”", text);
            const row = document.createElement("li");
            row.append(all);
            list.append(row);
            list.hidden = false;
            input.setAttribute("aria-expanded", "true");
        }

        async function ask(text) {
            const number = ++asked;
            try {
                const response = await fetch("/search/suggest?q=" + encodeURIComponent(text), {
                    headers: { Accept: "application/json" },
                });
                if (!response.ok) {
                    return;
                }
                const found = await response.json();
                if (number === asked && input.value.trim() === text) {
                    show(text, found);
                }
            } catch {
                // No suggestions: Enter still searches.
            }
        }

        input.addEventListener("input", () => {
            clearTimeout(timer);
            const text = input.value.trim();
            if (text.replace(/^[@!]/, "").length < 3) {
                asked++;
                close();
                return;
            }
            timer = setTimeout(() => ask(text), 150);
        });

        input.addEventListener("keydown", (event) => {
            if (list.hidden) {
                return;
            }
            const all = options();
            const current = all.findIndex((option) => option.getAttribute("aria-selected") === "true");
            if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                event.preventDefault();
                const step = event.key === "ArrowDown" ? 1 : -1;
                select((current + step + all.length) % all.length);
            } else if (event.key === "Enter" && current >= 0) {
                event.preventDefault();
                window.location.href = all[current].href;
            } else if (event.key === "Escape") {
                close();
            }
        });

        // Clicking a suggestion must not blur the field first.
        list.addEventListener("mousedown", (event) => event.preventDefault());
        input.addEventListener("blur", close);
        input.addEventListener("focus", () => {
            if (list.children.length && input.value.trim().length >= 3) {
                list.hidden = false;
                input.setAttribute("aria-expanded", "true");
            }
        });
    }

    /**
     * The dialog asking before a deletion, made once: the question, "Cancel"
     * and the confirming button.
     */
    function confirmDialog() {
        let dialog = document.getElementById("confirm-dialog");
        if (dialog) {
            return dialog;
        }
        dialog = document.createElement("dialog");
        dialog.id = "confirm-dialog";
        dialog.className = "confirm-dialog";
        dialog.setAttribute("aria-labelledby", "confirm-dialog-text");
        dialog.innerHTML = `
            <form method="dialog">
                <p id="confirm-dialog-text" class="confirm-dialog-text"></p>
                <div class="confirm-dialog-actions">
                    <button type="submit" value="cancel" class="button" data-confirm-cancel>Cancel</button>
                    <button type="submit" value="confirm" class="button danger-filled" data-confirm-ok></button>
                </div>
            </form>`;
        // A click on the dimmed page around it cancels.
        dialog.addEventListener("click", (event) => {
            if (event.target === dialog) {
                dialog.close("cancel");
            }
        });
        document.body.append(dialog);
        return dialog;
    }

    /**
     * Forms with `data-confirm="Question?"` ask before deleting something, in
     * a dialog of the site. The confirming button says what the form's button
     * says ("Delete", t("Remove")), or `data-confirm-label`. One listener for the
     * page: forms added later (live messages) ask too.
     */
    function setupConfirmations() {
        if (!("HTMLDialogElement" in window)) {
            // Very old browsers: their own box.
            document.addEventListener("submit", (event) => {
                const form = event.target.closest("form[data-confirm]");
                if (form && !window.confirm(form.dataset.confirm)) {
                    event.preventDefault();
                }
            });
            return;
        }
        document.addEventListener("submit", (event) => {
            const form = event.target.closest("form[data-confirm]");
            if (!form) {
                return;
            }
            // Asked already: sent for real.
            if (form.dataset.confirmed === "true") {
                delete form.dataset.confirmed;
                return;
            }
            event.preventDefault();
            const submitter = event.submitter;
            const dialog = confirmDialog();
            dialog.querySelector(".confirm-dialog-text").textContent = form.dataset.confirm;
            dialog.querySelector("[data-confirm-ok]").textContent =
                form.dataset.confirmLabel || submitter?.textContent.trim() || t("Confirm");
            dialog.returnValue = "";
            dialog.addEventListener(
                "close",
                () => {
                    if (dialog.returnValue === "confirm") {
                        form.dataset.confirmed = "true";
                        form.requestSubmit(submitter && form.contains(submitter) ? submitter : undefined);
                    } else {
                        submitter?.focus();
                    }
                },
                { once: true },
            );
            dialog.showModal();
            // The safe choice first: Enter doesn't delete by accident.
            dialog.querySelector("[data-confirm-cancel]").focus();
        });
    }

    // Photo posts (see macros/post.html): the arrows and dots of the
    // carousel, and double-click to like, like on Pixelfed.
    function setupPhotos(root = document) {
        for (const frame of root.querySelectorAll("[data-carousel]")) {
            const track = frame.querySelector("[data-photo-track]");
            const slides = track.children;
            if (slides.length < 2) {
                continue;
            }
            const previous = frame.querySelector("[data-photo-previous]");
            const next = frame.querySelector("[data-photo-next]");
            const count = frame.querySelector("[data-photo-count]");
            const dots = frame.querySelectorAll(".photo-dots span");
            const current = () => Math.round(track.scrollLeft / Math.max(track.clientWidth, 1));
            const update = () => {
                const index = Math.min(current(), slides.length - 1);
                previous.hidden = index === 0;
                next.hidden = index === slides.length - 1;
                count.textContent = `${index + 1} / ${slides.length}`;
                dots.forEach((dot, at) => dot.classList.toggle("active", at === index));
            };
            const go = (step) => track.scrollTo({ left: (current() + step) * track.clientWidth });
            previous.addEventListener("click", () => go(-1));
            next.addEventListener("click", () => go(1));
            track.addEventListener("scroll", update, { passive: true });
            track.addEventListener("keydown", (event) => {
                if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
                    event.preventDefault();
                    go(event.key === "ArrowLeft" ? -1 : 1);
                }
            });
            update();
        }

        for (const photo of root.querySelectorAll("img[data-like-target]")) {
            photo.addEventListener("dblclick", () => {
                const frame = photo.closest("[data-carousel]");
                const reveal = frame.querySelector(".photo-reveal");
                // Not through the cover of a sensitive photo.
                if (reveal && !reveal.checked) {
                    return;
                }
                const slide = photo.closest(".photo-slide");
                slide.classList.remove("photo-liked");
                void slide.offsetWidth;
                slide.classList.add("photo-liked");
                // Only likes, never takes the like back.
                const form = document.getElementById(photo.dataset.likeTarget);
                const button = form?.querySelector("button[aria-pressed]");
                if (button && button.getAttribute("aria-pressed") === "false") {
                    setTimeout(() => form.requestSubmit(), 350);
                }
            });
        }
    }

    // The photos chosen in the post form: shown, each with its alt text.
    function setupPhotoComposer() {
        for (const form of document.querySelectorAll("[data-photo-composer]")) {
            const input = form.querySelector('input[name="photos"]');
            const previews = form.querySelector("[data-photo-previews]");
            const hint = form.querySelector("[data-photo-hint]");
            const sensitive = form.querySelector("[data-photo-sensitive]");
            if (!input || !previews) {
                continue;
            }
            const max = Number(input.dataset.max) || 4;
            const defaultHint = hint.textContent;
            const urls = [];
            const show = () => {
                urls.splice(0).forEach((url) => URL.revokeObjectURL(url));
                previews.replaceChildren();
                const files = Array.from(input.files);
                if (files.length > max) {
                    hint.textContent = tf("At most {} photos: choose again.", max);
                    hint.classList.add("error");
                    input.value = "";
                    files.length = 0;
                } else {
                    hint.textContent = defaultHint;
                    hint.classList.remove("error");
                }
                files.forEach((file, index) => {
                    const item = document.createElement("li");
                    const image = document.createElement("img");
                    const url = URL.createObjectURL(file);
                    urls.push(url);
                    image.src = url;
                    image.alt = "";
                    const alt = document.createElement("textarea");
                    alt.name = "alt";
                    alt.maxLength = 1500;
                    alt.placeholder = t("Describe it for people who can't see it (alt text)");
                    alt.setAttribute("aria-label", tf("Description of photo {}", index + 1));
                    const remove = document.createElement("button");
                    remove.type = "button";
                    remove.className = "action danger";
                    remove.textContent = t("Remove");
                    remove.addEventListener("click", () => {
                        // Alt texts written for the others are kept.
                        const kept = Array.from(previews.querySelectorAll("textarea")).map((field) => field.value);
                        kept.splice(index, 1);
                        const remaining = new DataTransfer();
                        Array.from(input.files).forEach((other, at) => at !== index && remaining.items.add(other));
                        input.files = remaining.files;
                        show();
                        previews.querySelectorAll("textarea").forEach((field, at) => (field.value = kept[at] || ""));
                    });
                    item.append(image, alt, remove);
                    previews.append(item);
                });
                previews.hidden = files.length === 0;
                sensitive.hidden = files.length === 0;
            };
            input.addEventListener("change", show);
            form.addEventListener("reset", () => setTimeout(show));
        }
    }

    // Live streams (see src/av/live.rs): HLS made of fMP4 pieces, in
    // several qualities. Played through MediaSource wherever it exists, by
    // this script: the quality menu, Auto choosing by the connection's speed
    // and the player's size. Only browsers without it (older iPhones) play
    // the HLS themselves.
    const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

    async function fetchText(url) {
        try {
            const response = await fetch(url, { cache: "no-store" });
            return response.ok ? await response.text() : null;
        } catch {
            return null;
        }
    }

    // The qualities of a master playlist, best first.
    function liveVariants(text, master) {
        const lines = text.split("\n").map((line) => line.trim());
        const variants = [];
        lines.forEach((line, at) => {
            if (!line.startsWith("#EXT-X-STREAM-INF")) {
                return;
            }
            const uri = lines.slice(at + 1).find((next) => next && !next.startsWith("#"));
            const [width, height] = (line.match(/RESOLUTION=(\d+)x(\d+)/) || [0, 0, 0]).slice(1).map(Number);
            variants.push({
                url: new URL(uri, master),
                bandwidth: Number(line.match(/[^-]BANDWIDTH=(\d+)/)?.[1] || 0),
                codecs: line.match(/CODECS="([^"]+)"/)?.[1] || "avc1.64001f,mp4a.40.2",
                width,
                height,
                lines: Math.min(width, height) || height,
            });
        });
        return variants.sort((first, second) => second.lines - first.lines);
    }

    // Starts the video with its sound; a browser refusing it without a click
    // waits for one, on the "Play with sound" button like videos.
    function startLive(video, say) {
        if (!video.paused || video.dataset.userPaused || video.dataset.waitingForClick) {
            return;
        }
        video.play().catch(() => {
            video.dataset.waitingForClick = "true";
            const start = video.parentElement.querySelector("[data-player-start]");
            if (!start) {
                say("Press play to watch.");
                return;
            }
            start.hidden = false;
            start.addEventListener(
                "click",
                (event) => {
                    event.stopPropagation();
                    video.muted = false;
                    video.play().catch(() => {});
                },
                { once: true },
            );
            video.addEventListener("play", () => (start.hidden = true), { once: true });
        });
    }

    // The gear menu of a live stream: Auto, then each quality. `choose` gets
    // "auto" or a number of lines.
    function liveQualityMenu(frame, variants, choose) {
        const control = frame.querySelector("[data-live-quality]");
        if (!control || variants.length < 2) {
            return () => {};
        }
        const button = control.querySelector("[data-quality-button]");
        const menu = control.querySelector("[data-quality-menu]");
        const current = control.querySelector("[data-quality-current]");
        const autoDetail = control.querySelector("[data-quality-auto]");
        for (const variant of variants) {
            const item = document.createElement("button");
            item.type = "button";
            item.setAttribute("role", "menuitemradio");
            item.setAttribute("aria-checked", "false");
            item.dataset.qualityChoice = String(variant.lines);
            item.textContent = `${variant.lines}p`;
            if (variant.lines >= 720) {
                const hd = document.createElement("span");
                hd.className = "quality-detail";
                hd.textContent = "HD";
                item.append(hd);
            }
            menu.append(item);
        }
        const items = [...menu.querySelectorAll("[data-quality-choice]")];
        const open = (shown) => {
            menu.hidden = !shown;
            button.setAttribute("aria-expanded", String(shown));
            control.classList.toggle("open", shown);
            if (shown) {
                (menu.querySelector("[aria-checked=true]") || items[0]).focus();
            }
        };
        button.addEventListener("click", (event) => {
            event.stopPropagation();
            open(menu.hidden);
        });
        for (const item of items) {
            item.addEventListener("click", (event) => {
                event.stopPropagation();
                for (const other of items) {
                    other.setAttribute("aria-checked", String(other === item));
                }
                choose(item.dataset.qualityChoice);
                open(false);
                button.focus();
            });
        }
        menu.addEventListener("keydown", (event) => {
            const at = items.indexOf(document.activeElement);
            if (event.key === "Escape") {
                open(false);
                button.focus();
            } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                event.preventDefault();
                items[(at + (event.key === "ArrowDown" ? 1 : -1) + items.length) % items.length].focus();
            }
        });
        document.addEventListener("click", (event) => {
            if (!menu.hidden && !control.contains(event.target)) {
                open(false);
            }
        });
        control.hidden = false;
        // Shows what plays: "Auto (480p)" or "720p".
        return (choice, playing) => {
            current.textContent = choice === "auto" ? "Auto" : `${playing.lines}p`;
            autoDetail.textContent = choice === "auto" ? ` (${playing.lines}p)` : "";
            button.setAttribute("aria-label", tf("Quality: {}", choice === "auto" ? tf("Auto, {}", `${playing.lines}p`) : `${playing.lines}p`));
        };
    }

    async function playLive(video) {
        const frame = video.closest("[data-player]") || video.parentElement;
        const waiting = frame.querySelector("[data-live-waiting]");
        const say = (text) => {
            if (waiting) {
                waiting.textContent = text;
                waiting.hidden = !text;
            }
        };
        video.addEventListener("playing", () => say(""));
        const master = new URL(video.dataset.livePlayer, window.location.href);
        const Source = window.ManagedMediaSource || window.MediaSource;
        if (!Source) {
            if (video.canPlayType("application/vnd.apple.mpegurl")) {
                video.src = master.href;
                startLive(video, say);
            } else {
                say("This browser can't play live streams.");
            }
            return;
        }
        // ffmpeg writes the master playlist after the first pieces.
        let variants = [];
        for (let tries = 0; !variants.length; tries++) {
            const text = await fetchText(master);
            variants = text ? liveVariants(text, master) : [];
            if (!variants.length) {
                if (tries > 30) {
                    say("The live stream has ended.");
                    return;
                }
                await sleep(2000);
            }
        }
        const typeOf = (variant) => `video/mp4; codecs="${variant.codecs}"`;
        variants = variants.filter((variant) => Source.isTypeSupported(typeOf(variant)));
        if (!variants.length) {
            say("This browser can't play this stream.");
            return;
        }

        // Auto: the best quality the connection carries (75% of the speed
        // measured on the last pieces), not larger than the player shows.
        let speed = 0;
        let choice = "auto";
        try {
            choice = localStorage.getItem("videoQuality") || "auto";
        } catch {
            // Private browsing: Auto.
        }
        const automatic = () => {
            const box = video.getBoundingClientRect();
            const needed = Math.max(box.height, box.width * (9 / 16)) * (window.devicePixelRatio || 1);
            const fitting = variants.filter((variant) => variant.lines <= Math.max(needed * 1.25, 360));
            const pool = fitting.length ? fitting : variants.slice(-1);
            if (!speed) {
                return pool.find((variant) => variant.lines <= 720) || pool[pool.length - 1];
            }
            return pool.find((variant) => variant.bandwidth <= speed * 0.75) || pool[pool.length - 1];
        };
        const wanted = () =>
            choice === "auto" ? automatic() : variants.find((variant) => variant.lines <= Number(choice)) || variants[variants.length - 1];
        let current = wanted();
        const showQuality = liveQualityMenu(frame, variants, (picked) => {
            choice = picked;
            try {
                localStorage.setItem("videoQuality", picked);
            } catch {
                // Private browsing: this page only.
            }
            showQuality(choice, wanted());
        });
        showQuality(choice, current);

        const source = new Source();
        video.disableRemotePlayback = true;
        video.src = URL.createObjectURL(source);
        await new Promise((resolve) => source.addEventListener("sourceopen", resolve, { once: true }));
        const buffer = source.addSourceBuffer(typeOf(current));
        const append = (data) =>
            new Promise((resolve, reject) => {
                buffer.addEventListener("updateend", resolve, { once: true });
                buffer.addEventListener("error", reject, { once: true });
                buffer.appendBuffer(data);
            });
        const fetchData = async (url) => {
            const started = performance.now();
            const response = await fetch(url, { cache: "no-store" });
            if (!response.ok) {
                throw new Error(String(response.status));
            }
            const data = await response.arrayBuffer();
            // Bits a second, smoothed over the last pieces.
            const measured = (data.byteLength * 8) / Math.max((performance.now() - started) / 1000, 0.05);
            speed = speed ? speed * 0.7 + measured * 0.3 : measured;
            return data;
        };
        // Pieces are numbered alike in every quality: switching goes on
        // from the next number.
        const number = (piece) => Number(piece.match(/(\d+)\.m4s/)?.[1] ?? -1);
        let lastNumber = -1;
        let initFor = null;
        try {
            for (;;) {
                const text = await fetchText(current.url);
                if (!text) {
                    break;
                }
                const lines = text.split("\n").map((line) => line.trim());
                const pieces = lines.filter((line) => line && !line.startsWith("#"));
                if (initFor !== current && pieces.length) {
                    if (initFor && buffer.changeType) {
                        buffer.changeType(typeOf(current));
                    }
                    const init = lines.find((line) => line.startsWith("#EXT-X-MAP"))?.match(/URI="([^"]+)"/)?.[1];
                    await append(await fetchData(new URL(init || "init.mp4", current.url)));
                    initFor = current;
                    // Close to live: only the last pieces.
                    if (lastNumber < 0) {
                        lastNumber = number(pieces[Math.max(0, pieces.length - 4)]);
                    }
                }
                for (const piece of pieces) {
                    if (number(piece) <= lastNumber) {
                        continue;
                    }
                    await append(await fetchData(new URL(piece, current.url)));
                    lastNumber = number(piece);
                    // Another quality from the next piece on.
                    const next = wanted();
                    if (next !== current) {
                        current = next;
                        showQuality(choice, current);
                        break;
                    }
                }
                if (buffer.buffered.length) {
                    const start = buffer.buffered.start(0);
                    const end = buffer.buffered.end(buffer.buffered.length - 1);
                    // Behind the stream (a pause, a slow network): back to live.
                    if (video.currentTime < start || end - video.currentTime > 12) {
                        video.currentTime = Math.max(start, end - 3);
                    }
                    startLive(video, say);
                    // Only the last minute is kept in memory.
                    if (video.currentTime - start > 90 && !buffer.updating) {
                        await new Promise((resolve) => {
                            buffer.addEventListener("updateend", resolve, { once: true });
                            buffer.remove(0, video.currentTime - 60);
                        });
                    }
                }
                if (lines.includes("#EXT-X-ENDLIST")) {
                    break;
                }
                if (initFor === current) {
                    await sleep(2000);
                }
            }
        } catch {
            // A piece removed or a network error: the stream likely ended.
        }
        if (source.readyState === "open" && !buffer.updating) {
            source.endOfStream();
        }
        say("The live stream has ended.");
    }

    function setupLivePlayers(root = document) {
        for (const video of root.querySelectorAll("video[data-live-player]")) {
            if (video.dataset.liveReady) {
                continue;
            }
            video.dataset.liveReady = "true";
            video.addEventListener("pause", () => (video.dataset.userPaused = "true"));
            video.addEventListener("play", () => {
                delete video.dataset.userPaused;
                delete video.dataset.waitingForClick;
            });
            playLive(video);
        }
    }

    // A progress ring (macros/upload.html): its circle is 100 long, the
    // stroke shows `percent` of it; `null` spins without a number.
    function setRing(ring, percent) {
        const done = ring.querySelector("[data-ring]");
        const label = ring.querySelector("[data-ring-percent]");
        ring.classList.toggle("spinning", percent === null);
        done.style.strokeDasharray = percent === null ? "25 100" : `${percent} 100`;
        label.textContent = percent === null ? "" : `${percent}%`;
    }

    // The file areas of large uploads: clicked to choose a file, or a file
    // dropped on them; the chosen file's name and size are shown.
    function setupDropZones() {
        for (const zone of document.querySelectorAll("[data-upload-drop]")) {
            const input = zone.querySelector("input[type=file]");
            const name = zone.querySelector("[data-upload-drop-name]");
            const help = zone.querySelector("[data-upload-drop-help]");
            const empty = [name.textContent, help.textContent];
            const show = () => {
                const file = input.files[0];
                zone.classList.toggle("chosen", Boolean(file));
                name.textContent = file ? file.name : empty[0];
                help.textContent = file ? `${(file.size / 1024 / 1024).toFixed(1)} MB · click to choose another` : empty[1];
            };
            input.addEventListener("change", show);
            for (const kind of ["dragenter", "dragover"]) {
                zone.addEventListener(kind, (event) => {
                    event.preventDefault();
                    zone.classList.add("dragging");
                });
            }
            for (const kind of ["dragleave", "drop"]) {
                zone.addEventListener(kind, () => zone.classList.remove("dragging"));
            }
            zone.addEventListener("drop", (event) => {
                event.preventDefault();
                if (event.dataTransfer.files.length) {
                    input.files = event.dataTransfer.files;
                    show();
                }
            });
        }
    }

    // Large uploads (videos, episodes, posts with photos): sent with their
    // progress live (a ring with the percentage on the upload button, or a
    // bar), then the page the server answers is opened. Without JavaScript
    // the form is sent as usual.
    function setupUploadProgress() {
        for (const form of document.querySelectorAll("form[data-upload-progress]")) {
            const button = form.querySelector("[data-upload-button]") || form.querySelector("button[type=submit]");
            const ring = button.querySelector(".progress-ring");
            const label = button.querySelector("[data-upload-label]");
            const bar = form.querySelector("[data-upload-bar]");
            const status = form.querySelector("[data-upload-status]");
            const megabytes = (bytes) => (bytes / 1024 / 1024).toFixed(1);
            const timeLeft = (seconds) =>
                seconds < 60 ? `about ${Math.max(1, Math.round(seconds))} s left` : `about ${Math.round(seconds / 60)} min left`;
            const showing = (percent, text) => {
                if (ring) {
                    setRing(ring, percent);
                } else if (bar) {
                    bar.hidden = false;
                    bar.value = percent ?? 100;
                }
                status.hidden = false;
                status.textContent = text;
            };
            form.addEventListener("submit", (event) => {
                // Without a file to send (a post without photos), the
                // form goes as usual.
                const hasFile = [...form.querySelectorAll("input[type=file]")].some((input) => input.files.length);
                if (!window.FormData || !status || !hasFile) {
                    return;
                }
                event.preventDefault();
                const request = new XMLHttpRequest();
                request.open("POST", form.action);
                const original = label ? label.textContent : "";
                button.disabled = true;
                button.classList.add("uploading");
                if (label) {
                    label.textContent = t("Uploading");
                }
                showing(0, t("Starting the upload…"));
                const started = performance.now();
                request.upload.addEventListener("progress", (progress) => {
                    if (progress.lengthComputable) {
                        const percent = Math.min(100, Math.floor((progress.loaded / progress.total) * 100));
                        const seconds = (performance.now() - started) / 1000;
                        const speed = progress.loaded / Math.max(seconds, 0.001);
                        const left = seconds > 1 && percent < 100 ? ` · ${timeLeft((progress.total - progress.loaded) / speed)}` : "";
                        showing(percent, `${megabytes(progress.loaded)} of ${megabytes(progress.total)} MB${left}`);
                    }
                });
                request.upload.addEventListener("load", () => {
                    if (label) {
                        label.textContent = t("Saving");
                    }
                    showing(null, t("Uploaded, saving…"));
                });
                request.addEventListener("load", () => {
                    window.location.href = request.responseURL || form.action;
                });
                request.addEventListener("error", () => {
                    showing(0, t("The upload failed: check your connection, or the size of the file, and try again."));
                    button.disabled = false;
                    button.classList.remove("uploading");
                    if (label) {
                        label.textContent = original;
                    }
                });
                request.send(new FormData(form));
            });
        }
    }

    // The conversion of a video or episode, live through a WebSocket like
    // the notifications (see src/handlers/video.rs): the ring turns with
    // ffmpeg. On its page, the page reloads with the player when it's
    // done; in the Videos page's list (`data-conversion-list`), its card
    // says so. A connection cut (a proxy, the server restarting) is opened
    // again; leaving the page closes it without a word.
    let leavingPage = false;
    window.addEventListener("pagehide", () => (leavingPage = true));
    window.addEventListener("pageshow", () => (leavingPage = false));

    function setupConversions() {
        for (const box of document.querySelectorAll("[data-conversion]")) {
            const text = box.querySelector("[data-conversion-text]");
            const holder = box.querySelector("[data-conversion-ring]");
            const ring = holder.querySelector(".progress-ring");
            const open = box.querySelector("[data-conversion-open]");
            const inList = "conversionList" in box.dataset;
            const say = (long, short) => (text.textContent = inList ? short : long);
            const scheme = window.location.protocol === "https:" ? "wss:" : "ws:";
            let finished = false;
            let failures = 0;

            const finish = (ready) => {
                finished = true;
                setRing(ring, 100);
                box.classList.add(ready ? "ready" : "failed");
                if (!inList) {
                    say(ready ? "Ready, opening it…" : t("The conversion failed."), "");
                    setTimeout(() => window.location.reload(), 600);
                } else {
                    say("", ready ? "Ready to watch" : t("The conversion failed"));
                    if (open && ready) {
                        open.textContent = t("Watch");
                        open.href += "?play";
                    }
                }
            };

            const connect = () => {
                const socket = new WebSocket(`${scheme}//${window.location.host}${box.dataset.conversion}`);
                socket.addEventListener("message", (event) => {
                    let progress;
                    try {
                        progress = JSON.parse(event.data);
                    } catch {
                        return;
                    }
                    failures = 0;
                    holder.hidden = false;
                    if (progress.state === "waiting") {
                        setRing(ring, null);
                        if (progress.own) {
                            say("Waiting for your other upload to finish converting…", t("Waiting for your other upload…"));
                        } else {
                            say("Waiting for a free converter: other uploads are converting…", t("Waiting for a free converter…"));
                        }
                    } else if (progress.state === "converting") {
                        setRing(ring, progress.percent);
                        say("Converting with ffmpeg in every quality…", tf("Converting… {}%", progress.percent));
                    } else {
                        finish(progress.state === "ready");
                    }
                });
                socket.addEventListener("close", () => {
                    if (finished || leavingPage) {
                        return;
                    }
                    // Opened again, a little later each time; the ring and
                    // text keep what they showed meanwhile.
                    failures += 1;
                    if (failures > 12) {
                        say("The server can't be reached for now. Reload the page later to check the conversion.", t("Server unreachable"));
                        return;
                    }
                    setTimeout(connect, Math.min(30, 2 ** Math.min(failures, 5)) * 1000);
                });
            };
            connect();
        }
    }

    // Events (macros/post.html): their times are written in UTC and shown in
    // the viewer's time zone, like Mobilizon does.
    const eventTimeFormat = new Intl.DateTimeFormat(language, {
        weekday: "short",
        day: "numeric",
        month: "short",
        year: "numeric",
        hour: "2-digit",
        minute: "2-digit",
        timeZoneName: "short",
    });
    const eventHourFormat = new Intl.DateTimeFormat(language, { hour: "2-digit", minute: "2-digit", timeZoneName: "short" });
    const eventMonthFormat = new Intl.DateTimeFormat(language, { month: "short" });
    const viewerZone = (() => {
        try {
            return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
        } catch {
            return "UTC";
        }
    })();

    function setupEventTimes(root = document) {
        const sameDay = (first, second) => first.toDateString() === second.toDateString();
        for (const time of root.querySelectorAll("time[data-event-time]")) {
            const date = new Date(time.getAttribute("datetime"));
            if (Number.isNaN(date.getTime())) {
                continue;
            }
            // An end on the day it starts: only its hour.
            const start = time.parentElement.querySelector("time[data-event-time]:not([data-event-end])");
            const startDate = start && start !== time ? new Date(start.getAttribute("datetime")) : null;
            time.textContent = "eventEnd" in time.dataset && startDate && sameDay(startDate, date) ? eventHourFormat.format(date) : eventTimeFormat.format(date);
        }
        for (const block of root.querySelectorAll("[data-event-date]")) {
            const date = new Date(block.dataset.eventDate);
            if (!Number.isNaN(date.getTime())) {
                block.querySelector(".event-date-month").textContent = eventMonthFormat.format(date).replace(".", "").toUpperCase();
                block.querySelector(".event-date-day").textContent = String(date.getDate());
            }
        }
        for (const zone of root.querySelectorAll("[data-event-zone]")) {
            const organizer = zone.dataset.eventZone;
            zone.textContent = organizer && organizer !== viewerZone && organizer !== "UTC" ? `· your time (organizer in ${organizer.replace(/_/g, " ")})` : "· your time";
        }
    }

    // The event form: `datetime-local` fields are in the writer's time; their
    // offset from UTC and time zone go with them. An edited event's times,
    // given in UTC, are first turned into the writer's.
    function setupEventForms() {
        const pad = (number) => String(number).padStart(2, "0");
        const local = (date) => `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
        for (const form of document.querySelectorAll("form[data-event-form]")) {
            for (const input of form.querySelectorAll("input[data-utc-time]")) {
                if (input.value) {
                    const date = new Date(`${input.value}:00Z`);
                    if (!Number.isNaN(date.getTime())) {
                        input.value = local(date);
                    }
                }
            }
            form.querySelector("[data-timezone]").value = viewerZone;
            const help = form.querySelector("[data-timezone-help]");
            if (help) {
                help.textContent = tf("Times are in your time zone ({}); everyone sees them in theirs.", viewerZone.replace(/_/g, " "));
            }
            form.addEventListener("submit", () => {
                for (const offset of form.querySelectorAll("[data-offset-for]")) {
                    const input = form.querySelector(`#${offset.dataset.offsetFor}`);
                    const date = input && input.value ? new Date(input.value) : null;
                    // Minutes east of UTC on that day (summer time included).
                    offset.value = date && !Number.isNaN(date.getTime()) ? String(-date.getTimezoneOffset()) : "0";
                }
            });
        }
    }

    // The video player (macros/post.html): the browser's controls are
    // replaced by ours at the bottom, like PeerTube's. Play, the position,
    // the time, sound and level, quality (`setupVideoQualities`) and full
    // screen; a big play button in the middle while paused. The controls
    // hide while playing and come back when the mouse moves.
    // --- Charts of the creator dashboard (see templates/macros/dashboard.html) ---

    const SVG_NS = "http://www.w3.org/2000/svg";

    function svgNode(name, attributes) {
        const node = document.createElementNS(SVG_NS, name);
        for (const [key, value] of Object.entries(attributes)) {
            node.setAttribute(key, value);
        }
        return node;
    }

    /** A round top for the y axis and its 3 or 4 steps; counts are whole numbers. */
    function niceScale(max) {
        if (max <= 0) {
            return { top: 1, ticks: [0, 1] };
        }
        const rough = max / 3;
        const power = 10 ** Math.floor(Math.log10(rough));
        const step = Math.max(1, [1, 2, 2.5, 5, 10].map((factor) => factor * power).find((candidate) => candidate >= rough));
        const top = step * Math.ceil(max / step);
        const ticks = [];
        for (let tick = 0; tick <= top + step / 2; tick += step) {
            ticks.push(Math.round(tick * 100) / 100);
        }
        return { top, ticks };
    }

    /** 980, 1.2K, 12K, 4.2M. */
    function compactNumber(value) {
        const short = (number, unit) => `${number.toFixed(number < 10 ? 1 : 0).replace(/\.0$/, "")}${unit}`;
        if (value >= 1e6) return short(value / 1e6, "M");
        if (value >= 1e3) return short(value / 1e3, "K");
        return String(value);
    }

    /** "Nov 1", for the UTC day `2026-11-01`. */
    function chartDay(iso, withYear) {
        const options = { month: "short", day: "numeric", timeZone: "UTC" };
        if (withYear) options.year = "numeric";
        return new Date(`${iso}T00:00:00Z`).toLocaleDateString(language, options);
    }

    /**
     * One line chart (`[data-chart]`): drawn at the width it has, again when
     * it changes. Hover, touch or the arrow keys show each day's value in a
     * tooltip under a crosshair; the table below the chart has them all.
     */
    function setupChart(figure) {
        const data = JSON.parse(figure.dataset.chart);
        const points = data.points;
        const plot = figure.querySelector("[data-chart-plot]");
        if (!plot || points.length === 0) {
            return;
        }
        const tooltip = document.createElement("div");
        tooltip.className = "chart-tooltip";
        tooltip.hidden = true;
        const tooltipValue = document.createElement("strong");
        const tooltipDay = document.createElement("span");
        tooltip.append(tooltipValue, tooltipDay);
        let geometry = null;
        let shownWidth = 0;
        let current = null;

        const draw = () => {
            const width = Math.round(plot.clientWidth);
            if (!width || width === shownWidth) {
                return;
            }
            shownWidth = width;
            const height = 180;
            const margin = { top: 14, right: 14, bottom: 26, left: 40 };
            const innerWidth = width - margin.left - margin.right;
            const innerHeight = height - margin.top - margin.bottom;
            const { top, ticks } = niceScale(Math.max(...points.map((point) => point[1])));
            const x = (index) => margin.left + (points.length === 1 ? innerWidth / 2 : (index * innerWidth) / (points.length - 1));
            const y = (value) => margin.top + innerHeight - (value / top) * innerHeight;
            const base = y(0);

            const svg = svgNode("svg", { width, height, viewBox: `0 0 ${width} ${height}`, "aria-hidden": "true", focusable: "false" });
            for (const tick of ticks) {
                svg.append(svgNode("line", { x1: margin.left, x2: width - margin.right, y1: y(tick), y2: y(tick), class: "chart-gridline" }));
                const label = svgNode("text", { x: margin.left - 8, y: y(tick), class: "chart-axis", "text-anchor": "end", "dominant-baseline": "middle" });
                label.textContent = compactNumber(tick);
                svg.append(label);
            }
            const middle = Math.floor((points.length - 1) / 2);
            const dayIndexes = [...new Set([0, middle, points.length - 1])];
            dayIndexes.forEach((index, position) => {
                const anchor = dayIndexes.length === 1 ? "middle" : position === 0 ? "start" : position === dayIndexes.length - 1 ? "end" : "middle";
                const label = svgNode("text", { x: x(index), y: height - 6, class: "chart-axis", "text-anchor": anchor });
                // By month (years): the month and year tell where the line is.
                label.textContent = data.per !== "month" ? chartDay(points[index][0], false)
                    : new Date(`${points[index][0]}T00:00:00Z`).toLocaleDateString(language, { month: "short", year: "numeric", timeZone: "UTC" });
                svg.append(label);
            });
            const path = points.map((point, index) => `${index ? "L" : "M"}${x(index).toFixed(1)},${y(point[1]).toFixed(1)}`).join("");
            svg.append(svgNode("path", { d: `${path}L${x(points.length - 1).toFixed(1)},${base}L${x(0).toFixed(1)},${base}Z`, class: "chart-area" }));
            svg.append(svgNode("path", { d: path, class: "chart-line" }));
            // The end: its value labelled, the one number the line leads to.
            const last = points.length - 1;
            svg.append(svgNode("circle", { cx: x(last), cy: y(points[last][1]), r: 4, class: "chart-dot" }));
            const endY = y(points[last][1]);
            const endLabel = svgNode("text", {
                x: x(last) - 8,
                y: endY - 10 < margin.top ? endY + 18 : endY - 10,
                class: "chart-end-label",
                "text-anchor": "end",
            });
            endLabel.textContent = compactNumber(points[last][1]);
            svg.append(endLabel);
            const crosshair = svgNode("line", { y1: margin.top, y2: base, class: "chart-crosshair", visibility: "hidden" });
            const hoverDot = svgNode("circle", { r: 4, class: "chart-dot", visibility: "hidden" });
            svg.append(crosshair, hoverDot);
            plot.replaceChildren(svg, tooltip);
            geometry = { x, y, crosshair, hoverDot, margin, innerWidth, width };
            if (current !== null) {
                show(current);
            }
        };

        const show = (index) => {
            if (!geometry) return;
            current = Math.max(0, Math.min(points.length - 1, index));
            const [day, value] = points[current];
            const cx = geometry.x(current);
            geometry.crosshair.setAttribute("x1", cx);
            geometry.crosshair.setAttribute("x2", cx);
            geometry.crosshair.setAttribute("visibility", "visible");
            geometry.hoverDot.setAttribute("cx", cx);
            geometry.hoverDot.setAttribute("cy", geometry.y(value));
            geometry.hoverDot.setAttribute("visibility", "visible");
            tooltipValue.textContent = `${value.toLocaleString()} ${data.unit}`;
            tooltipDay.textContent = data.per === "week"
                ? tf("Week of {}", chartDay(day, true))
                : data.per === "month"
                    ? new Date(`${day}T00:00:00Z`).toLocaleDateString(language, { month: "long", year: "numeric", timeZone: "UTC" })
                    : chartDay(day, true);
            tooltip.hidden = false;
            const left = Math.max(0, Math.min(geometry.width - tooltip.offsetWidth, cx - tooltip.offsetWidth / 2));
            tooltip.style.left = `${left}px`;
        };
        const hide = () => {
            current = null;
            tooltip.hidden = true;
            geometry?.crosshair.setAttribute("visibility", "hidden");
            geometry?.hoverDot.setAttribute("visibility", "hidden");
        };
        // The nearest day to the pointer: readers aim at a date, not a 2px line.
        const nearest = (event) => {
            const box = plot.getBoundingClientRect();
            const offset = event.clientX - box.left - geometry.margin.left;
            const ratio = points.length === 1 ? 0 : offset / geometry.innerWidth;
            return Math.round(ratio * (points.length - 1));
        };
        plot.addEventListener("pointermove", (event) => geometry && show(nearest(event)));
        plot.addEventListener("pointerdown", (event) => geometry && show(nearest(event)));
        plot.addEventListener("pointerleave", hide);
        plot.addEventListener("focus", () => show(current ?? points.length - 1));
        plot.addEventListener("blur", hide);
        plot.addEventListener("keydown", (event) => {
            const moves = { ArrowLeft: -1, ArrowRight: 1, Home: -Infinity, End: Infinity };
            if (event.key in moves) {
                event.preventDefault();
                const step = moves[event.key];
                show(Number.isFinite(step) ? (current ?? points.length - 1) + step : step < 0 ? 0 : points.length - 1);
            } else if (event.key === "Escape") {
                hide();
            }
        });
        new ResizeObserver(draw).observe(plot);
        draw();
    }

    /** The language picker of the menu: chosen at once, its button only without JavaScript. */
    function setupLanguagePicker() {
        const form = document.querySelector("[data-language-picker]");
        if (!form) {
            return;
        }
        form.querySelector("[data-language-submit]").hidden = true;
        form.querySelector("select").addEventListener("change", () => form.submit());
    }

    function setupCharts() {
        document.querySelectorAll("[data-chart]").forEach(setupChart);
    }

    /**
     * A view of a video (`data-view` on its frame), like PeerTube's: told to
     * the server once it played 30 seconds, or half of a shorter one.
     * Seeking doesn't count, only time played. Visitors send a random id kept
     * by their browser so a reload isn't a new view (see src/entities/views.rs).
     */
    function countView(frame, video) {
        const address = frame.dataset.view;
        if (!address) {
            return;
        }
        let played = 0;
        let last = null;
        const tick = () => {
            const now = video.currentTime;
            // Small steps only: a jump is a seek, not watching.
            if (last !== null && !video.paused && now > last && now - last < 1.5) {
                played += now - last;
            }
            last = now;
            const needed = Number.isFinite(video.duration) && video.duration > 0 ? Math.min(30, video.duration / 2) : 30;
            if (played >= needed) {
                video.removeEventListener("timeupdate", tick);
                // `randomUUID` only exists on https (and localhost) pages.
                const fresh = () => (crypto.randomUUID
                    ? crypto.randomUUID()
                    : Array.from(crypto.getRandomValues(new Uint8Array(16)), (byte) => byte.toString(16).padStart(2, "0")).join(""));
                let viewer = null;
                try {
                    viewer = localStorage.getItem("tison-viewer");
                    if (!viewer) {
                        viewer = fresh();
                        localStorage.setItem("tison-viewer", viewer);
                    }
                } catch {
                    viewer = fresh();
                }
                fetch(address, {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({ viewer }),
                    keepalive: true,
                }).catch(() => {});
            }
        };
        video.addEventListener("timeupdate", tick);
        video.addEventListener("seeking", () => (last = null));
    }

    function setupPlayers() {
        const clock = (seconds) => {
            const whole = Math.max(0, Math.floor(seconds || 0));
            const hours = Math.floor(whole / 3600);
            const minutes = Math.floor(whole / 60) % 60;
            const rest = String(whole % 60).padStart(2, "0");
            return hours ? `${hours}:${String(minutes).padStart(2, "0")}:${rest}` : `${minutes}:${rest}`;
        };
        const savedLevel = () => {
            try {
                const level = JSON.parse(localStorage.getItem("videoLevel") || "null");
                return level && typeof level.volume === "number" ? level : { volume: 1, muted: false };
            } catch {
                return { volume: 1, muted: false };
            }
        };
        for (const frame of document.querySelectorAll("[data-player]")) {
            const video = frame.querySelector("[data-player-video]");
            const controls = frame.querySelector("[data-player-controls]");
            if (!video || !controls || frame.dataset.playerReady) {
                continue;
            }
            frame.dataset.playerReady = "true";
            countView(frame, video);
            const get = (name) => frame.querySelector(`[data-player-${name}]`);
            const play = get("play");
            const bigPlay = get("big-play");
            const spinner = get("spinner");
            const seek = get("seek");
            const played = get("played");
            const buffered = get("buffered");
            const current = get("current");
            const duration = get("duration");
            const mute = get("mute");
            const level = get("level");
            const fullscreen = frame.querySelector("[data-fullscreen]");
            // A live stream: no position nor time, see `player_chrome`.
            const live = "playerLive" in frame.dataset;

            // Ours instead of the browser's.
            video.controls = false;
            controls.hidden = false;
            bigPlay.hidden = false;
            frame.classList.add("custom-player");
            frame.tabIndex = -1;

            const toggle = () => (video.paused || video.ended ? video.play().catch(() => {}) : video.pause());
            const showState = () => {
                const paused = video.paused || video.ended;
                frame.classList.toggle("playing", !paused);
                play.setAttribute("aria-label", paused ? "Play" : t("Pause"));
                bigPlay.hidden = !paused;
            };
            play.addEventListener("click", toggle);
            bigPlay.addEventListener("click", toggle);
            video.addEventListener("click", toggle);
            video.addEventListener("dblclick", () => fullscreen.click());
            for (const kind of ["play", "pause", "ended"]) {
                video.addEventListener(kind, showState);
            }
            video.addEventListener("waiting", () => (spinner.hidden = false));
            for (const kind of ["playing", "canplay", "pause"]) {
                video.addEventListener(kind, () => (spinner.hidden = true));
            }

            // The position: what played, what loaded, and moving to a time.
            let seeking = false;
            const showTime = () => {
                if (live) {
                    return;
                }
                const total = video.duration;
                if (Number.isFinite(total) && total > 0) {
                    duration.textContent = clock(total);
                    const part = video.currentTime / total;
                    played.style.width = `${part * 100}%`;
                    if (!seeking) {
                        seek.value = Math.round(part * 1000);
                    }
                    seek.setAttribute("aria-valuetext", `${clock(video.currentTime)} of ${clock(total)}`);
                    const ranges = video.buffered;
                    const loaded = ranges.length ? ranges.end(ranges.length - 1) / total : 0;
                    buffered.style.width = `${loaded * 100}%`;
                }
                current.textContent = clock(video.currentTime);
            };
            for (const kind of ["timeupdate", "durationchange", "progress", "loadedmetadata", "seeked"]) {
                video.addEventListener(kind, showTime);
            }
            seek?.addEventListener("input", () => {
                seeking = true;
                if (Number.isFinite(video.duration)) {
                    video.currentTime = (seek.value / 1000) * video.duration;
                    played.style.width = `${seek.value / 10}%`;
                }
            });
            seek?.addEventListener("change", () => (seeking = false));

            // Sound and level, remembered for every video.
            const showLevel = () => {
                const silent = video.muted || video.volume === 0;
                frame.classList.toggle("muted", silent);
                frame.classList.toggle("quiet", !silent && video.volume < 0.5);
                mute.setAttribute("aria-label", silent ? "Sound on" : t("Mute"));
                level.value = silent ? 0 : video.volume;
                level.style.setProperty("--level", `${(silent ? 0 : video.volume) * 100}%`);
            };
            const remember = () => {
                try {
                    localStorage.setItem("videoLevel", JSON.stringify({ volume: video.volume, muted: video.muted }));
                } catch {
                    // Private browsing: this page only.
                }
            };
            const saved = savedLevel();
            video.volume = saved.volume;
            video.muted = saved.muted || video.muted;
            mute.addEventListener("click", () => {
                if (video.muted || video.volume === 0) {
                    video.muted = false;
                    if (video.volume === 0) {
                        video.volume = 0.5;
                    }
                } else {
                    video.muted = true;
                }
                remember();
            });
            level.addEventListener("input", () => {
                video.volume = Number(level.value);
                video.muted = video.volume === 0;
                remember();
            });
            video.addEventListener("volumechange", showLevel);
            showLevel();
            showTime();
            showState();

            // Full screen: the whole player, so its controls stay.
            if (frame.requestFullscreen && document.fullscreenEnabled) {
                fullscreen.addEventListener("click", () => {
                    if (document.fullscreenElement === frame) {
                        document.exitFullscreen().catch(() => {});
                    } else {
                        frame.requestFullscreen().catch(() => {});
                    }
                });
                document.addEventListener("fullscreenchange", () => {
                    const full = document.fullscreenElement === frame;
                    frame.classList.toggle("full", full);
                    fullscreen.setAttribute("aria-label", full ? "Leave full screen" : t("Full screen"));
                });
            } else if (video.webkitEnterFullscreen) {
                // iPhone: only the video itself goes full screen.
                fullscreen.addEventListener("click", () => video.webkitEnterFullscreen());
            } else {
                fullscreen.hidden = true;
            }

            // The controls hide after a while of playing without moving the
            // mouse; touching or moving brings them back.
            let idle = null;
            const wake = () => {
                frame.classList.remove("idle");
                clearTimeout(idle);
                idle = setTimeout(() => {
                    if (!video.paused && !frame.querySelector("[data-quality-menu]:not([hidden])") && !controls.contains(document.activeElement)) {
                        frame.classList.add("idle");
                    }
                }, 2500);
            };
            for (const kind of ["mousemove", "touchstart", "focusin", "keydown"]) {
                frame.addEventListener(kind, wake, { passive: true });
            }
            video.addEventListener("pause", wake);
            video.addEventListener("play", wake);
            frame.addEventListener("mouseleave", () => {
                if (!video.paused) {
                    frame.classList.add("idle");
                }
            });

            // Keys, when the player has the focus: Space or K plays, the
            // arrows move 5 seconds or change the level, M mutes, F goes
            // full screen.
            frame.addEventListener("keydown", (event) => {
                if (event.target.closest("[data-quality-menu]") || event.altKey || event.ctrlKey || event.metaKey) {
                    return;
                }
                const onRange = event.target.type === "range";
                const key = event.key.toLowerCase();
                if (key === " " || key === "k") {
                    if (event.target.tagName === "BUTTON" && key === " ") {
                        return;
                    }
                    event.preventDefault();
                    toggle();
                } else if (key === "m") {
                    mute.click();
                } else if (key === "f") {
                    fullscreen.click();
                } else if (!live && !onRange && (key === "arrowleft" || key === "arrowright")) {
                    event.preventDefault();
                    video.currentTime = Math.max(0, video.currentTime + (key === "arrowleft" ? -5 : 5));
                } else if (!onRange && (key === "arrowup" || key === "arrowdown")) {
                    event.preventDefault();
                    video.muted = false;
                    video.volume = Math.min(1, Math.max(0, video.volume + (key === "arrowup" ? 0.1 : -0.1)));
                    remember();
                }
            });
            // Clicking the picture gives the keys to the player.
            video.addEventListener("pointerdown", () => frame.focus({ preventScroll: true }));

            // Opened from a thumbnail (`?play`): the video starts, with its
            // sound. A browser refusing sound without a click on this page
            // keeps it waiting, with a button to play it with sound.
            const params = new URLSearchParams(window.location.search);
            if (params.has("play") && !document.querySelector("[data-player][data-autoplayed]")) {
                frame.dataset.autoplayed = "true";
                params.delete("play");
                const rest = params.toString();
                history.replaceState(history.state, "", window.location.pathname + (rest ? `?${rest}` : "") + window.location.hash);
                video.play().catch(() => {
                    const start = get("start");
                    start.hidden = false;
                    start.addEventListener("click", (event) => {
                        event.stopPropagation();
                        video.muted = false;
                        video.play().catch(() => {});
                    });
                    // However it starts, the button goes.
                    video.addEventListener("play", () => (start.hidden = true));
                });
            }
        }
    }

    // The quality of videos (see src/av/mod.rs), from the gear button in
    // the player: Auto takes the smallest that fills the player on this
    // screen; a choice is remembered for every video.
    function setupVideoQualities() {
        const remembered = () => {
            try {
                return localStorage.getItem("videoQuality") || "auto";
            } catch {
                return "auto";
            }
        };
        for (const control of document.querySelectorAll("[data-quality]")) {
            const frame = control.closest(".video-frame");
            const video = frame && frame.querySelector("video[data-quality-video]");
            const button = control.querySelector("[data-quality-button]");
            const menu = control.querySelector("[data-quality-menu]");
            if (!video || !button || !menu) {
                continue;
            }
            const choices = [...menu.querySelectorAll("[data-quality-choice]")];
            const files = choices.filter((choice) => choice.dataset.url);
            const current = control.querySelector("[data-quality-current]");
            const autoDetail = control.querySelector("[data-quality-auto]");

            const automatic = () => {
                const ratio = window.devicePixelRatio || 1;
                const box = video.getBoundingClientRect();
                const saveData = navigator.connection && navigator.connection.saveData;
                const allowed = files.filter((file) => !saveData || Number(file.dataset.qualityChoice) <= 360);
                const pool = allowed.length ? allowed : files.slice(-1);
                // Best first: the last one still filling the player.
                const filling = pool.filter((file) => Number(file.dataset.width) >= box.width * ratio || Number(file.dataset.height) >= box.height * ratio);
                return filling[filling.length - 1] || pool[0];
            };
            const play = (file) => {
                if (video.getAttribute("src") === file.dataset.url) {
                    return;
                }
                const time = video.currentTime;
                const playing = !video.paused;
                video.src = file.dataset.url;
                video.addEventListener(
                    "loadedmetadata",
                    () => {
                        video.currentTime = time;
                        if (playing) {
                            video.play().catch(() => {});
                        }
                    },
                    { once: true },
                );
            };
            // `choice` is "auto" or a level like "1080": the remembered one,
            // or the best this video has under it.
            let chosen = "auto";
            const choose = (choice) => {
                chosen = choice;
                const file = choice === "auto" ? null : files.find((file) => Number(file.dataset.qualityChoice) <= Number(choice));
                const playing = file || automatic();
                play(playing);
                for (const item of choices) {
                    item.setAttribute("aria-checked", String(item === (file || choices[0])));
                }
                current.textContent = file ? file.firstChild.textContent : t("Auto");
                autoDetail.textContent = file ? "" : ` (${playing.firstChild.textContent})`;
                button.setAttribute("aria-label", tf("Quality: {}", file ? file.firstChild.textContent : tf("Auto, {}", playing.firstChild.textContent)));
            };

            const open = (shown) => {
                menu.hidden = !shown;
                button.setAttribute("aria-expanded", String(shown));
                control.classList.toggle("open", shown);
                if (shown) {
                    (menu.querySelector("[aria-checked=true]") || choices[0]).focus();
                }
            };
            button.addEventListener("click", (event) => {
                event.stopPropagation();
                open(menu.hidden);
            });
            for (const item of choices) {
                item.addEventListener("click", (event) => {
                    event.stopPropagation();
                    const choice = item.dataset.qualityChoice;
                    try {
                        localStorage.setItem("videoQuality", choice);
                    } catch {
                        // Private browsing: only this video.
                    }
                    choose(choice);
                    open(false);
                    button.focus();
                });
            }
            menu.addEventListener("keydown", (event) => {
                const at = choices.indexOf(document.activeElement);
                if (event.key === "Escape") {
                    open(false);
                    button.focus();
                } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                    event.preventDefault();
                    const step = event.key === "ArrowDown" ? 1 : -1;
                    choices[(at + step + choices.length) % choices.length].focus();
                }
            });
            document.addEventListener("click", (event) => {
                if (!menu.hidden && !control.contains(event.target)) {
                    open(false);
                }
            });
            // A bigger player (full screen): Auto may take a better quality.
            document.addEventListener("fullscreenchange", () => {
                if (chosen === "auto") {
                    choose("auto");
                }
            });
            let resizing = null;
            window.addEventListener("resize", () => {
                clearTimeout(resizing);
                resizing = setTimeout(() => chosen === "auto" && choose("auto"), 400);
            });

            choose(remembered());
        }
    }

    // A chosen thumbnail shown at once, cropped to 16:9 like the server does.
    function setupThumbnailPreviews() {
        for (const input of document.querySelectorAll("input[data-thumbnail-input]")) {
            // In the form (upload) or beside it (edit page).
            const preview = input.form.parentElement.querySelector("[data-thumbnail-preview]");
            if (!preview) {
                continue;
            }
            const original = preview.getAttribute("src");
            input.addEventListener("change", () => {
                const file = input.files[0];
                if (preview.src.startsWith("blob:")) {
                    URL.revokeObjectURL(preview.src);
                }
                if (file) {
                    preview.src = URL.createObjectURL(file);
                } else if (original) {
                    preview.src = original;
                }
                preview.parentElement.hidden = !file && !original;
            });
        }
    }

    // "Copy" next to a value to paste elsewhere (the stream key, the RSS
    // feed), and t("Show") for a hidden one.
    function setupCopyButtons() {
        for (const button of document.querySelectorAll("button[data-copy]")) {
            button.hidden = false;
            button.addEventListener("click", async () => {
                const label = button.textContent;
                try {
                    await navigator.clipboard.writeText(button.dataset.copy);
                    button.textContent = t("Copied");
                    setTimeout(() => (button.textContent = label), 2000);
                } catch {
                    const field = button.parentElement.querySelector("input");
                    if (field) {
                        field.type = "text";
                        field.select();
                    }
                }
            });
        }
        for (const button of document.querySelectorAll("button[data-reveal]")) {
            const field = document.getElementById(button.dataset.reveal);
            if (!field) {
                continue;
            }
            button.hidden = false;
            button.addEventListener("click", () => {
                const shown = field.type === "text";
                field.type = shown ? "password" : "text";
                button.textContent = shown ? "Show" : t("Hide");
            });
        }
    }

    function start() {
        refreshTimes();
        setupConfirmations();
        setupSearch();
        setupTheme();
        setupMenu();
        setupConversation();
        setupShareButtons();
        setupCollapsible();
        setupPhotos();
        setupPhotoComposer();
        setupLivePlayers();
        setupDropZones();
        setupUploadProgress();
        setupConversions();
        setupThumbnailPreviews();
        setupPlayers();
        setupVideoQualities();
        setupEventTimes();
        setupEventForms();
        setupCopyButtons();
        setupReplies();
        setupNotificationsPage();
        setupMarkdownToolbars();
        setupArticlePreviews();
        setupCharts();
        setupLanguagePicker();
        setupPush();
        connectNotifications();
        // Often enough for "x seconds ago" to move, cheap even with many posts.
        setInterval(refreshTimes, 10 * 1000);
    }

    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", start);
    } else {
        start();
    }

    return { formatRelative, formatLocal, refreshTimes, setupShareButtons, setupCollapsible, setupPhotos, setupReplies, setupConfirmations, setupSearch, setupTheme, setupMenu };
})();
