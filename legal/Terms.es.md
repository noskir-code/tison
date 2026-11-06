<!--
==========================================================================
 NOTAS PARA LA ADMINISTRACIÓN — BORRE ESTE BLOQUE ANTES DE PUBLICAR
==========================================================================
Esta plantilla está redactada para una instancia del Fediverso
(ActivityPub) operada desde Quebec, Canadá, por una persona o una pequeña
organización sin fines de lucro. NO ES ASESORAMIENTO JURÍDICO. Hágala
revisar por un·a abogado·a o notario·a antes de fiarse de ella.

Reemplace cada {{PLACEHOLDER}}. Las secciones marcadas [OPCIONAL] pueden
eliminarse.

1. EN QUEBEC, LA VERSIÓN FRANCESA ES OBLIGATORIA (Terms.fr.md) y debe
   ofrecerse antes que esta. Según el art. 55 de la Carta de la lengua
   francesa (modificada por la ley 96, vigente desde el 1 de junio de
   2023), estas Condiciones son un contrato de adhesión. Esta versión en
   español es una traducción para comodidad de los miembros
   hispanohablantes.

2. Cláusulas deliberadamente NO incluidas porque son nulas o
   inaplicables frente a los consumidores de Quebec:
   - Arbitraje obligatorio / renuncia a las acciones colectivas (Ley de
     protección del consumidor, RLRQ c P-40.1, art. 11.1).
   - Exclusiones generales de responsabilidad (LPC art. 10; Código Civil
     de Quebec, art. 1474: ninguna exclusión por daños corporales o
     morales, ni por culpa intencional o grave).
   - Obligar a los consumidores de Quebec a demandar en otro lugar
     (C.C.Q. art. 3149).
   - Cambios unilaterales sin aviso (LPC art. 11.2: se exige un aviso
     previo por escrito en los contratos de consumo).

3. Acompáñela de una Privacy.es.md aparte. La Ley 25 de Quebec (Ley sobre
   la protección de la información personal en el sector privado) exige
   una persona responsable de la información personal, una política de
   privacidad publicada y la notificación de incidentes a la CAI. La ley
   federal PIPEDA también puede aplicarse.

4. Para vigilar (a noviembre de 2026): el proyecto de ley federal C-34
   (Ley de seguridad en las redes sociales), presentado en junio de 2026,
   impondría obligaciones a los servicios de redes sociales, entre ellas
   restringir las cuentas de menores de 16 años y retirar obligatoriamente
   el material de abuso sexual infantil y las imágenes íntimas no
   consentidas. Es un proyecto, no una ley: compruebe su estado y si las
   instancias pequeñas entran en su ámbito.
==========================================================================
-->

# Condiciones de uso — {{INSTANCE_NAME}}

