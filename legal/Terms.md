<!--
==========================================================================
 MAINTAINER NOTES — DELETE THIS BLOCK BEFORE PUBLISHING ON YOUR INSTANCE
==========================================================================
This template is written for a Fediverse (ActivityPub) instance operated
from Québec, Canada, by an individual or small non-profit. It is NOT legal
advice. Have it reviewed by a lawyer (avocat·e) or notary before relying on it.

Replace every {{PLACEHOLDER}}. Sections marked [OPTIONAL] can be removed.

1. FRENCH VERSION IS MANDATORY IF YOU OPERATE FROM / SERVE QUÉBEC.
   Under s. 55 of the Charter of the French Language (as amended by
   Bill 96, in force since 1 June 2023), these Terms are a contract of
   adhesion. Users are only bound by the English version if a French
   version was made available to them FIRST and they then chose English.
   Ship a TERMS.fr.md (Conditions d'utilisation), link it first on the
   sign-up page, and do not charge for the translation.

2. Clauses deliberately NOT included because they are void or unenforceable
   against Québec consumers:
   - Mandatory arbitration / class-action waivers (Consumer Protection Act,
     CQLR c P-40.1, s. 11.1).
   - Blanket exclusions of liability (CPA s. 10; Civil Code of Québec,
     art. 1474 — no exclusion for bodily/moral injury, intentional or
     gross fault).
   - Forcing Québec consumers to sue elsewhere (C.C.Q. art. 3149).
   - Unilateral changes without notice (CPA s. 11.2 requires advance
     written notice for consumer contracts).

3. Pair this with a separate PRIVACY.md. Québec's Law 25 (Act respecting
   the protection of personal information in the private sector) requires
   a designated person in charge of personal information, a published
   privacy policy, and incident reporting to the CAI. Federal PIPEDA may
   also apply.

4. Watch list (as of Nov 2026): federal Bill C-34 (Safe Social Media Act),
   introduced June 2026, would impose duties on social media services,
   including an under-16 account restriction and mandatory removal of
   CSAM and non-consensual intimate content. It is a bill, not law —
   check its status and whether small instances fall within scope.
==========================================================================
-->

# Terms of Use — {{INSTANCE_NAME}}

