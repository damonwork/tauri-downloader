# Arquitectura de Fluxor

## Objetivos

1. Mantener la red y el sistema de archivos fuera de la UI.
2. Evitar que el navegador imponga CORS o restricciones de headers al motor real.
3. Limitar memoria y concurrencia de forma explícita.
4. Poder pausar y recuperar una descarga sin mezclar contenido de fuentes distintas.
5. Compartir contratos entre la vista web de desarrollo y Tauri sin fingir paridad.

## Capas

### Presentación

`src/components/` contiene componentes Vue sin acceso directo a IPC, red o almacenamiento.
`src/App.vue` compone las vistas y traduce eventos de usuario en casos de uso.

### Aplicación

`src/application/download-gateway.ts` define el contrato que necesita la UI.
`use-download-manager.ts` mantiene el snapshot, errores operativos y sincronización de eventos.
No contiene reglas HTTP ni conoce detalles de Tauri.

### Dominio

`src/domain/` contiene modelos y funciones puras: estados, progreso, clasificación de archivo,
formato e interpretación segura de `curl`. Los estados variables son uniones discriminadas.

### Infraestructura web

`WebDownloadGateway` ofrece una simulación persistente para validar interacción y diseño. No usa
`fetch` para descargas, porque no podría prometer cookies arbitrarias, proxies ni acceso estable
al sistema de archivos.

### Infraestructura Tauri/Rust

Los comandos de `commands.rs` son adaptadores delgados. `DownloadManager` es la autoridad sobre
la cola, tareas activas, cancelación y persistencia. `DownloadEngine` solo se ocupa de HTTP,
rangos y archivos.

## Concurrencia

- `DownloadManager` mantiene un límite global configurable de archivos activos.
- Cada archivo configura un máximo de 1 a 32 segmentos; el motor usa menos según tamaño y soporte del servidor.
- Cada segmento recibe un rango no solapado y escribe en su propio archivo parcial.
- Si el servidor rechaza el protocolo de rangos, se descartan esos parciales y se continúa con un único flujo.
- El límite de velocidad se comparte entre todos los segmentos de una descarga, por lo que no se multiplica al aumentar conexiones.
- `CancellationToken` coordina pausa, reinicio y borrado.
- `RwLock` protege el snapshot; no se mantiene bloqueado durante red o disco.
- `Mutex` protege únicamente el mapa pequeño de tareas activas.
- Los bloques recibidos se escriben inmediatamente; nunca se acumula el archivo en memoria.
- El progreso viaja por un canal no bloqueante y la persistencia se agrupa por revisiones.
- Las tareas en cancelación siguen ocupando un slot hasta cerrar sus archivos.

## Reanudación e identidad

Un parcial solo puede anexarse cuando se cumplen estas condiciones:

1. El tamaño real del parcial en disco es el punto de inicio autoritativo.
2. Una reanudación devuelve `206 Partial Content`.
3. `Content-Range` comienza y termina exactamente donde se solicitó.
4. ETag o Last-Modified coincide cuando el servidor proporciona un validador.
5. La cantidad final de bytes coincide con el tamaño conocido.
6. Existe un ETag o Last-Modified durable antes de anexar un parcial.

Los cortes de red conservan los segmentos para reintentar. Solo un rechazo confirmado del
protocolo de rangos activa la degradación a un flujo, evitando borrar progreso por un fallo transitorio.

La UI solo marca una descarga como reanudable cuando el servidor confirma rangos y entrega un
ETag fuerte o Last-Modified. Un ETag débil no se usa con If-Range. Si faltan esas garantías, se
muestra el motivo y una descarga parcial solo puede reiniciarse desde cero.

## Persistencia

El snapshot se serializa a un archivo temporal, se sincroniza y se renombra sobre `state.json`.
Las escrituras pasan por un mutex exclusivo y mantienen un backup recuperable en plataformas
donde reemplazar un archivo existente no es atómico.
Una descarga que estaba activa al cerrarse se recupera como `queued`. Los eventos enviados a la
vista solo indican la revisión; la vista solicita después un snapshot consistente.

JSON es suficiente para este MVP y mantiene el diseño simple. Si se requieren consultas de
historial extensas o migraciones complejas, el contrato del manager permite sustituirlo por
SQLite sin modificar Vue ni el motor HTTP.

## Seguridad

- Solo se admiten esquemas HTTP y HTTPS para fuentes.
- No se ejecuta el contenido de `curl`.
- Se rechazan CR/LF en headers y cookies.
- `Range`, `If-Range`, `Content-Length`, `Content-Range` y `Cookie` son controlados por el motor.
- Las rutas relativas con `..` se rechazan.
- Los nombres de archivos no pueden incluir separadores.
- Los errores de reqwest se convierten en mensajes sin URL.
- Las solicitudes con credenciales no siguen redirecciones automáticamente.
- La conexión directa desactiva explícitamente proxies heredados del sistema.
- La finalización usa un hard link sin reemplazo para no sobrescribir archivos existentes.
- La CSP de Tauri y la del preview restringen scripts y conexiones.

## Extensión

Para añadir otro runtime se implementa `DownloadGateway`. Para añadir una estrategia de
almacenamiento nativo se extraerá un trait solo cuando exista una segunda implementación real.
Se evita crear interfaces preventivas alrededor de funciones que ya tienen una responsabilidad
única.