**Fecha de entrada en vigor:** {{EFFECTIVE_DATE}}
**Versión:** {{VERSION}} — [historial de cambios]({{CHANGELOG_URL}})
**English version:** [Terms of Use]({{TERMS_URL}}?lang=en) · **Version française :** [Conditions d'utilisation]({{TERMS_URL}}?lang=fr)

> **En resumen (no sustituye a las Condiciones completas):**
> {{INSTANCE_NAME}} es un servidor del Fediverso, una red de servidores
> gestionados de forma independiente que se comunican entre sí. Sus
> publicaciones públicas salen de este servidor y se copian en otros que no
> controlamos. Sea respetuoso y siga las normas; usted sigue siendo dueño
> de su contenido y puede irse llevándose a sus seguidores.

---

## 1. Quiénes somos y qué cubren estas Condiciones

1.1. {{INSTANCE_NAME}} (la «**Instancia**», «**nosotros**») es un servidor
accesible en `{{DOMAIN}}`, operado por {{OPERATOR_LEGAL_NAME}}
({{OPERATOR_TYPE: «una persona física» / «una organización sin fines de lucro constituida conforme a…»}}),
{{OPERATOR_POSTAL_ADDRESS}}, Quebec, Canadá.

1.2. Contacto: {{CONTACT_EMAIL}}. Denuncias de moderación: con la función
«Denunciar» de la interfaz, o en {{MODERATION_EMAIL}}.

1.3. Estas Condiciones de uso (las «**Condiciones**») forman un contrato
entre usted y el operador. Se aplican a toda persona que crea una cuenta en
la Instancia («**usted**», un «**miembro**»). Las secciones 6, 8 y 11
también describen cómo tratamos el contenido que llega de otros servidores.

1.4. Estas Condiciones se completan con los siguientes documentos, que
forman parte de ellas por remisión:
- **Normas del servidor / código de conducta**: {{RULES_URL}}
- **Política de privacidad**: {{PRIVACY_URL}}
- **Política de moderación y federación** [OPCIONAL]: {{MODERATION_POLICY_URL}}

Si un documento contradice estas Condiciones, prevalecen estas
Condiciones, salvo que la Política de privacidad prevalece en lo relativo
a la información personal.

1.5. **Idioma.** Esta versión en español es una traducción ofrecida para
su comodidad. Las versiones de referencia son la francesa, que se le
ofreció en primer lugar, y la inglesa ({{TERMS_URL}}?lang=en). Si las versiones
difieren, puede acogerse a la que le sea más favorable.

## 2. Cómo funciona el Fediverso (lea esto)

2.1. La Instancia funciona con {{SOFTWARE_NAME}} ({{SOFTWARE_URL}}),
software libre y de código abierto, y se comunica con otros servidores
mediante el protocolo ActivityPub y estándares relacionados (la
«**federación**»).

2.2. Cada servidor del Fediverso lo gestiona de forma independiente su
propio operador, con sus propias normas y leyes. **No controlamos otros
servidores** y no somos responsables de su contenido, su moderación, su
seguridad ni de cómo tratan sus datos.

2.3. Cuando publica contenido visible más allá de esta Instancia, se envían
copias a otros servidores, que las guardan y las muestran, incluidos los
servidores de quienes le siguen, de quienes menciona o de quienes le
responden.

2.4. **Fuera de esta Instancia, el borrado se hace en la medida de lo
posible.** Cuando edita o borra contenido, o borra su cuenta, enviamos
avisos de edición o borrado a los servidores que sabemos que lo recibieron.
No podemos garantizar que otros servidores respeten esos avisos, y pueden
quedar copias (incluidas capturas, archivos, índices de búsqueda y copias
de seguridad).

2.5. **Los mensajes directos o privados no están cifrados de extremo a
extremo**, salvo que el programa diga expresamente lo contrario. Se
guardan en este servidor y en los de los destinatarios, y los
administradores de esos servidores pueden acceder a ellos (véase la
sección 8.4). No los use para información confidencial.

## 3. Requisitos y cuentas

3.1. **Edad mínima.** Debe tener al menos **{{MINIMUM_AGE}}** años
(recomendado: 16) para crear una cuenta. Si no ha alcanzado la mayoría de
edad en su lugar de residencia (18 en Quebec), confirma que su madre, padre
o tutor legal ha leído y aceptado estas Condiciones en su nombre. No
recogemos a sabiendas información personal de menores de 14 años sin el
consentimiento de quien ejerce la autoridad parental.

3.2. **Registro.** Los registros están {{REGISTRATION_MODE: «abiertos» /
«sujetos a aprobación» / «solo por invitación»}}. Podemos rechazar
cualquier registro a nuestra discreción, siempre que no sea por un motivo
discriminatorio prohibido por la Carta de derechos y libertades de la
persona de Quebec.

3.3. **Exactitud y seguridad.** Debe dar una dirección de correo que
funcione y mantenerla al día. Es responsable de mantener en secreto su
contraseña y sus tokens de aplicación, y de la actividad realizada con su
cuenta, salvo que se deba a culpa nuestra o a un fallo de seguridad por
nuestra parte. Le recomendamos encarecidamente activar la autenticación en
dos pasos. Avísenos cuanto antes en {{CONTACT_EMAIL}} si sospecha un
acceso no autorizado.

3.4. **Nombres de usuario y suplantación.** Puede usar un seudónimo. No
puede hacerse pasar por otra persona u organización de forma engañosa. Las
cuentas de parodia y de fans deben identificarse claramente como tales.

3.5. **Cuentas automatizadas.** Los bots están {{BOTS_POLICY: «permitidos»
/ «permitidos si se marcan como bots en los ajustes del perfil y cumplen
las Normas» / «prohibidos»}}.

3.6. **Precio.** El uso de la Instancia es gratuito. Las donaciones
voluntarias {{DONATION_URL}} no crean derechos adicionales ni derecho a
ningún nivel de servicio, y no son reembolsables salvo que la ley lo exija.
[Si algún día vende cuentas de pago, habrá que revisar estas Condiciones:
se aplicarán las normas de Quebec de protección del consumidor sobre los
contratos a distancia.]

## 4. Su contenido

4.1. **Conserva sus derechos.** Conserva la propiedad y todos los derechos
de propiedad intelectual sobre los textos, imágenes, audio, vídeo y demás
material que publica («**su contenido**»).

4.2. **Licencia que nos concede.** Para que la Instancia y la federación
funcionen, concede al operador una **licencia no exclusiva, mundial,
gratuita e intransferible** (con derecho a sublicenciar solo en la medida
necesaria para la federación, como se describe abajo) para alojar,
guardar, reproducir, almacenar en caché, dar formato, redimensionar,
convertir, mostrar y transmitir su contenido, **solo para operar la
Instancia y entregar su contenido al público que eligió** (por ejemplo:
público, no listado, solo seguidores, personas mencionadas). Esto incluye
enviar su contenido a otros servidores según sus ajustes de visibilidad.

4.3. **Duración de la licencia.** La licencia termina cuando borra el
contenido o su cuenta, salvo a) para las copias ya entregadas a otros
servidores antes del borrado (véase 2.4), b) para las copias de seguridad,
que se sobrescriben en un plazo de {{BACKUP_RETENTION_DAYS}} días, y c)
cuando debamos conservar contenido para cumplir la ley o un procedimiento
judicial.