**Effective date:** {{EFFECTIVE_DATE}}
**Version:** {{VERSION}} — [changelog]({{CHANGELOG_URL}})
**Version française :** [Conditions d'utilisation]({{FRENCH_TERMS_URL}})

> **Plain-language summary (not a substitute for the full Terms):**
> {{INSTANCE_NAME}} is a server in the Fediverse, a network of independently
> run servers that talk to each other. Your public posts leave this server and
> are copied to others we don't control. Be decent, follow the rules, you keep
> ownership of your content, and you can leave and take your followers with you.

---

## 1. Who we are and what these Terms cover

1.1. {{INSTANCE_NAME}} (the "**Instance**", "**we**", "**us**") is a server
reachable at `{{DOMAIN}}`, operated by {{OPERATOR_LEGAL_NAME}}
({{OPERATOR_TYPE: "an individual" / "a non-profit organization incorporated under …"}}),
{{OPERATOR_POSTAL_ADDRESS}}, Québec, Canada.

1.2. Contact: {{CONTACT_EMAIL}}. Moderation reports: through the "Report"
function in the interface, or {{MODERATION_EMAIL}}.

1.3. These Terms of Use (the "**Terms**") form a contract between you and the
operator. They apply to anyone who creates an account on the Instance
("**you**", a "**Member**"). Sections 6, 8 and 11 also describe how we treat
content coming from other servers.

1.4. These Terms are complemented by the following documents, which are
incorporated by reference:
- **Server Rules / Code of Conduct**: {{RULES_URL}}
- **Privacy Policy**: {{PRIVACY_URL}}
- **Moderation and Federation Policy** [OPTIONAL]: {{MODERATION_POLICY_URL}}

If a document conflicts with these Terms, these Terms prevail, except that the
Privacy Policy prevails on matters concerning personal information.

1.5. **Language.** A French version of these Terms is available at
{{FRENCH_TERMS_URL}} and was offered to you before this version. By accepting
the English version, you confirm that it is your express wish to be bound by
the English version. If the two versions differ, you may rely on whichever is
more favourable to you.

## 2. How the Fediverse works (please read)

2.1. The Instance runs {{SOFTWARE_NAME}} ({{SOFTWARE_URL}}), free and
open-source software, and communicates with other servers using the
ActivityPub protocol and related standards ("**federation**").

2.2. Each server in the Fediverse is run independently by its own
operator, under its own rules and laws. **We do not control other servers**,
and we are not responsible for their content, their moderation, their
security, or how they handle your data.

2.3. When you publish content that is visible beyond this Instance, copies are
sent to, stored on and displayed by other servers, including the servers of
people who follow you or who are mentioned or reply to you.

2.4. **Deletion is best-effort beyond this Instance.** When you edit or delete
content, or delete your account, we send edit/delete notices to the servers
we know received it. We cannot guarantee that other servers will honour those
notices, and copies (including screenshots, archives, search indexes and
backups) may persist.

2.5. **Direct / private messages are not end-to-end encrypted** unless the
software explicitly says otherwise. They are stored on this server and on the
recipients' servers, and can be accessed by administrators of those servers
(see section 8.4). Do not use them for confidential information.

## 3. Eligibility and accounts

3.1. **Minimum age.** You must be at least **{{MINIMUM_AGE}}** years old
(recommended: 16) to create an account. If you are under the age of majority
in your place of residence (18 in Québec), you confirm that a parent or
legal guardian has read and accepted these Terms on your behalf. We do not
knowingly collect personal information from children under 14 without the
consent of a person having parental authority.

3.2. **Registration.** Registrations are {{REGISTRATION_MODE: "open" /
"subject to approval" / "by invitation only"}}. We may refuse any
registration at our discretion, provided we do not do so for a
discriminatory reason prohibited by the Québec Charter of Human Rights and
Freedoms.

3.3. **Accuracy and security.** You must provide a working email address and
keep it up to date. You are responsible for keeping your password and any
application tokens confidential and for activity carried out through your
account, unless the activity results from our fault or a security breach on
our side. We strongly recommend enabling two-factor authentication. Tell us
promptly at {{CONTACT_EMAIL}} if you suspect unauthorized access.

3.4. **Usernames and impersonation.** You may use a pseudonym. You may not
impersonate another person or organization in a way that is misleading.
Parody and fan accounts must be clearly labelled as such.

3.5. **Automated accounts.** Bots are {{BOTS_POLICY: "allowed" / "allowed if
marked as bots in the profile settings and compliant with the Rules" /
"not allowed"}}.

3.6. **Fees.** Use of the Instance is free of charge. Voluntary donations
{{DONATION_URL}} do not create additional rights or a right to any level of
service, and are non-refundable unless the law requires otherwise. [If you
ever sell paid accounts, these Terms must be revised: Québec consumer
protection rules on distance contracts will apply.]

## 4. Your content

4.1. **You keep your rights.** You retain all ownership and intellectual
property rights in the text, images, audio, video and other material you
post ("**Your Content**").

4.2. **Licence you give us.** So that the Instance and federation can work,
you grant the operator a **non-exclusive, worldwide, royalty-free,
non-transferable licence** (with the right to sublicense only to the extent
necessary for federation, as described below) to host, store, reproduce,
cache, format, resize, transcode, display, and transmit Your Content, **solely
for the purpose of operating the Instance and delivering Your Content to the
audience you selected** (for example: public, unlisted, followers-only,
mentioned users). This includes sending Your Content to other servers in
accordance with your visibility settings.

4.3. **Duration of the licence.** The licence ends when you delete the
content or your account, except (a) for copies already delivered to other
servers before deletion (see 2.4), (b) for backups, which are overwritten
within {{BACKUP_RETENTION_DAYS}} days, and (c) where we must keep content to
comply with the law or a legal process.

4.4. **What we will not do.** We will not sell Your Content, use it for
advertising, or license it to third parties for training artificial
intelligence models. [OPTIONAL — delete if not applicable:] We will
also, where technically feasible, signal to automated crawlers (e.g.
`robots.txt` and `ai.txt` directives) that content on this Instance may not be
used for AI training, but we cannot guarantee that third parties will comply.

4.5. **Your responsibility.** You are responsible for Your Content. You
confirm that you have the rights needed to post it and that posting it does
not violate these Terms, the Rules, or anyone else's rights, including
copyright, image rights (C.C.Q. arts. 3, 35–36) and privacy rights.

4.6. **Content from others.** Content from other Members or other servers
belongs to its authors. Boosting/sharing it through the normal functions of
the software is permitted; otherwise, ask the author before reusing it.

4.7. **Content warnings and sensitive media.** You must use content warnings
and mark media as sensitive as required by the Rules.

## 5. Acceptable use

5.1. You must comply with the **Server Rules** ({{RULES_URL}}) and all
applicable laws, including those of Canada and Québec.

5.2. Without limiting the Rules, you must **not** use the Instance to post,
send, link to, or facilitate:

a) **any sexual content involving a minor**, including drawn, animated or
AI-generated depictions (Criminal Code, s. 163.1). This results in
immediate suspension and a report to authorities (see 5.4);

