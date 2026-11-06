<!--
==========================================================================
 NOTES POUR L'ADMINISTRATION — SUPPRIMEZ CE BLOC AVANT DE PUBLIER
==========================================================================
Modèle de politique de confidentialité pour une instance du Fédivers
(ActivityPub) exploitée depuis le Québec, Canada. Il va avec Terms.fr.md.
CE N'EST PAS UN AVIS JURIDIQUE : faites-le réviser par un·e avocat·e avant
de vous y fier.

Remplacez chaque {{PLACEHOLDER}}. Supprimez les sections [FACULTATIF] qui
ne s'appliquent pas et toute catégorie de données que votre logiciel ne
recueille pas. La politique doit décrire ce que votre serveur fait
VRAIMENT : vérifiez les réglages de votre logiciel (conservation des
journaux et des adresses IP, traduction, aperçus de liens, stockage
d'objets, fournisseur de courriel) avant de publier.

1. QUELLES LOIS S'APPLIQUENT
   - Québec : Loi sur la protection des renseignements personnels dans le
     secteur privé (RLRQ c P-39.1, modifiée par la Loi 25). Elle s'applique
     à toute « entreprise », c'est-à-dire une activité économique
     organisée, commerciale ou NON. Une instance sans but lucratif est
     très probablement visée; un serveur purement personnel est une zone
     grise, et s'y conformer quand même est le choix prudent.
   - Fédéral : la LPRPDE s'applique aux renseignements personnels
     recueillis dans le cadre d'« activités commerciales » et aux flux
     transfrontaliers. Une instance non commerciale financée par des dons
     peut y échapper, mais les principes de ce document la respectent si
     elle s'applique.
   - RGPD de l'UE / du R.-U. [annexe A] si vous ciblez ou suivez des
     personnes dans l'UE ou au R.-U.

2. LISTE DE LA LOI 25 (ce que ce document seul NE fait PAS pour vous)
   [ ] Désigner une personne responsable de la protection des
       renseignements personnels (par défaut, la personne ayant la plus
       haute autorité dans l'organisation) et publier son titre et ses
       coordonnées sur le site (art. 3.1).
   [ ] Adopter et publier des politiques de gouvernance (art. 3.2) :
       conservation et destruction, rôles, traitement des plaintes.
   [ ] Tenir un registre des incidents de confidentialité; aviser la CAI
       et les personnes concernées en cas de « risque de préjudice
       sérieux » (art. 3.5 à 3.8).
   [ ] Faire une évaluation des facteurs relatifs à la vie privée (EFVP)
       avant de communiquer des renseignements personnels hors du Québec
       (art. 17). La fédération envoie des données partout dans le monde,
       et votre hébergeur ou fournisseur de courriel peut être hors du
       Québec : documentez une EFVP couvrant les deux.
   [ ] Confidentialité par défaut (art. 9.1) : les paramètres doivent
       offrir par défaut le plus haut niveau de confidentialité. Rendez
       facultatifs (opt-in) par défaut l'indexation par les moteurs de
       recherche, l'inclusion dans la découverte et les tendances, et
       l'affichage public des abonnés et abonnements lorsque le logiciel
       le permet.
   [ ] Toute fonction qui permet d'identifier, de localiser ou de profiler
       une personne doit être divulguée et DÉSACTIVÉE par défaut
       (art. 8.1).
   [ ] Répondre aux demandes d'accès, de rectification et de portabilité
       dans les 30 jours (art. 32).
   [ ] Garder la version anglaise (Privacy Policy) à jour avec celle-ci.
==========================================================================
-->

# Politique de confidentialité — {{INSTANCE_NAME}}

**Date d'entrée en vigueur :** {{EFFECTIVE_DATE}}
**Version :** {{VERSION}} — [historique des modifications]({{CHANGELOG_URL}})
**English version:** [Privacy Policy]({{PRIVACY_URL}}?lang=en) · **Versión en español:** [Política de privacidad]({{PRIVACY_URL}}?lang=es)

> **En bref (ne remplace pas la politique complète) :**
> Nous ne recueillons que ce qu'il faut pour faire fonctionner un serveur du
> Fédivers. Nous ne vendons pas vos données, n'affichons pas de publicité et
> n'utilisons pas de témoins de suivi. Ce que vous publiez publiquement est
> envoyé à d'autres serveurs que nous ne contrôlons pas. Vos messages
> directs ne sont pas chiffrés de bout en bout. Vous pouvez exporter,
> corriger ou supprimer vos données en tout temps.

---

## 1. Qui est responsable de vos renseignements

1.1. {{INSTANCE_NAME}} (`{{DOMAIN}}`) est exploité par
{{OPERATOR_LEGAL_NAME}}, {{OPERATOR_POSTAL_ADDRESS}}, Québec, Canada
(« **nous** »).

1.2. **Personne responsable de la protection des renseignements
personnels :** {{PRIVACY_OFFICER_TITLE}} (par exemple : « Administrateur
de l'Instance »), joignable à **{{PRIVACY_EMAIL}}** ou par la poste à
l'adresse ci-dessus. Cette personne traite les demandes d'accès, les
plaintes et les incidents de confidentialité.

1.3. Cette politique explique quels renseignements personnels nous
recueillons quand vous utilisez l'Instance, pourquoi, combien de temps
nous les conservons, avec qui nous les partageons, et quels sont vos
droits. Elle fait partie de nos [Conditions d'utilisation]({{TERMS_URL}}).

## 2. À propos du Fédivers

2.1. L'Instance fait partie d'un réseau de serveurs exploités de façon
indépendante qui échangent du contenu avec le protocole ActivityPub.
**Quand nous envoyons vos renseignements à un autre serveur selon vos
paramètres, l'exploitant de ce serveur en devient responsable**, selon
ses propres politiques et les lois de son territoire. Nous ne pouvons ni
contrôler ni garantir la façon dont il les utilise, les protège ou les
supprime.

2.2. Votre **profil public et vos publications publiques sont publics**
par nature : n'importe qui sur Internet, et n'importe quel serveur, peut
les voir, les copier et les conserver.

## 3. Les renseignements que nous recueillons

### 3.1. Les renseignements que vous nous donnez

| Renseignement | Obligatoire? | Visibilité |
|---|---|---|
| Nom d'utilisateur (identifiant) | Oui | Public, associé de façon permanente à l'Instance |
| Adresse courriel | Oui | Privée (équipe seulement) |
| Mot de passe | Oui | Conservé seulement sous forme de hachage cryptographique salé; jamais lisible par l'équipe |
| Raison de l'inscription (si les inscriptions sont approuvées) | {{YES_NO}} | Équipe seulement; supprimée après examen ou dans un délai de {{SIGNUP_REASON_RETENTION}} |
| Nom affiché, biographie, avatar, bannière, champs du profil | Non | Public |
| Date de naissance / confirmation de l'âge | {{YES_NO}} | Équipe seulement; sert uniquement à vérifier l'âge minimum |
| Publications, réponses, sondages, médias et leurs descriptions | Non | Selon la visibilité que vous choisissez (voyez 3.3) |
| Messages directs / privés | Non | Vous, les destinataires et les administrateurs des serveurs concernés (voyez 3.4) |
| Abonnements, abonnés, blocages, masquages, listes, signets, favoris, filtres | Non | Abonnements et abonnés : publics sauf si masqués dans les paramètres. Les autres : privés |
| Signalements et appels que vous faites | Non | Équipe; transmis facultativement et de façon anonymisée à l'équipe du serveur distant |
| Messages que vous nous envoyez | Non | Équipe seulement |

### 3.2. Les renseignements recueillis automatiquement

| Renseignement | Utilité | Conservation |
|---|---|---|
| Adresse IP d'inscription et de connexion | Sécurité, prévention des abus et du pourriel, application des suspensions | {{IP_RETENTION}} (recommandé : 12 mois au plus) |
| Adresses IP dans les journaux du serveur Web | Sécurité, dépannage, protection contre les attaques | {{SERVER_LOG_RETENTION}} (recommandé : 14 jours) |
| Nom du navigateur ou de l'application (agent utilisateur) et heure approximative de la session | Vous permet de voir et de révoquer vos sessions actives | Jusqu'à la révocation ou l'expiration de la session |
| Applications autorisées et jetons d'accès | Permettent à des applications tierces d'accéder à votre compte à votre demande | Jusqu'à ce que vous les révoquiez |
| Horodatages du compte (création, dernière activité) | Gestion du compte, politique d'inactivité | Durée de vie du compte |

Nous n'utilisons **pas** d'outils de statistiques, de pixels de suivi,
d'empreinte numérique ni d'outils publicitaires. [Si vous ajoutez des
statistiques respectueuses de la vie privée, décrivez-les ici.]

### 3.3. Visibilité des publications

| Visibilité | Qui la reçoit |
|---|---|
| Publique | Tout le monde; apparaît dans les fils publics et est envoyée aux autres serveurs |
| Non listée / publique discrète | Toute personne qui a le lien; n'apparaît pas dans les fils publics |
| Abonnés seulement | Vos abonnés, y compris sur d'autres serveurs |
| Directe / mentions seulement | Seulement les comptes mentionnés |

Mentionner quelqu'un dans une publication la lui envoie, même s'il ne vous
suit pas.

### 3.4. Messages directs

Les messages directs ne sont **pas chiffrés de bout en bout**. Ils sont
conservés sur ce serveur et sur ceux des destinataires. Nos
administrateurs peuvent techniquement y accéder. Nous n'y accédons que
lorsque c'est nécessaire pour examiner un signalement fait par vous ou par
le destinataire, pour respecter une obligation légale ou pour protéger la
sécurité du service. Un message signalé est montré aux modérateurs avec le
signalement. Ne les utilisez pas pour partager des renseignements
sensibles.

### 3.5. Renseignements sur les personnes d'autres serveurs

Nous recevons et conservons le contenu et les renseignements de profil de
comptes d'autres serveurs quand ils nous sont envoyés (par exemple parce
qu'un membre d'ici les suit). Nous les traitons seulement pour les
afficher à nos membres, les modérer et faire fonctionner la fédération.

## 4. Pourquoi nous utilisons vos renseignements

Nous utilisons les renseignements personnels seulement aux fins suivantes,
et à aucune autre fin sans votre consentement, sauf si la loi le permet :

a) **Fournir le service :** créer et gérer votre compte, publier et
livrer votre contenu selon vos paramètres, vous montrer du contenu, vous
envoyer les notifications que vous avez activées;

b) **Communiquer avec vous :** courriels de confirmation,
réinitialisations de mot de passe, alertes de sécurité, avis de
modération, et avis de changements à nos Conditions ou à cette politique.
Nous n'envoyons pas de courriels de marketing;

c) **Sécurité et modération :** prévenir le pourriel, les abus, les accès
non autorisés et le contournement des suspensions; traiter les
signalements et les appels;

