<!--
==========================================================================
 NOTAS PARA LA ADMINISTRACIÓN — BORRE ESTE BLOQUE ANTES DE PUBLICAR
==========================================================================
Plantilla de política de privacidad para una instancia del Fediverso
(ActivityPub) operada desde Quebec, Canadá. Acompaña a Terms.es.md. NO ES
ASESORAMIENTO JURÍDICO: hágala revisar por un·a abogado·a antes de fiarse
de ella.

Reemplace cada {{PLACEHOLDER}}. Borre las secciones [OPCIONAL] que no se
apliquen y cualquier categoría de datos que su programa no recoja. La
política debe describir lo que su servidor hace DE VERDAD: revise los
ajustes de su programa (conservación de registros y de direcciones IP,
traducción, vistas previas de enlaces, almacenamiento de objetos,
proveedor de correo) antes de publicar.

1. QUÉ LEYES SE APLICAN
   - Quebec: Ley sobre la protección de la información personal en el
     sector privado (RLRQ c P-39.1, modificada por la Ley 25). Se aplica a
     toda «empresa», es decir, una actividad económica organizada,
     comercial o NO. Una instancia sin fines de lucro muy probablemente
     está incluida; un servidor puramente personal es una zona gris, y
     cumplirla de todos modos es la opción prudente.
   - Federal: PIPEDA se aplica a la información personal recogida en el
     marco de «actividades comerciales» y a los flujos transfronterizos.
     Una instancia no comercial financiada con donaciones puede quedar
     fuera, pero los principios de este documento la cumplen si se aplica.
   - RGPD de la UE / del Reino Unido [anexo A] si se dirige a personas de
     la UE o del Reino Unido o las sigue.

2. LISTA DE LA LEY 25 (lo que este documento por sí solo NO hace)
   [ ] Designar a una persona responsable de la protección de la
       información personal (por defecto, la de mayor autoridad en la
       organización) y publicar su cargo y contacto en el sitio
       (art. 3.1).
   [ ] Adoptar y publicar políticas de gobernanza (art. 3.2):
       conservación y destrucción, funciones, gestión de reclamaciones.
   [ ] Llevar un registro de incidentes de confidencialidad; avisar a la
       CAI y a las personas afectadas cuando haya un «riesgo de perjuicio
       grave» (arts. 3.5 a 3.8).
   [ ] Hacer una evaluación de impacto en la privacidad antes de comunicar
       información personal fuera de Quebec (art. 17). La federación envía
       datos a todo el mundo, y su proveedor de alojamiento o de correo
       puede estar fuera de Quebec: documente una evaluación que cubra
       ambos.
   [ ] Confidencialidad por defecto (art. 9.1): los ajustes deben dar por
       defecto el mayor nivel de confidencialidad. Haga que sean
       voluntarios (opt-in) por defecto la indexación en buscadores, la
       inclusión en descubrimiento y tendencias, y la visualización
       pública de seguidores y seguidos cuando el programa lo permita.
   [ ] Toda función que identifique, localice o perfile a una persona
       debe declararse y estar DESACTIVADA por defecto (art. 8.1).
   [ ] Responder a las solicitudes de acceso, rectificación y
       portabilidad en un plazo de 30 días (art. 32).
   [ ] Mantener la versión inglesa (Privacy Policy) y la francesa al día
       con esta.
==========================================================================
-->

# Política de privacidad — {{INSTANCE_NAME}}

**Fecha de entrada en vigor:** {{EFFECTIVE_DATE}}
**Versión:** {{VERSION}} — [historial de cambios]({{CHANGELOG_URL}})
**English version:** [Privacy Policy]({{PRIVACY_URL}}?lang=en) · **Version française :** [Politique de confidentialité]({{PRIVACY_URL}}?lang=fr)

> **En resumen (no sustituye a la política completa):**
> Solo recogemos lo necesario para hacer funcionar un servidor del
> Fediverso. No vendemos sus datos, no mostramos publicidad ni usamos
> cookies de seguimiento. Lo que publica de forma pública se envía a otros
> servidores que no controlamos. Sus mensajes directos no están cifrados de
> extremo a extremo. Puede exportar, corregir o borrar sus datos en
> cualquier momento.

