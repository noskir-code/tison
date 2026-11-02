<!--
==========================================================================
 MAINTAINER NOTES — DELETE THIS BLOCK BEFORE PUBLISHING ON YOUR INSTANCE
==========================================================================
Privacy Policy template for a Fediverse (ActivityPub) instance operated
from Québec, Canada. It is designed to work with TERMS.md. It is NOT legal
advice: have it reviewed by a lawyer before relying on it.

Replace every {{PLACEHOLDER}}. Delete sections marked [OPTIONAL] that do
not apply, and delete any data category your software does not collect.
The policy must describe what your server ACTUALLY does — audit your
software's settings (log retention, IP retention, translation, link
previews, object storage, email provider) before publishing.

1. WHICH LAWS APPLY
   - Québec: Act respecting the protection of personal information in the
     private sector (CQLR c P-39.1, as amended by Law 25). It applies to
     any "enterprise", i.e. an organized economic activity, commercial or
     NOT. A non-profit instance is very likely covered; a purely personal
     hobby server is a grey zone — complying anyway is the safe choice.
   - Federal: PIPEDA applies to personal information collected in the
     course of "commercial activities", and to cross-border flows. A
     donation-funded, non-commercial instance may fall outside it, but
     the principles here satisfy it if it does apply.
   - EU/UK GDPR [Annex A] if you target or monitor people in the EU/UK.

2. LAW 25 CHECKLIST (things this document alone does NOT do for you)
   [ ] Designate a person in charge of the protection of personal
       information (by default, the person with the highest authority in
       the organization) and publish their title + contact on the site
       (s. 3.1).
   [ ] Adopt and publish internal governance policies (s. 3.2) — retention
       and destruction, roles, complaint handling.
   [ ] Keep a register of confidentiality incidents; notify the CAI and
       affected people when there is a "risk of serious injury" (ss.
       3.5–3.8).
   [ ] Do a privacy impact assessment (PIA) before communicating personal
       information outside Québec (s. 17). Federation sends data worldwide,
       and your host/email provider may be outside Québec — document a PIA
       covering both.
   [ ] Confidentiality by default (s. 9.1): settings must default to the
       highest level of confidentiality. Set opt-IN by default for search
       engine indexing, "discoverability"/trending inclusion, and the
       public display of follows/followers where the software allows it.
   [ ] Any feature that identifies, locates or profiles a person must be
       disclosed and OFF by default (s. 8.1).
   [ ] Answer access/rectification/portability requests within 30 days
       (s. 32).
   [ ] Publish a French version (Charter of the French Language) — the
       policy is shown to the public and forms part of a contract of
       adhesion.
==========================================================================
-->

# Privacy Policy — {{INSTANCE_NAME}}

**Effective date:** {{EFFECTIVE_DATE}}
**Version:** {{VERSION}} — [changelog]({{CHANGELOG_URL}})
**Version française :** [Politique de confidentialité]({{FRENCH_PRIVACY_URL}})

> **Plain-language summary (not a substitute for the full policy):**
> We collect only what we need to run a Fediverse server. We do not sell your
> data, show ads, or use tracking cookies. What you post publicly is sent to
> other servers we don't control. Your direct messages are not end-to-end
> encrypted. You can export, correct, or delete your data at any time.

---

## 1. Who is responsible for your information

1.1. {{INSTANCE_NAME}} (`{{DOMAIN}}`) is operated by
{{OPERATOR_LEGAL_NAME}}, {{OPERATOR_POSTAL_ADDRESS}}, Québec, Canada
("**we**", "**us**").

1.2. **Person in charge of the protection of personal information:**
{{PRIVACY_OFFICER_TITLE}} (for example: "Administrator of the Instance"),
reachable at **{{PRIVACY_EMAIL}}** or by mail at the address above. This
person handles access requests, complaints, and confidentiality incidents.

1.3. This policy explains what personal information we collect when you use
the Instance, why, how long we keep it, who we share it with, and your rights.
It forms part of our [Terms of Use]({{TERMS_URL}}).

## 2. A note about the Fediverse

2.1. The Instance is part of a network of independently operated servers that
exchange content using the ActivityPub protocol. **When we send your
information to another server in accordance with your settings, that server's
operator becomes responsible for it**, under their own policies and the laws
of their jurisdiction. We cannot control or guarantee how they use, protect,
or delete it.