d) **Respect de la loi :** répondre aux obligations légales et aux
procédures judiciaires valides (voyez 6.3), y compris la déclaration
obligatoire du matériel d'abus pédosexuel;

e) **Faire fonctionner le serveur :** sauvegardes, dépannage,
planification de la capacité (avec des statistiques globales et non
identifiantes lorsque c'est possible).

**Consentement.** En créant un compte, vous consentez aux utilisations
décrites en a) à e), qui sont nécessaires pour fournir le service. Les
fonctions facultatives de la section 5 demandent votre choix distinct et
volontaire (opt-in). Vous pouvez retirer votre consentement en tout temps,
sous réserve des restrictions légales; retirer votre consentement aux
utilisations nécessaires revient à fermer votre compte.

**Personnes mineures.** Nous ne recueillons pas sciemment de
renseignements personnels sur une personne de moins de 14 ans sans le
consentement du titulaire de l'autorité parentale.

**Aucune décision automatisée.** Nous ne prenons pas de décision à votre
sujet fondée exclusivement sur un traitement automatisé. [Si vous utilisez
des filtres anti-pourriel automatiques pouvant suspendre des comptes sans
examen humain, indiquez-le ici et expliquez comment demander un examen
humain — exigé par la Loi 25, art. 12.1.]

**Aucune vente, aucune publicité, aucun entraînement d'IA.** Nous ne
vendons ni ne louons de renseignements personnels, ne les utilisons pas à
des fins publicitaires et ne les cédons pas sous licence pour entraîner
des modèles d'intelligence artificielle.

