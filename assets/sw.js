// Browser notifications of Tison (see src/push.rs): shows what the server
// pushes, even with no Tison page open, and opens it when clicked. Served at
// /sw.js so it covers the whole site; registered by assets/system.js.

"use strict";

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));

self.addEventListener("push", (event) => {
    let data = {};
    try {
        data = event.data ? event.data.json() : {};
    } catch {
        // Shown with the defaults below.
    }
    event.waitUntil(show(data));
});

async function show(data) {
    const windows = await self.clients.matchAll({ type: "window", includeUncontrolled: true });
    // The page in front already shows it live (the WebSocket of system.js):
    // the notifications, or this conversation.
    if (data.page && windows.some((client) => client.focused && new URL(client.url).pathname === data.page)) {
        return;
    }
    await self.registration.showNotification(data.title || "Tison", {
        body: data.body || "",
        icon: data.icon || "/public/logo.png",
        badge: "/public/logo.png",
        tag: data.tag,
        // A new message replacing the one before still alerts.
        renotify: Boolean(data.tag && data.renotify),
        data: { url: data.url || "/notifications" },
    });
}

self.addEventListener("notificationclick", (event) => {
    event.notification.close();
    const url = new URL(event.notification.data?.url || "/notifications", self.location.origin).href;
    // A new tab: an open one may be playing a video or writing a post.
    event.waitUntil(self.clients.openWindow(url));
});

// The browser renewed the subscription: the server gets the new one.
self.addEventListener("pushsubscriptionchange", (event) => {
    event.waitUntil((async () => {
        const subscription = event.newSubscription
            || await self.registration.pushManager.subscribe(event.oldSubscription.options);
        const send = (url, body) => fetch(url, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(body),
        });
        if (event.oldSubscription) {
            await send("/push/unsubscribe", { endpoint: event.oldSubscription.endpoint });
        }
        await send("/push/subscribe", subscription.toJSON());
    })());
});
