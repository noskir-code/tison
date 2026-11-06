<!--
==========================================================================
 NOTAS PARA LA ADMINISTRACIÓN — BORRE ESTE BLOQUE ANTES DE PUBLICAR
==========================================================================
Plantilla de política de cookies para una instancia del Fediverso
(ActivityPub) operada desde Quebec, Canadá. Acompaña a Terms.es.md y
Privacy.es.md. NO ES ASESORAMIENTO JURÍDICO.

1. ENUMERE SUS COOKIES REALES. Los nombres de las cookies cambian entre
   programas (Mastodon, GoToSocial, Akkoma, Misskey, etc.) y entre
   versiones. Antes de publicar, abra su instancia en una ventana privada,
   inicie sesión y revise Herramientas de desarrollo → Aplicación/
   Almacenamiento → Cookies, Almacenamiento local, Almacenamiento de sesión,
   IndexedDB y Service workers. Anote cada entrada en las tablas de abajo.
   Vuelva a revisar tras cada actualización importante del programa y tras
   añadir una CDN o un proxy inverso (algunos añaden sus propias cookies,
   p. ej., protección contra bots).

2. POR QUÉ NO HAY AVISO DE CONSENTIMIENTO. Esta plantilla supone que usa
   SOLO cookies y almacenamiento estrictamente necesarios:
   - Quebec (Ley 25, art. 8.1): las tecnologías que identifican, localizan
     o perfilan a una persona deben declararse y estar desactivadas por
     defecto. Las cookies de sesión y de seguridad no hacen nada de eso.
   - UE/Reino Unido (Directiva ePrivacy, art. 5(3) / PECR): el
     almacenamiento estrictamente necesario está exento de consentimiento.
   Si algún día añade analítica, contenidos incrustados que instalen
   cookies de terceros o una CDN que instale cookies de seguimiento,
   necesitará un consentimiento previo, una forma de retirarlo, y habrá que
   reescribir esta política.

3. Mantenga la versión inglesa (Cookie Policy) al día con esta.
==========================================================================
-->

# Política de cookies — {{INSTANCE_NAME}}

**Fecha de entrada en vigor:** {{EFFECTIVE_DATE}}
**Versión:** {{VERSION}} — [historial de cambios]({{CHANGELOG_URL}})
**English version:** [Cookie Policy]({{COOKIES_URL}}?lang=en) · **Version française :** [Politique relative aux témoins]({{COOKIES_URL}}?lang=fr)

> **En resumen:** solo usamos las cookies y el almacenamiento del navegador
> necesarios para iniciar su sesión, proteger su cuenta y recordar sus
> preferencias de visualización. Ninguna cookie publicitaria, de analítica
> ni de seguimiento, y ninguna cookie de terceros. Por eso no hay aviso de
> cookies.

---

## 1. Alcance

1.1. Esta política explica cómo {{INSTANCE_NAME}} (`{{DOMAIN}}`), operado
por {{OPERATOR_LEGAL_NAME}}, usa las cookies y tecnologías similares cuando
visita el sitio web o usa la interfaz web. Forma parte de nuestra
[Política de privacidad]({{PRIVACY_URL}}) y de nuestras
[Condiciones de uso]({{TERMS_URL}}).

1.2. Las aplicaciones de terceros (clientes móviles o de escritorio) que use
para acceder a su cuenta no están cubiertas por esta política. Tienen sus
propias prácticas de privacidad, que le recomendamos revisar.

## 2. Qué son las cookies y las tecnologías similares

2.1. Una **cookie** es un pequeño archivo de texto que un sitio web guarda
en su navegador y vuelve a leer en visitas posteriores.

2.2. Las **tecnologías similares** incluyen el *almacenamiento local*, el
*almacenamiento de sesión*, *IndexedDB* y los *service workers* del
navegador, que guardan datos o código en su dispositivo. En esta política,
«cookies» las incluye a todas.

2.3. Las cookies **propias** las instala nuestro dominio (`{{DOMAIN}}`).
Las cookies **de terceros** las instalan otros dominios. Solo usamos
cookies propias.

## 3. Cookies que usamos

Todas las cookies de abajo son **estrictamente necesarias**. El servicio no
puede funcionar de forma segura sin ellas, por lo que no se pueden
desactivar en nuestra interfaz.

### 3.1. Cookies