## 5. Fonctions facultatives (désactivées par défaut)

Conformément à la confidentialité par défaut, les fonctions suivantes sont
**désactivées à moins que vous ne les activiez** dans vos paramètres :

- Permettre aux moteurs de recherche d'indexer votre profil;
- Inclure votre compte et vos publications dans les fonctions de
  découverte (suggestions, tendances);
- {{OTHER_OPTIONAL_FEATURES}}.

**Recherche des publications (activée par défaut).** Vos publications
publiques peuvent être trouvées par la recherche de cette Instance et
d'autres serveurs qui respectent le réglage `indexable` (comme Mastodon).
Vous pouvez la désactiver en tout temps dans vos paramètres; vos
publications n'apparaissent alors dans les résultats que pour vous et pour
les personnes qui ont interagi avec elles.

[FACULTATIF] **Traduction.** Si vous utilisez le bouton « traduire », le
texte de la publication est envoyé à {{TRANSLATION_PROVIDER}}
({{PROVIDER_LOCATION}}) pour être traduit. Seul le texte de la publication
est envoyé.

[FACULTATIF] **Aperçus de liens et médias distants.** Quand une
publication contient un lien, notre serveur (et non votre appareil) va
chercher un aperçu sur le site lié. Votre adresse IP n'est pas transmise à
ce site. Les médias distants sont mis en cache sur notre serveur pendant
{{MEDIA_CACHE_DAYS}} jours.