4.4. **Lo que no haremos.** No venderemos su contenido, no lo usaremos para
publicidad ni lo cederemos a terceros para entrenar modelos de
inteligencia artificial. [OPCIONAL — bórrelo si no se aplica:] Cuando sea
técnicamente posible, también indicaremos a los rastreadores automáticos
(p. ej., con las directivas `robots.txt` y `ai.txt`) que el contenido de
esta Instancia no puede usarse para entrenar IA, pero no podemos garantizar
que terceros lo respeten.

4.5. **Su responsabilidad.** Es responsable de su contenido. Confirma que
tiene los derechos necesarios para publicarlo y que publicarlo no infringe
estas Condiciones, las Normas ni los derechos de nadie, incluidos los
derechos de autor, el derecho a la propia imagen (C.C.Q. arts. 3, 35 y 36)
y el derecho a la privacidad.

4.6. **Contenido de otras personas.** El contenido de otros miembros o de
otros servidores pertenece a sus autores. Compartirlo con las funciones
normales del programa está permitido; si no, pida permiso al autor antes de
reutilizarlo.

4.7. **Advertencias de contenido y contenido sensible.** Debe usar
advertencias de contenido y marcar el contenido multimedia como sensible
según lo exigen las Normas.

## 5. Uso aceptable

5.1. Debe cumplir las **Normas del servidor** ({{RULES_URL}}) y todas las
leyes aplicables, incluidas las de Canadá y Quebec.

5.2. Sin limitar las Normas, **no** debe usar la Instancia para publicar,
enviar, enlazar o facilitar:

a) **cualquier contenido sexual que involucre a un menor**, incluidas
representaciones dibujadas, animadas o generadas por IA (Código Penal,
art. 163.1). Conlleva la suspensión inmediata y la denuncia a las
autoridades (véase 5.4);

b) imágenes íntimas de una persona compartidas sin su consentimiento,
incluidos deepfakes sexualizados (Código Penal, art. 162.1);

