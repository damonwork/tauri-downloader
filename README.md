# Fluxor

Gestor de descargas de escritorio construido con **Tauri 2, Rust, Vue 3 y TypeScript**.
Su objetivo es ofrecer descargas segmentadas, reanudación segura, credenciales por enlace,
importación de `curl` y perfiles proxy con una interfaz sencilla.

## Estado actual

El repositorio contiene un MVP funcional:

- [x] Alta mediante URL HTTP/HTTPS.
- [x] Parser seguro de `curl` para GET/HEAD, headers, cookies, User-Agent, Referer y proxy.
- [x] Cola nativa con límite global de descargas simultáneas.
- [x] Límite de 1 a 32 segmentos con degradación automática a un flujo si el servidor rechaza rangos o conexiones paralelas.
- [x] Escritura incremental a disco con memoria acotada al bloque de red actual.
- [x] Pausar, reanudar, reiniciar, reintentar manualmente y eliminar.
- [x] Reanudación validada con `Range` y `Content-Range`, más ETag o Last-Modified cuando están disponibles.
- [x] Indicador de reanudación con estado independiente del validador de identidad.
- [x] Enlaces firmados con negociación real: si el servidor rechaza HEAD, se comprueba con un `GET` de un byte por rango; si acepta rangos se segmenta y reanuda con normalidad (p. ej. CloudFront/HuggingFace), y si es estricto se degrada a un solo flujo con `Range: bytes=0-` (CDN de un solo uso).
- [x] Reemplazo de enlaces vencidos sin perder segmentos parciales compatibles.
- [x] Headers y cookies específicos por descarga.
- [x] Perfiles proxy HTTP, HTTPS y SOCKS5 con comprobación de salud.
- [x] Carpeta raíz configurable y subcarpetas por categoría dentro de Descargas del sistema.
- [x] Configuración individual por descarga: categoría, ruta, segmentos, proxy y credenciales.
- [x] Límite de velocidad por descarga con valor decimal y unidades KB/s, MB/s o GB/s, aplicado al tráfico agregado de todos sus segmentos.
- [x] Cambio del límite de velocidad de una descarga activa: se aplica al instante a los segmentos en curso y se conserva al reanudar o reiniciar.
- [x] Persistencia local atómica de cola, ajustes y perfiles.
- [x] Registro de diagnóstico persistente: eventos del puente, la cola y las transferencias, con filtro por nivel, copiar y limpiar desde Preferencias.
- [x] "Mostrar en carpeta" en la descarga completada, con apertura segura en Windows, macOS y Linux.
- [x] Puente local seguro (127.0.0.1:17846) con token para la extensión del navegador.
- [x] Interfaz responsive con búsqueda, filtros, inspector y atajos.
- [x] Vista web persistente para probar la UX sin ejecutar transferencias reales.
- [x] Extensión de navegador Chrome/Firefox con captura de descargas, medios y envío directo a la cola.
- [ ] Reintentos automáticos con backoff y clasificación de errores.
- [ ] Verificación SHA-256 al completar.
- [ ] Integración con el almacén seguro del sistema para secretos persistentes.
- [ ] Auto-update, bandeja del sistema y notificaciones nativas.

## Separación de capacidades

La UI se comparte entre navegador y Tauri, pero las capacidades no se simulan como si fueran
equivalentes:

| Capacidad | Vista web | Tauri |
|---|---:|---:|
| Gestionar y persistir una cola de demostración | Sí | Sí |
| Parsear URL y `curl` | Sí | Sí |
| Descargar sin CORS | No | Sí |
| Enviar cookies y headers arbitrarios | No | Sí |
| Usar proxies | Configuración solamente | Sí |
| Reanudar después de reiniciar el proceso | Simulado | Sí |

Las transferencias reales siempre recaen en Rust. La vista web existe para desarrollo,
documentación y futuras integraciones con un servicio compañero autenticado.

## Arquitectura