## 6. Avec qui nous partageons des renseignements

### 6.1. Autres serveurs du Fédivers

Comme décrit aux sections 2 et 3.3, votre contenu et votre profil public
sont livrés à d'autres serveurs selon vos paramètres de visibilité. Ces
serveurs peuvent se trouver n'importe où dans le monde (voyez la
section 7). Les publications publiques (pas les non listées, réservées aux
abonnés ou directes) sont aussi envoyées aux relais que l'Instance suit ou
dessert, qui les transmettent à d'autres serveurs (la liste est tenue par
les administrateurs).

### 6.2. Fournisseurs de services

Nous faisons appel aux fournisseurs suivants, qui traitent les
renseignements personnels seulement pour notre compte, en vertu
d'ententes écrites les obligeant à les protéger et à ne les utiliser que
pour les services qu'ils nous rendent :

| Fournisseur | Service | Renseignements concernés | Lieu |
|---|---|---|---|
| {{HOSTING_PROVIDER}} | Hébergement du serveur | Toutes les données conservées sur l'Instance | {{LOCATION}} |
| {{EMAIL_PROVIDER}} | Envoi des courriels | Adresse courriel, contenu des courriels | {{LOCATION}} |
| {{STORAGE_PROVIDER}} [FACULTATIF] | Stockage des médias | Médias téléversés | {{LOCATION}} |
| {{CDN_PROVIDER}} [FACULTATIF] | Diffusion de contenu / protection contre les attaques DDoS | Adresse IP, requêtes | {{LOCATION}} |
| {{TRANSLATION_PROVIDER}} [FACULTATIF] | Traduction | Texte des publications que vous traduisez | {{LOCATION}} |
| {{BACKUP_PROVIDER}} | Sauvegardes chiffrées | Toutes les données | {{LOCATION}} |

### 6.3. Exigences légales

Nous pouvons communiquer des renseignements personnels sans votre
consentement seulement lorsque la loi le permet ou l'exige, par exemple :
en réponse à une ordonnance judiciaire, un mandat ou une autre procédure
judiciaire valide; pour signaler du matériel d'abus pédosexuel comme
l'exige la loi fédérale; pour prévenir un acte de violence lorsqu'il y a un
motif raisonnable de croire qu'un risque sérieux et imminent de mort ou de
blessures graves menace une personne ou un groupe identifiable. Nous
examinerons chaque demande, ne communiquerons que ce qui est nécessaire
et, sauf si la loi l'interdit ou si cela augmentait le risque, vous en
informerons. Nous publierons des statistiques globales sur les demandes
légales à {{TRANSPARENCY_URL}} [FACULTATIF].

### 6.4. Transfert de l'exploitation

Si l'exploitation de l'Instance est transférée à un autre exploitant
(voyez les Conditions, section 14.4), les renseignements personnels ne
seront transférés qu'à un exploitant qui accepte de les protéger selon
cette politique, et vous en serez avisé à l'avance.