2.2. Your **public profile and public posts are public** by nature: anyone on
the internet, and any server, can see, copy and store them.

## 3. What information we collect

### 3.1. Information you give us

| Information | Required? | Visibility |
|---|---|---|
| Username (handle) | Yes | Public, permanently associated with the Instance |
| Email address | Yes | Private (staff only) |
| Password | Yes | Stored only as a salted cryptographic hash; never readable by staff |
| Reason for joining (if registrations need approval) | {{YES_NO}} | Staff only; deleted after review or within {{SIGNUP_REASON_RETENTION}} |
| Display name, bio, avatar, header, profile fields | No | Public |
| Date of birth / age confirmation | {{YES_NO}} | Staff only; used only to verify minimum age |
| Posts, replies, polls, media and their descriptions | No | According to the visibility you choose (see 3.3) |
| Direct / private messages | No | You, the recipients, and administrators of the servers involved (see 3.4) |
| Follows, followers, blocks, mutes, lists, bookmarks, favourites, filters | No | Follows/followers: public unless hidden in settings. Others: private |
| Reports you file and appeals | No | Staff; optionally forwarded anonymized to the remote server's staff |
| Messages you send to us | No | Staff only |

### 3.2. Information collected automatically

| Information | Purpose | Retention |
|---|---|---|
| IP address of sign-up and logins | Security, abuse and spam prevention, enforcing suspensions | {{IP_RETENTION}} (recommended: 12 months max) |
| IP addresses in web server logs | Security, troubleshooting, protection against attacks | {{SERVER_LOG_RETENTION}} (recommended: 14 days) |
| Browser / app name (user agent) and approximate session time | Lets you see and revoke your active sessions | Until the session is revoked or expires |
| Authorized applications and access tokens | Lets third-party apps access your account at your request | Until you revoke them |
| Account timestamps (creation, last activity) | Account management, inactivity policy | Life of the account |

We do **not** use analytics, tracking pixels, fingerprinting, or advertising
tools. [If you add privacy-respecting analytics, describe them here.]

### 3.3. Visibility of posts

| Visibility | Who receives it |
|---|---|
| Public | Everyone; appears in public timelines and is sent to other servers |
| Unlisted / quiet public | Everyone with the link; not shown in public timelines |
| Followers only | Your followers, including on other servers |
| Direct / mentioned only | Only the accounts mentioned |

Mentioning someone in a post sends it to them even if they do not follow you.

### 3.4. Direct messages

Direct messages are **not end-to-end encrypted**. They are stored on this
server and on the recipients' servers. Our administrators can technically
access them. We access them only when necessary to investigate a report you
or the recipient filed, to comply with a legal obligation, or to protect the
security of the service. Do not use them to share sensitive information.

### 3.5. Information about people on other servers

We receive and store content and profile information from accounts on other
servers when it is sent to us (for example, because a Member here follows
them). We process it only to display it to our Members, to moderate it, and
to operate federation.

## 4. Why we use your information

We use personal information only for the following purposes, and not for any
other purpose without your consent unless the law allows it:

a) **Providing the service:** creating and managing your account,
publishing and delivering your content according to your settings,
showing you content, sending notifications you enabled;

b) **Communicating with you:** confirmation emails, password resets, security
alerts, moderation notices, and notice of changes to our Terms or this
policy. We do not send marketing emails;

c) **Security and moderation:** preventing spam, abuse, unauthorized access
and evasion of suspensions; handling reports and appeals;

d) **Legal compliance:** responding to legal obligations and valid legal
processes (see 6.3), including mandatory reporting of child sexual abuse
material;

e) **Running the server:** backups, troubleshooting, capacity planning
(using aggregate, non-identifying statistics where possible).

**Consent.** By creating an account you consent to the uses described in
a)–e), which are necessary to provide the service. Optional features listed in
section 5 require your separate, opt-in choice. You may withdraw consent at any
time, subject to legal restrictions; withdrawing consent to necessary uses
means closing your account.

**Minors.** We do not knowingly collect personal information from a person
under 14 without the consent of the person having parental authority.

**No automated decisions.** We do not make decisions about you based
exclusively on automated processing. [If you use automated spam filters that
can suspend accounts without human review, state it here and explain how to
request human review — required by Law 25, s. 12.1.]