b) intimate images of a person shared without their consent, including
sexualized deepfakes (Criminal Code, s. 162.1);

c) content that incites violence, threats (Criminal Code, s. 264.1), or the
promotion of terrorism or violent extremism;

d) hate propaganda, or content that promotes hatred or discrimination against
people based on a protected ground such as race, colour, ethnic or national
origin, religion, sex, sexual orientation, gender identity or expression,
age, or disability (Criminal Code, ss. 318–319; Québec Charter, s. 10);

e) harassment, bullying, doxxing (publishing someone's personal information
without consent), stalking, or organizing pile-ons;

f) defamation (C.C.Q. art. 1457) or content that violates someone's privacy;

g) material that infringes copyright, trademarks or other intellectual
property rights;

h) spam, unsolicited commercial messages (including messages contrary to
Canada's Anti-Spam Legislation), artificial engagement, or bulk account
creation;

i) malware, phishing, scams, or fraud;

j) undisclosed sponsored content or misleading advertising;

k) {{ADDITIONAL_PROHIBITIONS}}.

5.3. You must **not**:
- interfere with the security or operation of the Instance or other
  servers (e.g. denial-of-service, vulnerability exploitation without
  authorization, circumventing rate limits, suspensions or blocks);
- scrape or bulk-collect content or personal information from the
  Instance by automated means, other than through the standard ActivityPub
  and API interfaces used in accordance with users' visibility settings,
  and never to build profiles of individuals or datasets for AI training
  without consent;
- create another account to evade a suspension.

**Security research:** if you find a vulnerability, please report it
privately to {{SECURITY_EMAIL}} before disclosing it.

5.4. **Mandatory reporting.** Under the *Act respecting the mandatory
reporting of Internet child pornography by persons who provide an Internet
service* (S.C. 2011, c. 4), we are required to report child sexual abuse
material to the designated organization (Cybertip.ca) or the police, and to
preserve related data for the period required by law. We will comply.

## 6. Moderation