c) contenido que incite a la violencia, amenazas (Código Penal,
art. 264.1), o la promoción del terrorismo o del extremismo violento;

d) propaganda de odio, o contenido que promueva el odio o la
discriminación contra personas por un motivo protegido como la raza, el
color, el origen étnico o nacional, la religión, el sexo, la orientación
sexual, la identidad o expresión de género, la edad o la discapacidad
(Código Penal, arts. 318 y 319; Carta de Quebec, art. 10);

e) acoso, intimidación, doxxing (publicar información personal de alguien
sin su consentimiento), acecho u organización de linchamientos en grupo;

f) difamación (C.C.Q. art. 1457) o contenido que vulnere la privacidad de
alguien;

g) material que infrinja derechos de autor, marcas u otros derechos de
propiedad intelectual;

h) spam, mensajes comerciales no solicitados (incluidos los contrarios a la
Ley antispam de Canadá), interacción artificial o creación masiva de
cuentas;

i) malware, phishing, estafas o fraude;

j) contenido patrocinado no declarado o publicidad engañosa;

k) {{ADDITIONAL_PROHIBITIONS}}.

5.3. **No** debe:
- interferir en la seguridad o el funcionamiento de la Instancia o de otros
  servidores (p. ej., denegación de servicio, explotación de
  vulnerabilidades sin autorización, eludir límites de frecuencia,
  suspensiones o bloqueos);
- extraer o recopilar en masa contenido o información personal de la
  Instancia por medios automatizados, salvo mediante las interfaces
  ActivityPub y API estándar usadas según los ajustes de visibilidad de
  las personas, y nunca para crear perfiles de individuos ni conjuntos de
  datos para entrenar IA sin consentimiento;
- crear otra cuenta para eludir una suspensión.

**Investigación de seguridad:** si encuentra una vulnerabilidad,
comuníquela en privado a {{SECURITY_EMAIL}} antes de divulgarla.

5.4. **Denuncia obligatoria.** Conforme a la *Ley sobre la denuncia
obligatoria de la pornografía infantil en Internet por las personas que
prestan servicios de Internet* (L.C. 2011, c. 4), debemos denunciar el
material de abuso sexual infantil al organismo designado (Cybertip.ca) o a
la policía, y conservar los datos relacionados durante el plazo que exige
la ley. Lo cumpliremos.

## 6. Moderación

6.1. Moderamos la Instancia para hacer cumplir estas Condiciones y las
Normas. La moderación la llevan a cabo {{MODERATORS_DESCRIPTION: p. ej.,
«moderadores voluntarios designados por el operador»}}. No filtramos el
contenido de antemano ni tenemos una obligación general de vigilarlo, pero
actuamos ante las denuncias y ante el contenido del que tengamos
conocimiento.

6.2. **Medidas posibles.** Según la gravedad y la reincidencia, podemos:
dar una advertencia; marcar contenido multimedia como sensible; añadir una
advertencia de contenido; retirar contenido; limitar (silenciar) una
cuenta; congelarla; suspenderla; o borrarla definitivamente. El contenido
retirado por los moderadores se oculta a todo el mundo y se conserva
{{REMOVED_CONTENT_DAYS}} días para que una apelación aceptada pueda
restablecerlo; después se borra. Cuando la ley lo exija, también podemos
conservar información y comunicarla a las autoridades (véase la Política
de privacidad).

6.3. **Aviso y apelación.** Salvo cuando sea necesaria una medida inmediata
(contenido ilegal grave, spam, amenaza a la seguridad o exigencia legal),
le diremos qué medida se tomó y por qué, en la interfaz o por correo.
Puede apelar **en un plazo de {{APPEAL_DAYS}} días** con la función de
apelación o en {{MODERATION_EMAIL}}. Cuando sea posible, otro moderador
revisará la apelación. Intentamos responder en un plazo de
{{APPEAL_RESPONSE_DAYS}} días.