```text
Vue components
    |
    v
useDownloadManager (casos de uso y estado de presentación)
    |
    v
DownloadGateway
    |-- WebDownloadGateway   (preview persistente, sin red real)
    `-- TauriDownloadGateway (IPC tipado)
              |
              v
      async Tauri commands
              |
              v
       DownloadManager       (cola, estado y persistencia)
              |
              v
       DownloadEngine        (HTTP, rangos, segmentos y archivos)
```

Los DTO usan estados discriminados en lugar de combinaciones ambiguas de campos opcionales.
Por ejemplo, `DownloadState::Failed` siempre contiene su mensaje y `ProxyHealth::Online`
siempre contiene la latencia. Consulta [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Desarrollo

Requisitos para la interfaz:

```bash
npm ci
npm run dev
npm run build
npm test
```

Requisitos adicionales para escritorio: Rust estable y las dependencias de sistema indicadas
por Tauri 2.

```bash
npm run tauri dev
npm run tauri build
```

## Validación sin Rust local

La vista de producción puede ejecutarse en la red Docker externa `proxy` sin publicar puertos:

```bash
docker compose -f compose.preview.yaml up --build -d
```

Otros contenedores de esa red pueden acceder a `http://fluxor-preview:4173`. El servicio no
define `ports` ni `expose`, por lo que no queda accesible directamente desde el host.

Para validar Rust sin tenerlo instalado, el script `scripts/test-rust-ci.sh` levanta un
contenedor efímero (Dockerfile.rust-ci) y ejecuta formato, Clippy con `-D warnings`, los tests
y `cargo check` dentro de él; no deja imágenes ni volúmenes:

```bash
./scripts/test-rust-ci.sh
```

La extensión del navegador se valida con `npm run verify:extension`: comprueba la sintaxis de
todos los scripts, simula la carga de módulos en el orden de Chrome (`importScripts`) y de
Firefox (`background.scripts`) y verifica los casos de nombrado de archivo espejo de los tests
Rust.

## Integración continua

- `quality.yaml` valida frontend (build, tests, versiones y `verify:extension`), formato Rust, Clippy y `cargo check`.
- `release.yaml` se ejecuta manualmente desde GitHub Actions y crea el tag a partir de la versión del proyecto.
- Windows genera instaladores NSIS `.exe` y MSI.
- Linux genera AppImage y deb.
- macOS genera DMG/app para Intel y Apple Silicon.
- Los artefactos temporales del workflow se conservan 2 días.
- Los releases se crean como borradores para poder firmar/revisar antes de publicarlos.

## Seguridad

- El texto `curl` se interpreta como datos y nunca se entrega a un shell.
- Las solicitudes usan un User-Agent de navegador y `Accept-Encoding: identity`; un User-Agent pegado en `curl` tiene prioridad.
- Los headers `Cookie` pegados desde el navegador se convierten al almacén de cookies y los headers de transporte se ignoran.
- Se rechazan opciones que leen archivos locales o envían cuerpos/formularios.
- Rust vuelve a validar URLs, nombres, headers, cookies, proxies y rutas.
- Los eventos de progreso solo contienen la revisión del snapshot, nunca secretos.
- Los errores de red no incluyen la URL solicitada.
- Una respuesta reanudada debe ser `206` y coincidir con el rango exacto solicitado.
- Pausar conserva los parciales; si el servidor no permite reanudar, la UI advierte que será necesario reiniciar.
- Sin ETag o Last-Modified, un parcial solo se anexa tras confirmar `206`, rango exacto y tamaño compatible.
- Las redirecciones se desactivan cuando la solicitud contiene headers personalizados distintos de `Referer` o `User-Agent`; las cookies se eliminan automáticamente al cambiar de host.
- Dos descargas no pueden reservar el mismo archivo de destino.
- Los archivos se descargan como parciales ocultos y se renombran al finalizar.

Actualmente el estado se guarda en el directorio privado de datos de la aplicación como JSON.
Esto permite recuperar cookies y headers después de reiniciar, pero todavía no cifra secretos
en reposo. No debe considerarse adecuado para equipos compartidos hasta integrar el almacén
seguro del sistema.

## Roadmap