**No sale, no advertising, no AI training.** We do not sell or rent personal
information, do not use it for advertising, and do not license it for
training artificial intelligence models.

## 5. Optional features (off by default)

In line with confidentiality-by-default, the following are **disabled unless
you turn them on** in your settings:

- Allowing search engines to index your profile;
- Including your account and posts in discovery features (suggestions,
  trending, public full-text search);
- {{OTHER_OPTIONAL_FEATURES}}.

[OPTIONAL] **Translation.** If you use the "translate" button, the text of the
post is sent to {{TRANSLATION_PROVIDER}} ({{PROVIDER_LOCATION}}) to be
translated. Only the post text is sent.

[OPTIONAL] **Link previews and remote media.** When a post contains a link,
our server (not your device) fetches a preview from the linked site. Your IP
address is not shared with that site. Remote media are cached on our server
for {{MEDIA_CACHE_DAYS}} days.

## 6. Who we share information with

### 6.1. Other Fediverse servers

As described in sections 2 and 3.3, your content and public profile are
delivered to other servers in accordance with your visibility settings. Those
servers may be located anywhere in the world (see section 7).

### 6.2. Service providers

We use the following providers, who process personal information only on our
behalf, under written agreements requiring them to protect it and use it only
for the services they provide to us:

| Provider | Service | Information involved | Location |
|---|---|---|---|
| {{HOSTING_PROVIDER}} | Server hosting | All data stored on the Instance | {{LOCATION}} |
| {{EMAIL_PROVIDER}} | Sending emails | Email address, email content | {{LOCATION}} |
| {{STORAGE_PROVIDER}} [OPTIONAL] | Media storage | Uploaded media | {{LOCATION}} |
| {{CDN_PROVIDER}} [OPTIONAL] | Content delivery / DDoS protection | IP address, requests | {{LOCATION}} |
| {{TRANSLATION_PROVIDER}} [OPTIONAL] | Translation | Text of posts you translate | {{LOCATION}} |
| {{BACKUP_PROVIDER}} | Encrypted backups | All data | {{LOCATION}} |

### 6.3. Legal requirements

We may disclose personal information without your consent only where the law
permits or requires it, such as: in response to a valid court order, warrant,
or other legal process; to report child sexual abuse material as required by
federal law; to prevent an act of violence where there is reasonable cause to
believe a serious and imminent risk of death or serious bodily injury threatens
a person or identifiable group. We will review each request, disclose only what
is necessary, and, unless legally prohibited or doing so would increase the
risk, inform you. We will publish aggregate statistics on legal requests at
{{TRANSPARENCY_URL}} [OPTIONAL].

### 6.4. Transfer of operation

If operation of the Instance is transferred to another operator (see Terms,
section 14.4), personal information will be transferred only to an operator
that agrees to protect it under this policy, and you will be notified in
advance.

## 7. Information communicated outside Québec

7.1. Your information may be stored or processed outside Québec, including:
- by our service providers, located in {{PROVIDER_LOCATIONS}};
- by other Fediverse servers, located anywhere in the world, when your
  content is federated to them.

7.2. Before communicating personal information outside Québec, we assessed
the privacy impacts, including the sensitivity of the information, the
purposes, the protection measures, and the legal framework of the
destination. For our service providers, we rely on contractual protections.
For federation, the communication is inherent to the service you chose and
is limited to what your visibility settings allow; you control it through
those settings, and you can avoid it by using followers-only or direct
visibility and by locking your account.

## 8. How long we keep information

| Information | Retention |
|---|---|
| Account data and your content | As long as your account exists, until you delete it |
| Deleted posts and media | Removed from the live database immediately; purged from backups within {{BACKUP_RETENTION_DAYS}} days |
| Deleted account | {{DELETION_DELAY}}, then deleted, except your username, which is kept permanently (without other data) to prevent impersonation |
| IP addresses | See section 3.2 |
| Moderation records (reports, actions, appeals) | {{MODERATION_RETENTION}} (recommended: 2 years after the last action), to handle repeat abuse and appeals |
| Data subject to a legal obligation or a legal hold | As long as required by law |
| Cached content from other servers | {{REMOTE_CACHE_RETENTION}} |

When the purpose is achieved, we destroy the information or anonymize it so
that it can no longer, irreversibly, identify a person.

## 9. Security

