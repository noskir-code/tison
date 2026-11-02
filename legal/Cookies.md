<!--
==========================================================================
 MAINTAINER NOTES — DELETE THIS BLOCK BEFORE PUBLISHING ON YOUR INSTANCE
==========================================================================
Cookie Policy template for a Fediverse (ActivityPub) instance operated from
Québec, Canada. Works with TERMS.md and PRIVACY.md. NOT legal advice.

1. LIST YOUR REAL COOKIES. Cookie names differ between software (Mastodon,
   GoToSocial, Akkoma, Misskey, etc.) and between versions. Before
   publishing, open your instance in a private browser window, log in, and
   check Developer Tools → Application/Storage → Cookies, Local Storage,
   Session Storage, IndexedDB, and Service Workers. Record every entry in
   the tables below. Re-check after each major software upgrade, and after
   adding a CDN or reverse proxy (some add their own cookies, e.g. bot
   protection).

2. WHY THERE IS NO CONSENT BANNER. This template assumes you use ONLY
   strictly necessary cookies and storage:
   - Québec (Law 25, s. 8.1): technologies that identify, locate or
     profile a person must be disclosed and turned off by default.
     Session/security cookies do none of these.
   - EU/UK (ePrivacy Directive art. 5(3) / PECR): strictly necessary
     storage is exempt from consent.
   If you ever add analytics, embeds that set third-party cookies, or a CDN
   that sets tracking cookies, you will need prior opt-in consent and a
   way to withdraw it, and this policy must be rewritten.

3. Provide a French version (Politique relative aux témoins).
==========================================================================
-->

# Cookie Policy — {{INSTANCE_NAME}}

**Effective date:** {{EFFECTIVE_DATE}}
**Version:** {{VERSION}} — [changelog]({{CHANGELOG_URL}})
**Version française :** [Politique relative aux témoins]({{FRENCH_COOKIES_URL}})

> **Plain-language summary:** We use only the cookies and browser storage
> needed to log you in, keep your account secure, and remember your display
> preferences. No advertising, analytics, or tracking cookies, and no
> third-party cookies. That's why there is no cookie banner.

---

## 1. Scope

1.1. This policy explains how {{INSTANCE_NAME}} (`{{DOMAIN}}`), operated by
{{OPERATOR_LEGAL_NAME}}, uses cookies and similar technologies when you visit
the website or use the web interface. It forms part of our
[Privacy Policy]({{PRIVACY_URL}}) and [Terms of Use]({{TERMS_URL}}).

1.2. Third-party apps (mobile or desktop clients) that you use to access your
account are not covered by this policy. They have their own privacy
practices, which you should review.

## 2. What cookies and similar technologies are

2.1. A **cookie** is a small text file that a website stores in your browser
and reads back on later visits.

2.2. **Similar technologies** include browser *local storage*,
*session storage*, *IndexedDB*, and *service workers*, which store data or
code on your device. In this policy, "cookies" includes all of them.

2.3. **First-party** cookies are set by our domain (`{{DOMAIN}}`).
**Third-party** cookies are set by other domains. We use only first-party
cookies.

## 3. Cookies we use

All of the cookies below are **strictly necessary**. The service cannot work
securely without them, so they cannot be turned off in our interface.

### 3.1. Cookies

| Name | Purpose | Duration | Set when |
|---|---|---|---|
| {{SESSION_COOKIE}} (e.g. `_mastodon_session`) | Keeps you logged in during your visit; holds a session identifier and anti-forgery (CSRF) token | End of browser session / {{DURATION}} | You visit the site |
| {{REMEMBER_COOKIE}} (e.g. `remember_user_token`) | Keeps you logged in between visits if you choose "Remember me" | {{DURATION}} | You log in with "Remember me" |
| {{OTHER_COOKIE}} | {{PURPOSE}} | {{DURATION}} | {{TRIGGER}} |

Our cookies are marked `Secure` (sent only over encrypted HTTPS connections),
`HttpOnly` where possible (not readable by scripts), and `SameSite` (not sent
with requests from other websites), to protect against theft and cross-site
attacks.

### 3.2. Browser storage

| Type | What is stored | Purpose | How to remove |
|---|---|---|---|
| Local storage | Interface preferences (e.g. column layout, theme, language, unsent draft, dismissed announcements) | Remembers your display settings on this device | Clear site data in your browser |
| Session storage | Temporary interface state | Keeps the interface working during your visit | Removed when the tab is closed |
| IndexedDB / cache | Cached timelines, emoji, and interface files | Speeds up loading and reduces data use | Clear site data in your browser |
| Service worker | Small script for push notifications and offline loading | Delivers web push notifications you enabled | Disable notifications or clear site data |

### 3.3. Web push notifications

If you enable browser notifications, your browser creates a push subscription
(an endpoint address provided by your browser's push service, such as
Mozilla, Google or Apple) and we store it on our server. Notification content
passes through that push service, encrypted according to the Web Push
standard. You can revoke this at any time in your notification settings or
your browser settings.

## 4. What we do not use

- No advertising or marketing cookies.
- No analytics or audience-measurement tools.
- No social-media tracking pixels, "like" buttons, or fingerprinting.
- No third-party cookies.
- [OPTIONAL] **Embeds:** posts may show previews of links (images, title,
  description). These previews are generated and served by our server, so
  the linked site does not receive your IP address or set cookies.
  {{IF_EMBEDDED_PLAYERS: describe any embedded video/audio players that load
  third-party content and require a click before loading}}.

## 5. Content from other servers

Images, videos and avatars from other Fediverse servers are
{{REMOTE_MEDIA_MODE: "downloaded and served from our own server (proxied), so
other servers do not receive your IP address or set cookies on your device" /
"loaded directly from their original server in some cases, which may receive
your IP address"}}.

## 6. How to manage cookies

6.1. You can view, block or delete cookies in your browser settings. Help for
the main browsers: Firefox, Chrome, Safari, Edge (search "clear cookies" in
your browser's help).

6.2. If you block or delete strictly necessary cookies, you will be logged out
and may not be able to log in or use the web interface. Public pages remain
readable without cookies.

6.3. Logging out deletes your session cookie. You can also end sessions on
other devices at any time under **Settings → Account → Sessions**
{{OR_EQUIVALENT_PATH}}.

## 7. Changes

If we change how we use cookies, we will update this policy and its version
number. If we ever introduce cookies that are not strictly necessary, we
will ask for your consent before setting them, and they will be off by
default.

## 8. Contact

Questions: {{PRIVACY_EMAIL}} (person in charge of the protection of personal
information). See section 12 of the [Privacy Policy]({{PRIVACY_URL}}) for how
to file a complaint.

---

*This document is based on an open template from {{GITHUB_REPO_URL}}, released
under {{TEMPLATE_LICENSE}}.*