6.4. **Contenido de otros servidores y federación.** Podemos ocultar,
filtrar, retirar de esta Instancia o negarnos a recibir contenido o cuentas
de otros servidores, y podemos limitar o bloquear («desfederar») servidores
enteros, para proteger a nuestros miembros o cumplir la ley. En
{{BLOCKLIST_URL}} se publica una lista de servidores moderados [OPCIONAL:
con los motivos, salvo cuando publicarlos causaría daño]. La
desfederación puede cortar sus relaciones de seguimiento con cuentas de
esos servidores; esto no es un incumplimiento de estas Condiciones.

6.5. **Denuncias.** Los miembros pueden denunciar contenido o cuentas.
Cuando denuncia contenido de otro servidor, puede elegir reenviar una copia
anonimizada de la denuncia a los moderadores de ese servidor. No abuse del
sistema de denuncias.

## 7. Reclamaciones de derechos de autor y propiedad intelectual

7.1. **Canadá (régimen de aviso y aviso).** Si es titular de derechos de
autor y cree que un contenido de la Instancia los infringe, puede enviar a
{{COPYRIGHT_EMAIL}} un aviso conforme al artículo 41.26 de la *Ley de
derechos de autor* (R.S.C. 1985, c. C-42), que incluya: su nombre y
dirección; la obra afectada; la ubicación (URL) de la presunta infracción;
la infracción alegada; y la fecha y hora de la presunta infracción. Cuando
se aplique esa ley, reenviaremos el aviso al miembro afectado como exige
la ley.

7.2. **Retirada.** Con independencia de 7.1, podemos retirar contenido que
creamos razonablemente que infringe derechos de propiedad intelectual,
conforme a la sección 6 (incluidos el aviso y la apelación).

7.3. [OPCIONAL — Estados Unidos] Para acogerse al puerto seguro de la DMCA
estadounidense (17 U.S.C. § 512), debe registrar un agente designado ante
la U.S. Copyright Office y publicar aquí un procedimiento completo de aviso
de retirada y contraaviso. {{DMCA_AGENT_DETAILS}}

7.4. **Infractores reincidentes.** Suspenderemos las cuentas de los
miembros que infrinjan repetidamente los derechos de propiedad intelectual
de otros.

## 8. Privacidad e información personal

8.1. Cómo recogemos, usamos, comunicamos y conservamos la información
personal se describe en la **Política de privacidad** ({{PRIVACY_URL}}).
La persona responsable de la protección de la información personal es
{{PRIVACY_OFFICER_NAME_OR_TITLE}}, {{PRIVACY_EMAIL}}.

8.2. Su perfil público, sus publicaciones públicas y la lista de sus
seguidores y seguidos (salvo que la oculte en sus ajustes) son
**públicos** y se federan.

8.3. Tiene derechos de acceso y rectificación y, cuando proceda, el
derecho a la desindexación o el borrado de información y a la
portabilidad de los datos, conforme a la *Ley sobre la protección de la
información personal en el sector privado* de Quebec y otras leyes
aplicables. Puede ejercerlos en los ajustes de su cuenta (exportación,
edición, borrado) o en {{PRIVACY_EMAIL}}. Puede presentar una
reclamación ante la Commission d'accès à l'information de Quebec.

8.4. **Acceso de los administradores.** Por seguridad, cumplimiento legal y
gestión de denuncias, los administradores pueden acceder técnicamente a
los datos guardados en la Instancia, incluidos los mensajes directos. Ese
acceso se limita a lo necesario para esos fines, {{LOGGED: «queda
registrado» / «no queda registrado»}} y nunca se usa para otro fin. Un
mensaje que usted denuncia se muestra a los moderadores con su denuncia.

## 9. Migración, exportación y borrado de la cuenta

9.1. **Exportación.** Puede exportar sus datos (archivo de publicaciones,
seguidos, bloqueos, silenciados, listas, marcadores) en cualquier momento
en los ajustes, con límites de frecuencia razonables.

9.2. **Migración.** Puede trasladar su cuenta a otro servidor con la
función de migración del programa, que redirige a sus seguidores. No
bloquearemos una migración salvo que su cuenta esté suspendida por una
infracción grave o una obligación legal lo impida.

