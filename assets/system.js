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
// style.css (`--collapsed-lines`) is cut with a "Show more" button. Without
// JavaScript posts stay complete.
//
// Notifications: the unread count in the menu (`[data-unread]`) is kept up to
// date through a WebSocket to /notifications/live. On the notifications page
// new notifications appear at the top, read and deleted ones update at once,
// actions are sent without reloading, and older ones load when scrolling
// down. Without JavaScript the same links and forms reload the page.
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
            button.textContent = "Link copied";
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
                button.textContent = "Share link";
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
            button.textContent = "Show more";
            button.setAttribute("aria-expanded", "false");
            button.setAttribute("aria-controls", text.id);
            button.addEventListener("click", () => {
                const collapsed = text.classList.toggle("collapsed");
                button.textContent = collapsed ? "Show more" : "Show less";
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
            button.setAttribute("aria-label", open ? "Close the menu" : "Open the menu");
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
            button.querySelector("[data-theme-label]").textContent = dark ? "Light mode" : "Dark mode";
            button.setAttribute("aria-pressed", String(dark));
            button.title = dark ? "Switch to the light theme" : "Switch to the dark theme";
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
            for (const [label, items] of [["People", found.users], ["Communities", found.communities]]) {
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
            all.textContent = `All results for “${text}”`;
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
     * says ("Delete", "Remove"), or `data-confirm-label`. One listener for the
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
                form.dataset.confirmLabel || submitter?.textContent.trim() || "Confirm";
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
                    hint.textContent = `At most ${max} photos: choose again.`;
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
                    alt.placeholder = "Describe it for people who can't see it (alt text)";
                    alt.setAttribute("aria-label", `Description of photo ${index + 1}`);
                    const remove = document.createElement("button");
                    remove.type = "button";
                    remove.className = "action danger";
                    remove.textContent = "Remove";
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
            button.setAttribute("aria-label", `Quality: ${choice === "auto" ? `Auto, ${playing.lines}p` : `${playing.lines}p`}`);
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
                    label.textContent = "Uploading";
                }
                showing(0, "Starting the upload…");
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
                        label.textContent = "Saving";
                    }
                    showing(null, "Uploaded, saving…");
                });
                request.addEventListener("load", () => {
                    window.location.href = request.responseURL || form.action;
                });
                request.addEventListener("error", () => {
                    showing(0, "The upload failed: check your connection, or the size of the file, and try again.");
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
                    say(ready ? "Ready, opening it…" : "The conversion failed.", "");
                    setTimeout(() => window.location.reload(), 600);
                } else {
                    say("", ready ? "Ready to watch" : "The conversion failed");
                    if (open && ready) {
                        open.textContent = "Watch";
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
                            say("Waiting for your other upload to finish converting…", "Waiting for your other upload…");
                        } else {
                            say("Waiting for a free converter: other uploads are converting…", "Waiting for a free converter…");
                        }
                    } else if (progress.state === "converting") {
                        setRing(ring, progress.percent);
                        say("Converting with ffmpeg in every quality…", `Converting… ${progress.percent}%`);
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
                        say("The server can't be reached for now. Reload the page later to check the conversion.", "Server unreachable");
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
                help.textContent = `Times are in your time zone (${viewerZone.replace(/_/g, " ")}); everyone sees them in theirs.`;
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
                play.setAttribute("aria-label", paused ? "Play" : "Pause");
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
                mute.setAttribute("aria-label", silent ? "Sound on" : "Mute");
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
                    fullscreen.setAttribute("aria-label", full ? "Leave full screen" : "Full screen");
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
                current.textContent = file ? file.firstChild.textContent : "Auto";
                autoDetail.textContent = file ? "" : ` (${playing.firstChild.textContent})`;
                button.setAttribute("aria-label", `Quality: ${file ? file.firstChild.textContent : `Auto, ${playing.firstChild.textContent}`}`);
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
    // feed), and "Show" for a hidden one.
    function setupCopyButtons() {
        for (const button of document.querySelectorAll("button[data-copy]")) {
            button.hidden = false;
            button.addEventListener("click", async () => {
                const label = button.textContent;
                try {
                    await navigator.clipboard.writeText(button.dataset.copy);
                    button.textContent = "Copied";
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
                button.textContent = shown ? "Show" : "Hide";
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