| Nombre | Finalidad | Duración | Se instala cuando |
|---|---|---|---|
| {{SESSION_COOKIE}} (p. ej. `_mastodon_session`) | Mantiene su sesión iniciada durante la visita; contiene un identificador de sesión y un token contra la falsificación de peticiones (CSRF) | Fin de la sesión del navegador / {{DURATION}} | Visita el sitio |
| {{REMEMBER_COOKIE}} (p. ej. `remember_user_token`) | Mantiene su sesión iniciada entre visitas si elige «Mantener la sesión» | {{DURATION}} | Inicia sesión con «Mantener la sesión» |
| {{OTHER_COOKIE}} | {{PURPOSE}} | {{DURATION}} | {{TRIGGER}} |

Nuestras cookies llevan las marcas `Secure` (solo se envían por conexiones
HTTPS cifradas), `HttpOnly` cuando es posible (los scripts no pueden
leerlas) y `SameSite` (no se envían con peticiones de otros sitios), para
protegerlas contra el robo y los ataques entre sitios.

### 3.2. Almacenamiento del navegador

| Tipo | Qué se guarda | Finalidad | Cómo borrarlo |
|---|---|---|---|
| Almacenamiento local | Preferencias de la interfaz (p. ej. columnas, tema, idioma, borrador sin enviar, anuncios descartados) | Recuerda sus ajustes de visualización en este dispositivo | Borrar los datos del sitio en su navegador |
| Almacenamiento de sesión | Estado temporal de la interfaz | Mantiene la interfaz funcionando durante la visita | Se borra al cerrar la pestaña |
| IndexedDB / caché | Cronologías, emojis y archivos de la interfaz en caché | Acelera la carga y reduce el uso de datos | Borrar los datos del sitio en su navegador |
| Service worker | Pequeño script para notificaciones push y carga sin conexión | Entrega las notificaciones web que activó | Desactivar las notificaciones o borrar los datos del sitio |

### 3.3. Notificaciones web push

Si activa las notificaciones del navegador, este crea una suscripción push
(una dirección que da el servicio push de su navegador, como Mozilla,
Google o Apple) que guardamos en nuestro servidor. El contenido de las
notificaciones pasa por ese servicio, cifrado según el estándar Web Push.
Puede revocarla en cualquier momento en sus ajustes de notificaciones o en
los de su navegador.

## 4. Lo que no usamos

- Ninguna cookie publicitaria o de marketing.
- Ninguna herramienta de analítica o de medición de audiencia.
- Ningún píxel de seguimiento de redes sociales, botón «Me gusta» ni
  huella digital del navegador.
- Ninguna cookie de terceros.
- [OPCIONAL] **Contenido incrustado:** las publicaciones pueden mostrar
  vistas previas de enlaces (imagen, título, descripción). Nuestro servidor
  las genera y las sirve, así que el sitio enlazado no recibe su dirección
  IP ni instala cookies.
  {{IF_EMBEDDED_PLAYERS: describa los reproductores de vídeo o audio
  incrustados que cargan contenido de terceros y piden un clic antes de
  cargarse}}.

## 5. Contenido de otros servidores

Las imágenes, vídeos y avatares de otros servidores del Fediverso se
{{REMOTE_MEDIA_MODE: «descargan y sirven desde nuestro propio servidor (por
proxy), así que otros servidores no reciben su dirección IP ni instalan
cookies en su dispositivo» / «cargan en algunos casos directamente desde su
servidor de origen, que puede recibir su dirección IP»}}.

## 6. Cómo gestionar las cookies

6.1. Puede ver, bloquear o borrar las cookies en los ajustes de su
navegador. Para obtener ayuda con los navegadores principales (Firefox,
Chrome, Safari, Edge), busque «borrar cookies» en la ayuda de su navegador.

6.2. Si bloquea o borra las cookies estrictamente necesarias, se cerrará su
sesión y es posible que no pueda iniciarla ni usar la interfaz web. Las
páginas públicas siguen siendo legibles sin cookies.

6.3. Cerrar la sesión borra su cookie de sesión. También puede cerrar en
cualquier momento las sesiones de sus otros dispositivos en
**Ajustes → Cuenta → Sesiones** {{OR_EQUIVALENT_PATH}}.

## 7. Cambios

Si cambiamos la forma en que usamos las cookies, actualizaremos esta
política y su número de versión. Si algún día añadimos cookies que no sean
estrictamente necesarias, le pediremos su consentimiento antes de
instalarlas, y estarán desactivadas por defecto.

## 8. Contacto

Preguntas: {{PRIVACY_EMAIL}} (persona responsable de la protección de la
información personal). Consulte la sección 12 de la
[Política de privacidad]({{PRIVACY_URL}}) para saber cómo presentar una
reclamación.

---

*Este documento se basa en una plantilla abierta de {{GITHUB_REPO_URL}},
publicada bajo {{TEMPLATE_LICENSE}}.*