9.3. **Borrado por usted.** Puede borrar su cuenta en cualquier momento en
los ajustes. El borrado se realiza {{DELETION_DELAY: p. ej., «de
inmediato» / «tras un periodo de gracia de 30 días durante el que puede
cancelarlo»}}. Su nombre de usuario puede reservarse de forma permanente
para evitar suplantaciones. Las copias federadas se tratan como se
describe en 2.4.

9.4. **Terminación por nuestra parte.** Podemos suspender o terminar su
cuenta por un incumplimiento de estas Condiciones o de las Normas,
conforme a la sección 6. Antes de borrar definitivamente una cuenta en
regla por motivos ajenos a un incumplimiento (por ejemplo, el cierre de la
Instancia o una larga inactividad), daremos un aviso de al menos
**{{NOTICE_DAYS}} días** por correo, para que pueda exportar sus datos o
migrar. **Cuentas inactivas:** {{INACTIVITY_POLICY: p. ej., «las cuentas
sin inicio de sesión en 24 meses pueden borrarse tras un aviso»}}.

9.5. **Cierre de la Instancia.** Si decidimos cerrar la Instancia, daremos
un aviso de al menos **{{SHUTDOWN_NOTICE_DAYS}} días** (recomendado: 90) en
la Instancia y por correo, mantendremos la exportación y la migración
disponibles durante ese periodo y, cuando sea posible, enviaremos avisos de
borrado de cuentas a los servidores federados.

## 10. Disponibilidad del servicio

10.1. La Instancia la gestionan {{«voluntarios» / «sin fines de lucro»}}
con recursos limitados. Haremos esfuerzos razonables para mantenerla
disponible y segura y para hacer copias de seguridad, pero no prometemos
un servicio ininterrumpido ni sin errores. Podemos hacer mantenimiento,
actualizar el programa, o cambiar, añadir o quitar funciones.

10.2. **Incidentes de seguridad.** Si un incidente de confidencialidad que
afecte a su información personal presenta un riesgo de perjuicio grave, se
lo notificaremos a usted y a la Commission d'accès à l'information como
exige la ley.

## 11. Responsabilidad

11.1. **Contenido de otras personas.** El contenido publicado por otros
miembros o que llega de otros servidores es responsabilidad de sus
autores. No somos responsables de él, salvo en la medida en que la ley nos
haga responsables una vez que conocemos su carácter ilícito y no actuamos
con prontitud (véase, en Quebec, la *Ley sobre el marco jurídico de las
tecnologías de la información*, art. 22).

11.2. **Limitación, en la medida permitida por la ley.** El servicio se
presta gratuitamente. En la medida en que lo permita la ley aplicable, el
operador no responde de pérdidas indirectas ni de pérdidas causadas por
hechos fuera de nuestro control razonable (p. ej., caídas de proveedores de
alojamiento, acciones de otros servidores, fuerza mayor).

11.3. **Lo que esta sección no limita.** **Nada en estas Condiciones
excluye ni limita nuestra responsabilidad** por daños corporales o
morales, por daños causados intencionadamente o por culpa grave, ni en
cualquier otro caso en que la ley no permita la exclusión o la limitación.
Si es consumidor en el sentido de la *Ley de protección del consumidor* de
Quebec o de una legislación similar, nada en estas Condiciones le priva de
los derechos que esa legislación le da.

11.4. **Su responsabilidad.** Responde, según las reglas generales del
derecho, de los daños que cause al incumplir estas Condiciones o la ley.
[Conforme al derecho del consumo: no se impone a los miembros ninguna
cláusula de indemnización general.]

## 12. Cambios en estas Condiciones

12.1. Podemos modificar estas Condiciones, por ejemplo, para reflejar
cambios en la ley, en el programa o en nuestras prácticas de moderación.
Las versiones anteriores se archivan en {{CHANGELOG_URL}}.

12.2. **Aviso previo.** Para los cambios que no sean correcciones menores,
le avisaremos **al menos 30 días** antes de que entren en vigor, por
correo y con un anuncio en la Instancia. El aviso indicará la cláusula
nueva o modificada, la cláusula tal como estaba, la fecha de entrada en
vigor y su derecho a rechazarla.