---

## 1. Quién es responsable de su información

1.1. {{INSTANCE_NAME}} (`{{DOMAIN}}`) está operado por
{{OPERATOR_LEGAL_NAME}}, {{OPERATOR_POSTAL_ADDRESS}}, Quebec, Canadá
(«**nosotros**»).

1.2. **Persona responsable de la protección de la información
personal:** {{PRIVACY_OFFICER_TITLE}} (por ejemplo: «Administración de la
Instancia»), localizable en **{{PRIVACY_EMAIL}}** o por correo postal en
la dirección anterior. Esta persona gestiona las solicitudes de acceso,
las reclamaciones y los incidentes de confidencialidad.

1.3. Esta política explica qué información personal recogemos cuando usa
la Instancia, por qué, cuánto tiempo la conservamos, con quién la
compartimos y cuáles son sus derechos. Forma parte de nuestras
[Condiciones de uso]({{TERMS_URL}}).

## 2. Una nota sobre el Fediverso

2.1. La Instancia forma parte de una red de servidores gestionados de
forma independiente que intercambian contenido con el protocolo
ActivityPub. **Cuando enviamos su información a otro servidor según sus
ajustes, el operador de ese servidor pasa a ser responsable de ella**,
conforme a sus propias políticas y a las leyes de su jurisdicción. No
podemos controlar ni garantizar cómo la usa, la protege o la borra.

2.2. Su **perfil público y sus publicaciones públicas son públicos** por
naturaleza: cualquier persona en Internet, y cualquier servidor, puede
verlos, copiarlos y guardarlos.

## 3. Qué información recogemos

### 3.1. Información que usted nos da

| Información | ¿Obligatoria? | Visibilidad |
|---|---|---|
| Nombre de usuario (identificador) | Sí | Pública, asociada de forma permanente a la Instancia |
| Dirección de correo | Sí | Privada (solo el equipo) |
| Contraseña | Sí | Guardada solo como hash criptográfico con sal; el equipo nunca puede leerla |
| Motivo para unirse (si los registros requieren aprobación) | {{YES_NO}} | Solo el equipo; se borra tras revisarla o en un plazo de {{SIGNUP_REASON_RETENTION}} |
| Nombre visible, biografía, avatar, cabecera, campos del perfil | No | Pública |
| Fecha de nacimiento / confirmación de edad | {{YES_NO}} | Solo el equipo; se usa únicamente para verificar la edad mínima |
| Publicaciones, respuestas, encuestas, contenido multimedia y sus descripciones | No | Según la visibilidad que elija (véase 3.3) |
| Mensajes directos / privados | No | Usted, los destinatarios y los administradores de los servidores implicados (véase 3.4) |
| Seguidos, seguidores, bloqueos, silenciados, listas, marcadores, favoritos, filtros | No | Seguidos y seguidores: públicos salvo que los oculte en los ajustes. Los demás: privados |
| Denuncias y apelaciones que presenta | No | El equipo; opcionalmente se reenvían anonimizadas al equipo del servidor remoto |
| Mensajes que nos envía | No | Solo el equipo |

### 3.2. Información recogida automáticamente

| Información | Finalidad | Conservación |
|---|---|---|
| Dirección IP de registro y de inicio de sesión | Seguridad, prevención de abusos y spam, aplicación de suspensiones | {{IP_RETENTION}} (recomendado: 12 meses como máximo) |
| Direcciones IP en los registros del servidor web | Seguridad, resolución de problemas, protección contra ataques | {{SERVER_LOG_RETENTION}} (recomendado: 14 días) |
| Nombre del navegador o aplicación (agente de usuario) y hora aproximada de la sesión | Le permite ver y revocar sus sesiones activas | Hasta que la sesión se revoque o caduque |
| Aplicaciones autorizadas y tokens de acceso | Permiten que aplicaciones de terceros accedan a su cuenta a petición suya | Hasta que las revoque |
| Marcas de tiempo de la cuenta (creación, última actividad) | Gestión de la cuenta, política de inactividad | Vida de la cuenta |

