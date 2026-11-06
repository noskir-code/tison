<!--
==========================================================================
 NOTES POUR L'ADMINISTRATION — SUPPRIMEZ CE BLOC AVANT DE PUBLIER
==========================================================================
Modèle de politique relative aux témoins pour une instance du Fédivers
(ActivityPub) exploitée depuis le Québec, Canada. Va avec Terms.fr.md et
Privacy.fr.md. CE N'EST PAS UN AVIS JURIDIQUE.

1. LISTEZ VOS VRAIS TÉMOINS. Le nom des témoins change d'un logiciel à
   l'autre (Mastodon, GoToSocial, Akkoma, Misskey, etc.) et d'une version à
   l'autre. Avant de publier, ouvrez votre instance dans une fenêtre de
   navigation privée, connectez-vous et vérifiez Outils de développement →
   Application/Stockage → Témoins (cookies), Stockage local, Stockage de
   session, IndexedDB et Service workers. Inscrivez chaque entrée dans les
   tableaux ci-dessous. Vérifiez de nouveau après chaque mise à jour majeure
   du logiciel et après l'ajout d'un CDN ou d'un mandataire inverse
   (certains ajoutent leurs propres témoins, p. ex. contre les robots).

2. POURQUOI IL N'Y A PAS DE BANDEAU DE CONSENTEMENT. Ce modèle suppose que
   vous n'utilisez QUE des témoins et du stockage strictement nécessaires :
   - Québec (Loi 25, art. 8.1) : les technologies qui permettent
     d'identifier, de localiser ou de profiler une personne doivent être
     divulguées et désactivées par défaut. Les témoins de session et de
     sécurité ne font rien de cela.
   - UE/R.-U. (directive ePrivacy, art. 5(3) / PECR) : le stockage
     strictement nécessaire est exempté de consentement.
   Si vous ajoutez un jour des statistiques, des intégrations qui déposent
   des témoins tiers ou un CDN qui dépose des témoins de suivi, il vous
   faudra un consentement préalable (opt-in), un moyen de le retirer, et
   cette politique devra être réécrite.

3. Gardez la version anglaise (Cookie Policy) à jour avec celle-ci.
==========================================================================
-->

# Politique relative aux témoins — {{INSTANCE_NAME}}

**Date d'entrée en vigueur :** {{EFFECTIVE_DATE}}
**Version :** {{VERSION}} — [historique des modifications]({{CHANGELOG_URL}})
**English version:** [Cookie Policy]({{COOKIES_URL}}?lang=en) · **Versión en español:** [Política de cookies]({{COOKIES_URL}}?lang=es)

> **En bref :** nous utilisons seulement les témoins (cookies) et le
> stockage du navigateur nécessaires pour vous connecter, protéger votre
> compte et retenir vos préférences d'affichage. Aucun témoin publicitaire,
> statistique ou de suivi, et aucun témoin tiers. C'est pourquoi il n'y a
> pas de bandeau de témoins.

---

## 1. Portée

1.1. Cette politique explique comment {{INSTANCE_NAME}} (`{{DOMAIN}}`),
exploité par {{OPERATOR_LEGAL_NAME}}, utilise les témoins et les
technologies semblables quand vous visitez le site Web ou utilisez
l'interface Web. Elle fait partie de notre
[Politique de confidentialité]({{PRIVACY_URL}}) et de nos
[Conditions d'utilisation]({{TERMS_URL}}).

1.2. Les applications tierces (clients mobiles ou de bureau) que vous
utilisez pour accéder à votre compte ne sont pas visées par cette
politique. Elles ont leurs propres pratiques de confidentialité, que nous
vous invitons à consulter.

## 2. Ce que sont les témoins et les technologies semblables

2.1. Un **témoin** (cookie) est un petit fichier texte qu'un site Web
enregistre dans votre navigateur et relit lors de vos visites suivantes.

2.2. Les **technologies semblables** comprennent le *stockage local*, le
*stockage de session*, *IndexedDB* et les *service workers* du navigateur,
qui enregistrent des données ou du code sur votre appareil. Dans cette
politique, « témoins » les désigne tous.

2.3. Les témoins **internes** sont déposés par notre domaine
(`{{DOMAIN}}`). Les témoins **tiers** sont déposés par d'autres domaines.
Nous n'utilisons que des témoins internes.

## 3. Les témoins que nous utilisons

Tous les témoins ci-dessous sont **strictement nécessaires**. Le service ne
peut pas fonctionner de façon sécuritaire sans eux; ils ne peuvent donc pas
être désactivés dans notre interface.

### 3.1. Témoins

| Nom | Utilité | Durée | Déposé quand |
|---|---|---|---|
| {{SESSION_COOKIE}} (p. ex. `_mastodon_session`) | Vous garde connecté pendant votre visite; contient un identifiant de session et un jeton contre la falsification de requêtes (CSRF) | Fin de la session du navigateur / {{DURATION}} | Vous visitez le site |
| {{REMEMBER_COOKIE}} (p. ex. `remember_user_token`) | Vous garde connecté d'une visite à l'autre si vous choisissez « Rester connecté » | {{DURATION}} | Vous vous connectez avec « Rester connecté » |
| {{OTHER_COOKIE}} | {{PURPOSE}} | {{DURATION}} | {{TRIGGER}} |

Nos témoins sont marqués `Secure` (envoyés seulement par des connexions
HTTPS chiffrées), `HttpOnly` lorsque c'est possible (illisibles par les
scripts) et `SameSite` (non envoyés avec les requêtes venant d'autres
sites), pour les protéger contre le vol et les attaques intersites.