12.3. **Su derecho a rechazar.** Si no acepta los cambios, puede cerrar su
cuenta (y exportarla o migrarla) sin coste antes de que entren en vigor.
Si sigue usando su cuenta después de la fecha de entrada en vigor, se
aplican las nuevas Condiciones.

## 13. Conflictos y ley aplicable

13.1. **Hable primero con nosotros.** La mayoría de los problemas se
resuelven de manera informal: escriba a {{CONTACT_EMAIL}}. Esto no es un
requisito previo para ejercer sus derechos.

13.2. **Ley aplicable.** Estas Condiciones se rigen por las leyes vigentes
en la provincia de Quebec y las leyes federales de Canadá aplicables allí.
Si es un consumidor que reside en otro lugar, esta elección de ley no le
priva de la protección de las normas imperativas de su lugar de
residencia.

13.3. **Tribunales.** Los conflictos se someten a los tribunales
competentes del distrito judicial de {{JUDICIAL_DISTRICT: p. ej.,
«Montreal»}}, Quebec, **sin perjuicio de su derecho como consumidor a
demandar ante los tribunales de su domicilio** (C.C.Q. art. 3149). Las
reclamaciones dentro del límite de la División de reclamaciones menores
del Tribunal de Quebec pueden presentarse allí.

13.4. **Sin arbitraje obligatorio ni renuncia a acciones colectivas.**
Nada en estas Condiciones le obliga a someter un conflicto a arbitraje ni
restringe su derecho a demandar o a formar parte de una acción colectiva.

## 14. Disposiciones generales

14.1. **Acuerdo completo.** Estas Condiciones y los documentos enumerados
en 1.4 forman el acuerdo completo entre usted y el operador sobre la
Instancia.

14.2. **Divisibilidad.** Si alguna disposición se considera nula o
inaplicable, el resto de estas Condiciones sigue vigente, y la disposición
se aplica en la mayor medida que permita la ley.

14.3. **Sin renuncia.** Que no hagamos cumplir una disposición no supone
renunciar a nuestro derecho a hacerla cumplir más adelante.

14.4. **Cesión.** Podemos transferir la operación de la Instancia a otro
operador (por ejemplo, una organización sin fines de lucro) que acepte
quedar obligado por estas Condiciones y la Política de privacidad; le
avisaremos al menos 30 días antes para que pueda cerrar o migrar su cuenta
si no está de acuerdo.

14.5. **Avisos.** Enviamos los avisos a la dirección de correo asociada a
su cuenta. Usted nos envía los suyos a {{CONTACT_EMAIL}}.

---

## [OPCIONAL] Anexo A — Miembros en la Unión Europea / el Reino Unido

Si reside en la UE o en el Reino Unido, el RGPD / UK GDPR puede aplicarse a
nuestro tratamiento de su información personal; sus derechos y nuestras
bases jurídicas se describen en la Política de privacidad. Nuestro
representante en la UE (art. 27 del RGPD), si es necesario:
{{EU_REPRESENTATIVE}}. Puede presentar una reclamación ante su autoridad
local de protección de datos. Las protecciones imperativas del consumidor
de su país de residencia siguen aplicándose. Nota: las instancias pequeñas
con miembros en la UE también pueden estar sujetas a la Ley de Servicios
Digitales (Reglamento (UE) 2022/2065), incluidas las obligaciones de tener
un punto de contacto y de motivar las decisiones de moderación.

## [OPCIONAL] Anexo B — Miembros en Estados Unidos

Los miembros en Estados Unidos deben tener al menos 13 años (COPPA) o la
edad mínima de la sección 3.1, la que sea mayor. Véase la sección 7.3 para
los avisos de derechos de autor.

---

*Este documento se basa en una plantilla abierta de {{GITHUB_REPO_URL}},
publicada bajo {{TEMPLATE_LICENSE: p. ej., «CC BY-SA 4.0» / «CC0»}}.*