**No** usamos analítica, píxeles de seguimiento, huella digital ni
herramientas publicitarias. [Si añade analítica respetuosa con la
privacidad, descríbala aquí.]

### 3.3. Visibilidad de las publicaciones

| Visibilidad | Quién la recibe |
|---|---|
| Pública | Todo el mundo; aparece en las cronologías públicas y se envía a otros servidores |
| No listada / pública discreta | Quien tenga el enlace; no aparece en las cronologías públicas |
| Solo seguidores | Sus seguidores, también en otros servidores |
| Directa / solo mencionados | Solo las cuentas mencionadas |

Mencionar a alguien en una publicación se la envía, aunque no le siga.

### 3.4. Mensajes directos

Los mensajes directos **no están cifrados de extremo a extremo**. Se
guardan en este servidor y en los de los destinatarios. Nuestros
administradores pueden acceder técnicamente a ellos. Solo accedemos cuando
es necesario para investigar una denuncia presentada por usted o por el
destinatario, para cumplir una obligación legal o para proteger la
seguridad del servicio. Un mensaje denunciado se muestra a los moderadores
con la denuncia. No los use para compartir información sensible.

### 3.5. Información sobre personas de otros servidores

Recibimos y guardamos el contenido y la información de perfil de cuentas
de otros servidores cuando nos llegan (por ejemplo, porque un miembro de
aquí las sigue). Solo los tratamos para mostrarlos a nuestros miembros,
moderarlos y hacer funcionar la federación.

## 4. Por qué usamos su información

Usamos la información personal solo para los fines siguientes, y para
ningún otro sin su consentimiento salvo que la ley lo permita:

a) **Prestar el servicio:** crear y gestionar su cuenta, publicar y
entregar su contenido según sus ajustes, mostrarle contenido, enviarle las
notificaciones que activó;

b) **Comunicarnos con usted:** correos de confirmación, restablecimiento de
contraseña, alertas de seguridad, avisos de moderación y avisos de cambios
en nuestras Condiciones o en esta política. No enviamos correos de
marketing;

c) **Seguridad y moderación:** prevenir el spam, los abusos, los accesos no
autorizados y la elusión de suspensiones; gestionar denuncias y
apelaciones;

d) **Cumplimiento legal:** atender obligaciones legales y procedimientos
judiciales válidos (véase 6.3), incluida la denuncia obligatoria del
material de abuso sexual infantil;

e) **Hacer funcionar el servidor:** copias de seguridad, resolución de
problemas, planificación de capacidad (con estadísticas agregadas y no
identificativas cuando sea posible).

**Consentimiento.** Al crear una cuenta consiente los usos descritos en
a) a e), necesarios para prestar el servicio. Las funciones opcionales de
la sección 5 requieren su elección aparte y voluntaria (opt-in). Puede
retirar su consentimiento en cualquier momento, con las restricciones
legales; retirar el consentimiento a los usos necesarios supone cerrar su
cuenta.

**Menores.** No recogemos a sabiendas información personal de una persona
menor de 14 años sin el consentimiento de quien ejerce la autoridad
parental.

**Sin decisiones automatizadas.** No tomamos decisiones sobre usted
basadas exclusivamente en un tratamiento automatizado. [Si usa filtros
antispam automáticos que pueden suspender cuentas sin revisión humana,
indíquelo aquí y explique cómo pedir una revisión humana — exigido por la
Ley 25, art. 12.1.]

**Sin venta, sin publicidad, sin entrenamiento de IA.** No vendemos ni
alquilamos información personal, no la usamos para publicidad ni la
cedemos para entrenar modelos de inteligencia artificial.

## 5. Funciones opcionales (desactivadas por defecto)

Conforme a la confidencialidad por defecto, lo siguiente está
**desactivado salvo que lo active** en sus ajustes:

- Permitir que los buscadores indexen su perfil;
- Incluir su cuenta y sus publicaciones en las funciones de
  descubrimiento (sugerencias, tendencias);
- {{OTHER_OPTIONAL_FEATURES}}.

**Búsqueda de publicaciones (activada por defecto).** Sus publicaciones
públicas pueden encontrarse con la búsqueda de esta Instancia y de otros
servidores que respetan el ajuste `indexable` (como Mastodon). Puede
desactivarla en cualquier momento en sus ajustes; sus publicaciones solo
aparecerán entonces en los resultados para usted y para quienes
interactuaron con ellas.

[OPCIONAL] **Traducción.** Si usa el botón «traducir», el texto de la
publicación se envía a {{TRANSLATION_PROVIDER}} ({{PROVIDER_LOCATION}})
para traducirlo. Solo se envía el texto de la publicación.

[OPCIONAL] **Vistas previas de enlaces y contenido remoto.** Cuando una
publicación contiene un enlace, nuestro servidor (no su dispositivo)
obtiene una vista previa del sitio enlazado. Su dirección IP no se
comparte con ese sitio. El contenido multimedia remoto se guarda en caché
en nuestro servidor durante {{MEDIA_CACHE_DAYS}} días.

## 6. Con quién compartimos la información

### 6.1. Otros servidores del Fediverso

Como se describe en las secciones 2 y 3.3, su contenido y su perfil
público se entregan a otros servidores según sus ajustes de visibilidad.
Esos servidores pueden estar en cualquier parte del mundo (véase la
sección 7). Las publicaciones públicas (no las no listadas, solo para
seguidores o directas) también se envían a los relés que la Instancia sigue
o atiende, que las reenvían a otros servidores (la lista la mantiene la
administración).

### 6.2. Proveedores de servicios

Usamos los siguientes proveedores, que tratan información personal solo
por cuenta nuestra, mediante acuerdos escritos que les obligan a
protegerla y a usarla solo para los servicios que nos prestan:

| Proveedor | Servicio | Información implicada | Ubicación |
|---|---|---|---|
| {{HOSTING_PROVIDER}} | Alojamiento del servidor | Todos los datos guardados en la Instancia | {{LOCATION}} |
| {{EMAIL_PROVIDER}} | Envío de correos | Dirección de correo, contenido de los correos | {{LOCATION}} |
| {{STORAGE_PROVIDER}} [OPCIONAL] | Almacenamiento multimedia | Contenido multimedia subido | {{LOCATION}} |
| {{CDN_PROVIDER}} [OPCIONAL] | Distribución de contenido / protección DDoS | Dirección IP, peticiones | {{LOCATION}} |
| {{TRANSLATION_PROVIDER}} [OPCIONAL] | Traducción | Texto de las publicaciones que traduce | {{LOCATION}} |
| {{BACKUP_PROVIDER}} | Copias de seguridad cifradas | Todos los datos | {{LOCATION}} |

### 6.3. Requisitos legales

Solo podemos comunicar información personal sin su consentimiento cuando
la ley lo permite o lo exige, por ejemplo: en respuesta a una orden
judicial, una orden de registro u otro procedimiento legal válido; para
denunciar material de abuso sexual infantil como exige la ley federal;
para evitar un acto de violencia cuando hay motivos razonables para creer
que un riesgo grave e inminente de muerte o de lesiones graves amenaza a
una persona o a un grupo identificable. Revisaremos cada solicitud,
comunicaremos solo lo necesario y, salvo que la ley lo prohíba o hacerlo
aumente el riesgo, se lo comunicaremos. Publicaremos estadísticas
agregadas sobre las solicitudes legales en {{TRANSPARENCY_URL}}
[OPCIONAL].

### 6.4. Transferencia de la operación

Si la operación de la Instancia se transfiere a otro operador (véanse las
Condiciones, sección 14.4), la información personal solo se transferirá a
un operador que acepte protegerla conforme a esta política, y se le
avisará con antelación.