1. Pruebas HTTP de integración con servidor controlado: rangos, desconexiones y cambio de ETag.
2. Reintentos automáticos con backoff sin ocupar slots de concurrencia.
3. Checksums, límites por host y rotación de proxies.
4. Almacén seguro de credenciales y migraciones de persistencia.
5. Auto-update y mejoras de distribución de la extensión.

## Extensión del navegador

El workflow de release adjunta `fluxor-extension-chrome-vX.Y.Z.zip` y
`fluxor-extension-firefox-vX.Y.Z.zip` al borrador del release. La extensión usa
WebExtensions compatibles con Chrome y Firefox y está organizada en módulos bajo
`browser-extension/lib/` (`log`, `url`, `naming`, `store`, `capture`) con un
`background.js` como punto de entrada; Chrome los carga con `importScripts` y
Firefox vía `background.scripts` del manifest.

### Funciones de la extensión

- **Captura de descargas del navegador**: intercepta las descargas nativas
  (`downloads.onCreated`), las envía a Fluxor y cancela la copia del navegador.
  Aplica también al enlace final tras redirecciones y a archivos genéricos
  (ZIP, PDF, artifacts, etc.).
- **Detección de medios**: observa las respuestas multimedia (`onHeadersReceived`)
  y registra candidatos con su nombre resuelto, estado HTTP y cabeceras; evita
  repetir la misma detección mientras el reproductor reutilice el mismo enlace.
- **Botón sobre `<video>`**: añade "Descargar con Fluxor" encima de los
  reproductores para enviar el vídeo activo directamente.
- **Menú contextual**: con el botón derecho sobre un enlace (o un botón envuelto
  en un enlace) aparece "Descargar con Fluxor", que envía esa URL a la cola
  con las cookies, el Referer y el User-Agent de la pestaña actual; también
  aparece sobre elementos de audio y vídeo.
- **Nombres de archivo inteligentes**: prioriza el `Content-Disposition`, luego el
  título de la página para episodios (`Ver episodio N de X - Fansub` y
  `X Episodio N`), luego el nombre de la URL y el slug de la página; siempre
  devuelve un nombre sanitizado y nunca aplica el título sobre nombres reales.
- **Artifacts de GitHub Actions**: la respuesta del blob trae el nombre real en
  `Content-Disposition`, pero Chrome dispara `downloads.onCreated` con el
  nombre provisional del blob (hash) antes de recibir los headers. La
  extensión lee el nombre de los headers de respuesta (mecanismo genérico
  para cualquier sitio) y además intercepta el clic en el icono de descarga
  (como IDM, el único sitio donde el nombre existe antes de la respuesta),
  por lo que el artifact llega con su nombre real, p. ej.
  `fluxor-browser-extensions.zip`.
- **Contexto real de la petición**: captura cookies, Referer y User-Agent de la
  solicitud original (`onBeforeSendHeaders`) y los reutiliza para la descarga.
- **Enlaces firmados**: detecta URLs firmadas (Policy, Signature, X-Amz, etc.) y
  fuerza el flujo único con `Range: bytes=0-`, sin segmentar ni prometer pausa.
- **Deduplicación de envíos**: ignora el mismo enlace durante 15 segundos y
  extiende la ventana tras un envío exitoso para evitar descargas duplicadas.
- **Diagnóstico en el popup**: registro de eventos persistente con nivel
  (Debug/Info/Aviso/Error), filtro, copiar y limpiar; la URL se muestra sin
  tokens ni firmas.
- **Historial de capturas**: lista de candidatos con nombre, estado del envío y
  errores, con botón para limpiarla.
- **Prueba de conexión y token**: comprueba el puente local desde el popup y
  guarda el token de autorización.

Para conectarla, abre Preferencias en Fluxor, copia el token del puente local y
pégalo en el popup de la extensión. El puente solo escucha en `127.0.0.1` y las
operaciones de escritura requieren ese token.

Chrome permite instalar el ZIP desde `chrome://extensions` activando el modo de
desarrollador y usando "Cargar descomprimida" tras extraerlo. Firefox permite
cargar el contenido temporalmente desde `about:debugging`; la instalación
permanente requiere firmar el complemento mediante Mozilla Add-ons.