## 7. Renseignements communiqués hors du Québec

7.1. Vos renseignements peuvent être conservés ou traités hors du Québec,
notamment :
- par nos fournisseurs de services, situés à {{PROVIDER_LOCATIONS}};
- par d'autres serveurs du Fédivers, situés n'importe où dans le monde,
  quand votre contenu leur est fédéré.

7.2. Avant de communiquer des renseignements personnels hors du Québec,
nous avons évalué les facteurs relatifs à la vie privée, dont la
sensibilité des renseignements, les fins, les mesures de protection et le
cadre juridique de la destination. Pour nos fournisseurs de services, nous
nous appuyons sur des protections contractuelles. Pour la fédération, la
communication est inhérente au service que vous avez choisi et se limite à
ce que vos paramètres de visibilité permettent; vous la contrôlez par ces
paramètres, et vous pouvez l'éviter en utilisant la visibilité « abonnés
seulement » ou directe et en verrouillant votre compte.

## 8. Combien de temps nous conservons les renseignements

| Renseignement | Conservation |
|---|---|
| Données du compte et votre contenu | Tant que votre compte existe, jusqu'à ce que vous le supprimiez |
| Publications et médias que vous supprimez | Retirés immédiatement de la base de données active; purgés des sauvegardes dans un délai de {{BACKUP_RETENTION_DAYS}} jours |
| Publications et commentaires retirés par les modérateurs | Masqués pour tout le monde immédiatement, conservés {{REMOVED_CONTENT_DAYS}} jours pour qu'un appel puisse les rétablir, puis effacés avec leurs médias |
| Compte supprimé | {{DELETION_DELAY}}, puis supprimé, sauf votre nom d'utilisateur, conservé de façon permanente (sans autre donnée) pour empêcher l'usurpation d'identité |
| Adresses IP | Voyez la section 3.2 |
| Dossiers de modération (signalements, mesures, appels) | {{MODERATION_RETENTION}} (recommandé : 2 ans après la dernière mesure), pour traiter les abus répétés et les appels |
| Données visées par une obligation légale ou une suspension légale de destruction | Aussi longtemps que la loi l'exige |
| Contenu mis en cache provenant d'autres serveurs | {{REMOTE_CACHE_RETENTION}} |

Une fois la fin atteinte, nous détruisons les renseignements ou les
anonymisons de façon à ce qu'ils ne permettent plus, de façon
irréversible, d'identifier une personne.

## 9. Sécurité

9.1. Nous prenons des mesures de sécurité raisonnables et adaptées à la
sensibilité des renseignements, notamment : connexions chiffrées
(HTTPS/TLS); mots de passe hachés; authentification à deux facteurs
facultative; accès administrateur restreint avec authentification forte;
mises à jour régulières du logiciel; {{ENCRYPTED_BACKUPS}}; et accès aux
renseignements personnels limité aux membres de l'équipe qui en ont
besoin.

9.2. Aucun système n'est parfaitement sûr. Si un incident de
confidentialité présente un risque de préjudice sérieux pour vous, nous
vous aviserons sans tarder, ainsi que la Commission d'accès à
l'information du Québec (et, s'il y a lieu, le Commissariat à la
protection de la vie privée du Canada), et nous inscrirons chaque incident
dans notre registre des incidents.

## 10. Témoins et stockage local

Nous n'utilisons que des témoins et du stockage du navigateur
**strictement nécessaires** au service : vous garder connecté, protéger
les formulaires contre la falsification (CSRF) et retenir vos préférences
d'interface. Nous n'utilisons pas de témoins publicitaires ou de suivi, et
aucun n'est déposé par des tiers.

## 11. Vos droits

Sous réserve des exceptions prévues par la loi, vous avez le droit :

a) **d'être informé** des renseignements personnels que nous détenons à
votre sujet, de leur utilisation, de qui y a accès et de leur durée de
conservation;

b) **d'y accéder** et d'en obtenir une copie;

c) **de faire rectifier** les renseignements inexacts, incomplets ou
équivoques;

d) **à la portabilité :** recevoir les renseignements que vous avez
fournis dans un format technologique structuré et couramment utilisé, ou
les faire transmettre à une autre organisation — les fonctions intégrées
d'**exportation** et de **migration du compte** servent à cela;