## 7. Información comunicada fuera de Quebec

7.1. Su información puede guardarse o tratarse fuera de Quebec, en
particular:
- por nuestros proveedores de servicios, situados en
  {{PROVIDER_LOCATIONS}};
- por otros servidores del Fediverso, situados en cualquier parte del
  mundo, cuando su contenido se federa con ellos.

7.2. Antes de comunicar información personal fuera de Quebec, evaluamos el
impacto en la privacidad, incluidos la sensibilidad de la información, los
fines, las medidas de protección y el marco jurídico del destino. Para
nuestros proveedores, nos basamos en protecciones contractuales. Para la
federación, la comunicación es inherente al servicio que eligió y se
limita a lo que permiten sus ajustes de visibilidad; usted la controla con
esos ajustes, y puede evitarla usando la visibilidad «solo seguidores» o
directa y protegiendo su cuenta.

## 8. Cuánto tiempo conservamos la información

| Información | Conservación |
|---|---|
| Datos de la cuenta y su contenido | Mientras exista su cuenta, hasta que la borre |
| Publicaciones y contenido multimedia que usted borra | Se retiran de inmediato de la base de datos activa; se purgan de las copias de seguridad en un plazo de {{BACKUP_RETENTION_DAYS}} días |
| Publicaciones y comentarios retirados por los moderadores | Se ocultan a todo el mundo de inmediato, se conservan {{REMOVED_CONTENT_DAYS}} días para que una apelación pueda restablecerlos y después se borran con su contenido multimedia |
| Cuenta borrada | {{DELETION_DELAY}}, después se borra, salvo su nombre de usuario, que se conserva de forma permanente (sin otros datos) para evitar suplantaciones |
| Direcciones IP | Véase la sección 3.2 |
| Registros de moderación (denuncias, medidas, apelaciones) | {{MODERATION_RETENTION}} (recomendado: 2 años después de la última medida), para gestionar abusos repetidos y apelaciones |
| Datos sujetos a una obligación legal o a una retención legal | Mientras lo exija la ley |
| Contenido de otros servidores en caché | {{REMOTE_CACHE_RETENTION}} |

Cuando se cumple la finalidad, destruimos la información o la anonimizamos
para que ya no pueda, de forma irreversible, identificar a una persona.

## 9. Seguridad

9.1. Tomamos medidas de seguridad razonables y adecuadas a la sensibilidad
de la información, entre ellas: conexiones cifradas (HTTPS/TLS);
contraseñas con hash; autenticación en dos pasos opcional; acceso de
administración restringido con autenticación robusta; actualizaciones
periódicas del programa; {{ENCRYPTED_BACKUPS}}; y acceso a la información
personal limitado a los miembros del equipo que lo necesitan.

9.2. Ningún sistema es perfectamente seguro. Si un incidente de
confidencialidad presenta un riesgo de perjuicio grave para usted, se lo
notificaremos sin demora, así como a la Commission d'accès à l'information
de Quebec (y, si procede, a la Oficina del Comisionado de Privacidad de
Canadá), y anotaremos cada incidente en nuestro registro de incidentes.

## 10. Cookies y almacenamiento local

Solo usamos cookies y almacenamiento del navegador **estrictamente
necesarios** para el servicio: mantener su sesión iniciada, proteger los
formularios contra la falsificación (CSRF) y recordar sus preferencias de
interfaz. No usamos cookies publicitarias ni de seguimiento, y ningún
tercero instala cookies.

## 11. Sus derechos

Con las excepciones previstas por la ley, tiene derecho a:

a) **Ser informado** de la información personal que tenemos sobre usted,
de su uso, de quién tiene acceso a ella y de cuánto tiempo se conserva;

b) **Acceder** a ella y obtener una copia;

c) **Rectificar** la información inexacta, incompleta o ambigua;

d) **Portabilidad:** recibir la información que nos dio en un formato
tecnológico estructurado y de uso común, o que se envíe a otra
organización — las funciones integradas de **exportación** y de
**migración de la cuenta** sirven para ello;