### 3.2. Stockage du navigateur

| Type | Ce qui est enregistré | Utilité | Comment l'effacer |
|---|---|---|---|
| Stockage local | Préférences d'interface (p. ex. disposition des colonnes, thème, langue, brouillon non envoyé, annonces masquées) | Retient vos réglages d'affichage sur cet appareil | Effacer les données du site dans votre navigateur |
| Stockage de session | État temporaire de l'interface | Garde l'interface fonctionnelle pendant votre visite | Effacé à la fermeture de l'onglet |
| IndexedDB / cache | Fils, émojis et fichiers d'interface mis en cache | Accélère le chargement et réduit l'utilisation de données | Effacer les données du site dans votre navigateur |
| Service worker | Petit script pour les notifications poussées et le chargement hors ligne | Livre les notifications Web que vous avez activées | Désactiver les notifications ou effacer les données du site |

### 3.3. Notifications Web poussées

Si vous activez les notifications du navigateur, celui-ci crée un
abonnement de notification (une adresse fournie par le service de
notification de votre navigateur, comme Mozilla, Google ou Apple) que nous
enregistrons sur notre serveur. Le contenu des notifications passe par ce
service, chiffré selon la norme Web Push. Vous pouvez révoquer cet
abonnement en tout temps dans vos paramètres de notification ou dans ceux
de votre navigateur.

## 4. Ce que nous n'utilisons pas

- Aucun témoin publicitaire ou de marketing.
- Aucun outil de statistiques ou de mesure d'audience.
- Aucun pixel de suivi de médias sociaux, bouton « J'aime » ni prise
  d'empreinte numérique.
- Aucun témoin tiers.
- [FACULTATIF] **Intégrations :** les publications peuvent afficher un
  aperçu des liens (image, titre, description). Ces aperçus sont produits
  et servis par notre serveur : le site lié ne reçoit pas votre adresse IP
  et ne dépose pas de témoins.
  {{IF_EMBEDDED_PLAYERS: décrivez les lecteurs vidéo ou audio intégrés qui
  chargent du contenu tiers et qui exigent un clic avant de se charger}}.

## 5. Contenu provenant d'autres serveurs

Les images, vidéos et avatars d'autres serveurs du Fédivers sont
{{REMOTE_MEDIA_MODE: « téléchargés et servis depuis notre propre serveur
(par mandataire), de sorte que les autres serveurs ne reçoivent pas votre
adresse IP et ne déposent pas de témoins sur votre appareil » / « dans
certains cas chargés directement depuis leur serveur d'origine, qui peut
recevoir votre adresse IP »}}.

## 6. Comment gérer les témoins

6.1. Vous pouvez consulter, bloquer ou supprimer les témoins dans les
paramètres de votre navigateur. Pour obtenir de l'aide avec les principaux
navigateurs (Firefox, Chrome, Safari, Edge), cherchez « effacer les
témoins » ou « effacer les cookies » dans l'aide de votre navigateur.

6.2. Si vous bloquez ou supprimez les témoins strictement nécessaires,
vous serez déconnecté et pourriez ne plus pouvoir vous connecter ni
utiliser l'interface Web. Les pages publiques restent lisibles sans
témoins.

6.3. La déconnexion supprime votre témoin de session. Vous pouvez aussi
fermer en tout temps les sessions ouvertes sur vos autres appareils dans
**Paramètres → Compte → Sessions** {{OR_EQUIVALENT_PATH}}.

## 7. Modifications

Si nous changeons notre utilisation des témoins, nous mettrons à jour
cette politique et son numéro de version. Si nous introduisons un jour des
témoins qui ne sont pas strictement nécessaires, nous vous demanderons
votre consentement avant de les déposer, et ils seront désactivés par
défaut.

## 8. Nous joindre

Questions : {{PRIVACY_EMAIL}} (personne responsable de la protection des
renseignements personnels). Voyez la section 12 de la
[Politique de confidentialité]({{PRIVACY_URL}}) pour savoir comment porter
plainte.

---

*Ce document est fondé sur un modèle ouvert de {{GITHUB_REPO_URL}}, publié
sous {{TEMPLATE_LICENSE}}.*