e) **de retirer votre consentement** aux utilisations facultatives;

f) **de demander la cessation de la diffusion ou la désindexation** de
renseignements vous concernant, ou la réindexation d'un lien, lorsque les
conditions de la loi sont remplies;

g) **de demander la suppression** de vos renseignements — vous pouvez
supprimer vous-même votre compte dans les paramètres en tout temps;

h) **de porter plainte** (voyez la section 12).

**Comment exercer vos droits.** Plusieurs droits peuvent être exercés
directement dans les paramètres de votre compte (modifier le profil,
exporter, supprimer le compte, révoquer des applications et des
sessions). Sinon, écrivez à {{PRIVACY_EMAIL}}. Nous pourrions vous demander
de confirmer votre identité, habituellement en écrivant depuis l'adresse
courriel du compte. Nous répondrons **dans les 30 jours** suivant la
réception de la demande, sans frais (des frais raisonnables peuvent
s'appliquer seulement pour la transcription, la reproduction ou la
transmission, et nous vous en informerons à l'avance). Si nous refusons,
nous expliquerons pourquoi et comment contester la décision.

**Limites.** Nous ne pouvons agir que sur les copies conservées sur cette
Instance. Pour le contenu fédéré à d'autres serveurs, nous enverrons des
demandes de suppression (voyez les Conditions, section 2.4), et vous
pouvez joindre directement les exploitants de ces serveurs.

## 12. Plaintes

Veuillez d'abord joindre la personne responsable de la protection des
renseignements personnels à {{PRIVACY_EMAIL}}. Si vous n'êtes pas
satisfait, vous pouvez porter plainte auprès de :

- **la Commission d'accès à l'information du Québec** — https://www.cai.gouv.qc.ca
- **le Commissariat à la protection de la vie privée du Canada** (lorsque
  la loi fédérale s'applique) — https://www.priv.gc.ca
- [annexe A] votre autorité locale de protection des données si vous
  résidez dans l'UE ou au R.-U.

## 13. Modifications de cette politique

Nous vous aviserons des changements importants au moins 30 jours avant
leur entrée en vigueur, par courriel et par une annonce sur l'Instance.
Les versions précédentes sont archivées à {{CHANGELOG_URL}}.

---

## [FACULTATIF] Annexe A — RGPD / UK GDPR

Si vous résidez dans l'Espace économique européen ou au Royaume-Uni :

**Responsable du traitement :** voyez la section 1. **Représentant dans
l'UE (art. 27), s'il est requis :** {{EU_REPRESENTATIVE}}.

**Bases légales (art. 6) :**

| Traitement | Base légale |
|---|---|
| Fourniture du service et fédération (3.1, 3.3, 6.1) | Exécution d'un contrat (art. 6(1)(b)) |
| Sécurité, prévention des abus, journaux IP, modération | Intérêts légitimes (art. 6(1)(f)) à garder le service sûr |
| Fonctions facultatives (section 5) | Consentement (art. 6(1)(a)), révocable en tout temps |
| Demandes légales, déclaration obligatoire | Obligation légale (art. 6(1)(c)) |

**Transferts internationaux :** le Canada bénéficie d'une décision
d'adéquation de l'UE pour les organisations soumises à la LPRPDE. Pour les
autres fournisseurs ou lorsque l'adéquation ne s'applique pas, nous nous
appuyons sur des clauses contractuelles types. La fédération vers des
serveurs d'autres pays se fait à votre demande, selon vos paramètres de
visibilité.

**Droits supplémentaires :** en plus de la section 11, vous avez le droit
de limiter le traitement et de vous opposer au traitement fondé sur des
intérêts légitimes (art. 18 et 21). Vous pouvez porter plainte auprès de
votre autorité de contrôle.

## [FACULTATIF] Annexe B — États-Unis

Nous ne vendons ni ne partageons de renseignements personnels au sens où
ces termes sont définis dans la loi californienne (CCPA/CPRA), et nous ne
recueillons pas sciemment de renseignements sur des enfants de moins de
13 ans (COPPA).

---

*Ce document est fondé sur un modèle ouvert de {{GITHUB_REPO_URL}}, publié
sous {{TEMPLATE_LICENSE}}.*