e) **Retirar su consentimiento** a los usos opcionales;

f) **Pedir el cese de la difusión o la desindexación** de información
sobre usted, o la reindexación de un enlace, cuando se cumplan las
condiciones de la ley;

g) **Pedir el borrado** de su información — puede borrar usted mismo su
cuenta en los ajustes en cualquier momento;

h) **Presentar una reclamación** (véase la sección 12).

**Cómo ejercer sus derechos.** Muchos derechos pueden ejercerse
directamente en los ajustes de su cuenta (editar el perfil, exportar,
borrar la cuenta, revocar aplicaciones y sesiones). Si no, escriba a
{{PRIVACY_EMAIL}}. Podemos pedirle que confirme su identidad, normalmente
escribiendo desde la dirección de correo de la cuenta. Responderemos **en
un plazo de 30 días** desde la recepción de la solicitud, sin coste (solo
puede cobrarse una cantidad razonable por transcripción, reproducción o
envío, y se lo diremos de antemano). Si nos negamos, le explicaremos por
qué y cómo impugnar la decisión.

**Límites.** Solo podemos actuar sobre las copias guardadas en esta
Instancia. Para el contenido federado a otros servidores, enviaremos
solicitudes de borrado (véanse las Condiciones, sección 2.4), y puede
contactar directamente con los operadores de esos servidores.

## 12. Reclamaciones

Contacte primero con la persona responsable de la protección de la
información personal en {{PRIVACY_EMAIL}}. Si no queda satisfecho, puede
presentar una reclamación ante:

- **la Commission d'accès à l'information de Quebec** — https://www.cai.gouv.qc.ca
- **la Oficina del Comisionado de Privacidad de Canadá** (cuando se aplica
  la ley federal) — https://www.priv.gc.ca
- [anexo A] su autoridad local de protección de datos si reside en la UE o
  en el Reino Unido.

## 13. Cambios en esta política

Le avisaremos de los cambios importantes al menos 30 días antes de que
entren en vigor, por correo y con un anuncio en la Instancia. Las
versiones anteriores se archivan en {{CHANGELOG_URL}}.

---

## [OPCIONAL] Anexo A — RGPD / UK GDPR

Si reside en el Espacio Económico Europeo o en el Reino Unido:

**Responsable del tratamiento:** véase la sección 1. **Representante en la
UE (art. 27), si es necesario:** {{EU_REPRESENTATIVE}}.

**Bases jurídicas (art. 6):**

| Tratamiento | Base jurídica |
|---|---|
| Prestación del servicio y federación (3.1, 3.3, 6.1) | Ejecución de un contrato (art. 6(1)(b)) |
| Seguridad, prevención de abusos, registros IP, moderación | Intereses legítimos (art. 6(1)(f)) en mantener el servicio seguro |
| Funciones opcionales (sección 5) | Consentimiento (art. 6(1)(a)), revocable en cualquier momento |
| Solicitudes legales, denuncia obligatoria | Obligación legal (art. 6(1)(c)) |

**Transferencias internacionales:** Canadá cuenta con una decisión de
adecuación de la UE para las organizaciones sujetas a PIPEDA. Para otros
proveedores o cuando no se aplica la adecuación, nos basamos en cláusulas
contractuales tipo. La federación con servidores de otros países se hace a
petición suya, según sus ajustes de visibilidad.

**Derechos adicionales:** además de la sección 11, tiene derecho a limitar
el tratamiento y a oponerse al tratamiento basado en intereses legítimos
(arts. 18 y 21). Puede presentar una reclamación ante su autoridad de
control.

## [OPCIONAL] Anexo B — Estados Unidos

No vendemos ni compartimos información personal en el sentido en que la
ley de California define esos términos (CCPA/CPRA), y no recogemos a
sabiendas información de menores de 13 años (COPPA).

---

*Este documento se basa en una plantilla abierta de {{GITHUB_REPO_URL}},
publicada bajo {{TEMPLATE_LICENSE}}.*