9.1. We take reasonable security measures appropriate to the sensitivity of
the information, including: encrypted connections (HTTPS/TLS); hashed
passwords; optional two-factor authentication; restricted administrator access
with strong authentication; regular software updates; {{ENCRYPTED_BACKUPS}};
and access to personal information limited to staff who need it.

9.2. No system is perfectly secure. If a confidentiality incident presents a
risk of serious injury to you, we will notify you and the Commission d'accès à
l'information du Québec (and, if applicable, the Office of the Privacy
Commissioner of Canada) promptly, and record every incident in our incident
register.

## 10. Cookies and local storage

We use only cookies and browser storage that are **strictly necessary** for the
service: keeping you logged in, protecting forms against forgery (CSRF), and
remembering your interface preferences. We do not use advertising or tracking
cookies, and none are placed by third parties.

## 11. Your rights

Subject to the exceptions provided by law, you have the right to:

a) **Be informed** of the personal information we hold about you, its use,
who has access to it, and how long it is kept;

b) **Access** it and obtain a copy;

c) **Rectify** information that is inaccurate, incomplete, or ambiguous;

d) **Portability:** receive the information you provided in a structured,
commonly used technological format, or have it sent to another organization
— the built-in **export** and **account migration** features serve this
purpose;

e) **Withdraw consent** to optional uses;

f) **Request cessation of dissemination or de-indexation** of information
about you, or the re-indexation of a link, where the conditions of the law are
met;

g) **Request deletion** of your information — you can delete your account
yourself in the settings at any time;

h) **File a complaint** (see section 12).

**How to exercise your rights.** Many rights can be exercised directly in your
account settings (edit profile, export, delete account, revoke apps and
sessions). Otherwise, write to {{PRIVACY_EMAIL}}. We may ask you to confirm your
identity, typically by writing from the email address on the account. We will
respond **within 30 days** of receiving the request, free of charge (a
reasonable fee may apply only for transcription, reproduction or transmission
costs, and we will tell you in advance). If we refuse, we will explain why and
how to contest the decision.

**Limits.** We can only act on copies held on this Instance. For content
federated to other servers, we will send deletion requests (see Terms, section
2.4), and you may contact those servers' operators directly.

## 12. Complaints

Please first contact the person in charge of the protection of personal
information at {{PRIVACY_EMAIL}}. If you are not satisfied, you may file a
complaint with:

- **Commission d'accès à l'information du Québec** — https://www.cai.gouv.qc.ca
- **Office of the Privacy Commissioner of Canada** (where federal law applies)
  — https://www.priv.gc.ca
- [Annex A] your local data protection authority if you reside in the EU/UK.

## 13. Changes to this policy

We will notify you of significant changes at least 30 days before they take
effect, by email and an announcement on the Instance. Previous versions are
archived at {{CHANGELOG_URL}}.

---

## [OPTIONAL] Annex A — GDPR / UK GDPR

If you reside in the European Economic Area or the United Kingdom:

**Controller:** see section 1. **EU representative (art. 27), if required:**
{{EU_REPRESENTATIVE}}.

**Legal bases (art. 6):**

| Processing | Legal basis |
|---|---|
| Providing the service and federation (3.1, 3.3, 6.1) | Performance of a contract (art. 6(1)(b)) |
| Security, abuse prevention, IP logs, moderation | Legitimate interests (art. 6(1)(f)) in keeping the service safe |
| Optional features (section 5) | Consent (art. 6(1)(a)), withdrawable at any time |
| Legal requests, mandatory reporting | Legal obligation (art. 6(1)(c)) |

**International transfers:** Canada benefits from an EU adequacy decision for
organizations subject to PIPEDA. For other providers or where adequacy does not
apply, we rely on Standard Contractual Clauses. Federation to servers in other
countries happens at your direction under your visibility settings.

**Additional rights:** in addition to section 11, you have the right to
restrict processing and to object to processing based on legitimate interests
(art. 18, 21). You may lodge a complaint with your supervisory authority.

## [OPTIONAL] Annex B — United States

We do not sell or share personal information as those terms are defined in
California law (CCPA/CPRA), and we do not knowingly collect information from
children under 13 (COPPA).

---

*This document is based on an open template from {{GITHUB_REPO_URL}}, released
under {{TEMPLATE_LICENSE}}.*