6.1. We moderate the Instance to enforce these Terms and the Rules. Moderation
is carried out by {{MODERATORS_DESCRIPTION: e.g. "volunteer moderators
appointed by the operator"}}. We do not pre-screen content and have no general
obligation to monitor it, but we act on reports and on content we become aware
of.

6.2. **Possible actions.** Depending on the severity and recurrence, we may:
issue a warning; mark media as sensitive; add a content warning; remove
content; limit (silence) an account; freeze it; suspend it; or permanently
delete it. Where the law requires, we may also preserve information and
disclose it to authorities (see Privacy Policy).

6.3. **Notice and appeal.** Except where immediate action is necessary
(serious illegal content, spam, security threat, or legal requirement), we
will tell you which action was taken and why, through the interface or by
email. You may appeal **within {{APPEAL_DAYS}} days** using the appeal function
or at {{MODERATION_EMAIL}}. A different moderator will review the appeal
where possible. We aim to answer within {{APPEAL_RESPONSE_DAYS}} days.

6.4. **Remote content and federation.** We may hide, filter, remove from
this Instance, or refuse to receive content or accounts from other servers,
and we may limit or block ("defederate from") entire servers, to protect our
Members or comply with the law. A list of moderated servers is published at
{{BLOCKLIST_URL}} [OPTIONAL: with reasons, except where publishing the reason
would cause harm]. Defederation may sever your follow relationships with
accounts on those servers; this is not a breach of these Terms.

6.5. **Reports.** Members may report content or accounts. When you report
content from another server, you may choose to forward an anonymized copy of
the report to that server's moderators. Do not abuse the reporting system.

## 7. Copyright and intellectual property complaints

7.1. **Canada (notice-and-notice).** If you are a copyright owner and believe
content on the Instance infringes your copyright, you may send a notice that
complies with section 41.26 of the *Copyright Act* (R.S.C. 1985, c. C-42) to
{{COPYRIGHT_EMAIL}}, including: your name and address; the work concerned;
the location (URL) of the alleged infringement; the infringement alleged; and
the date and time of the alleged infringement. Where the Copyright Act
applies, we will forward the notice to the Member concerned as required by
law.

7.2. **Removal.** Independently of 7.1, we may remove content that we
reasonably believe infringes intellectual property rights, in accordance with
section 6 (including notice and appeal).

7.3. [OPTIONAL — United States] If you wish to rely on the U.S. DMCA safe
harbour (17 U.S.C. § 512), you must register a designated agent with the U.S.
Copyright Office and publish a full takedown / counter-notice procedure here.
{{DMCA_AGENT_DETAILS}}

7.4. **Repeat infringers.** We will suspend the accounts of Members who
repeatedly infringe others' intellectual property rights.

## 8. Privacy and personal information

8.1. How we collect, use, disclose and retain personal information is described
in the **Privacy Policy** ({{PRIVACY_URL}}). The person in charge of the
protection of personal information is {{PRIVACY_OFFICER_NAME_OR_TITLE}},
{{PRIVACY_EMAIL}}.

8.2. Your public profile, public posts, and the list of your followers and
follows (unless hidden in your settings) are **public** and federated.

8.3. You have rights of access, rectification, and, where applicable, the
right to have information de-indexed or erased and to data portability,
under Québec's *Act respecting the protection of personal information in the
private sector* and other applicable laws. You can exercise them through your
account settings (export, edit, deletion) or at {{PRIVACY_EMAIL}}. You may
file a complaint with the Commission d'accès à l'information du Québec.

8.4. **Administrator access.** For security, legal compliance, and handling of
reports, administrators may technically access data stored on the Instance,
including direct messages. Such access is limited to what is necessary for
these purposes, is {{LOGGED: "logged" / "not logged"}}, and is never used
for any other purpose.

## 9. Account migration, export and deletion

9.1. **Export.** You may export your data (posts archive, follows, blocks,
mutes, lists, bookmarks) at any time through the settings, subject to
reasonable rate limits.

9.2. **Migration.** You may move your account to another server using the
software's migration function, which redirects your followers. We will not
block a migration unless your account is suspended for a serious violation or
a legal obligation prevents it.

9.3. **Deletion by you.** You may delete your account at any time in the
settings. Deletion is processed {{DELETION_DELAY: e.g. "immediately" /
"after a 30-day grace period during which you may cancel it"}}. Your username
may be reserved permanently to prevent impersonation. Federated copies are
handled as described in 2.4.

9.4. **Termination by us.** We may suspend or terminate your account for a
breach of these Terms or the Rules, following section 6. Before an account
in good standing is permanently deleted for reasons not related to a breach
(for example closure of the Instance or long inactivity), we will give at least
**{{NOTICE_DAYS}} days'** notice by email, so you can export your data or
migrate. **Inactive accounts:** {{INACTIVITY_POLICY: e.g. "accounts with no
login for 24 months may be deleted after notice"}}.

9.5. **Closure of the Instance.** If we decide to shut down the Instance, we
will give at least **{{SHUTDOWN_NOTICE_DAYS}} days'** notice (recommended: 90)
on the Instance and by email, keep exports and migration available during
that period, and, where feasible, send account-deletion notices to federated
servers.

## 10. Availability of the service

10.1. The Instance is run {{"by volunteers" / "on a non-profit basis"}} with
limited resources. We will make reasonable efforts to keep it available and
secure, and to maintain backups, but we do not promise uninterrupted or
error-free service. We may perform maintenance, update the software, or change,
add, or remove features.

10.2. **Security incidents.** If a confidentiality incident involving your
personal information presents a risk of serious injury, we will notify you
and the Commission d'accès à l'information as required by law.

## 11. Liability

11.1. **Content of others.** Content posted by other Members or coming from
other servers is the responsibility of its authors. We are not responsible for
it, except to the extent the law makes us responsible once we have actual
knowledge of its unlawful nature and fail to act promptly (see, in Québec, the
*Act to establish a legal framework for information technology*, s. 22).

11.2. **Limitation, to the extent permitted by law.** The service is
provided free of charge. To the extent permitted by applicable law, the
operator is not liable for indirect losses or for losses caused by events
beyond our reasonable control (e.g. outages of hosting providers, actions of
other servers, force majeure).

11.3. **What this section does not limit.** **Nothing in these Terms excludes
or limits our liability** for bodily or moral injury, for injury caused
intentionally or through gross fault, or in any other case where the law does
not permit exclusion or limitation. If you are a consumer within the meaning
of the Québec *Consumer Protection Act* or similar legislation, nothing in
these Terms deprives you of the rights that legislation gives you.

11.4. **Your liability.** You are liable, under the general rules of law, for
damage you cause by violating these Terms or the law. [Consumer-safe: no
broad indemnity clause is imposed on Members.]

## 12. Changes to these Terms

12.1. We may amend these Terms, for example to reflect changes in the law, in
the software, or in our moderation practices. Previous versions are archived
at {{CHANGELOG_URL}}.

12.2. **Advance notice.** For changes other than minor corrections, we will
notify you **at least 30 days** before they take effect, by email and by an
announcement on the Instance. The notice will set out the new or amended
clause, the clause as it read before, the date it takes effect, and your
right to refuse.

12.3. **Your right to refuse.** If you do not accept the changes, you may close
your account (and export or migrate it) at no cost before they take effect.
If you continue to use your account after the effective date, the new Terms
apply.

## 13. Disputes and applicable law

13.1. **Talk to us first.** Most problems can be solved informally: please
write to {{CONTACT_EMAIL}}. This is not a precondition to exercising your
legal rights.

13.2. **Applicable law.** These Terms are governed by the laws in force in
the Province of Québec and the federal laws of Canada applicable there.
If you are a consumer residing elsewhere, this choice of law does not deprive
you of the protection of the mandatory rules of your place of residence.

13.3. **Courts.** Disputes are submitted to the competent courts of the
judicial district of {{JUDICIAL_DISTRICT: e.g. "Montréal"}}, Québec, **without
prejudice to your right as a consumer to bring an action before the courts of
your domicile** (C.C.Q. art. 3149). Claims within the limit of the Small Claims
Division of the Court of Québec may be brought there.

13.4. **No mandatory arbitration and no class-action waiver.** Nothing in
these Terms requires you to submit a dispute to arbitration or restricts your
right to bring legal proceedings or to be a member of a class action.

## 14. General provisions

14.1. **Entire agreement.** These Terms and the documents listed in 1.4 form
the entire agreement between you and the operator about the Instance.

14.2. **Severability.** If any provision is found void or unenforceable, the
rest of these Terms remains in force, and the provision is applied to the
fullest extent permitted by law.

14.3. **No waiver.** Our failure to enforce a provision is not a waiver of our
right to enforce it later.

14.4. **Assignment.** We may transfer the operation of the Instance to
another operator (for example, a non-profit organization) who agrees to be
bound by these Terms and the Privacy Policy; we will notify you at least
30 days beforehand so that you may close or migrate your account if you
disagree.

14.5. **Notices.** We send notices to the email address associated with your
account. You send notices to {{CONTACT_EMAIL}}.

---

## [OPTIONAL] Annex A — Members in the European Union / United Kingdom

If you reside in the EU or UK, the GDPR / UK GDPR may apply to our processing of
your personal information; your rights and our legal bases are described in the
Privacy Policy. Our representative in the EU (GDPR art. 27), if required:
{{EU_REPRESENTATIVE}}. You may lodge a complaint with your local data
protection authority. Mandatory consumer protections of your country of
residence continue to apply. Note: small instances with users in the EU may
also be subject to the Digital Services Act (Regulation (EU) 2022/2065),
including a point of contact and statement-of-reasons obligations for
moderation decisions.

## [OPTIONAL] Annex B — Members in the United States

Members in the U.S. must be at least 13 years old (COPPA) or the minimum age in
section 3.1, whichever is higher. See section 7.3 for copyright notices.

---

*This document is based on an open template from {{GITHUB_REPO_URL}}, released
under {{TEMPLATE_LICENSE: e.g. "CC BY-SA 4.0" / "CC0"}}.*