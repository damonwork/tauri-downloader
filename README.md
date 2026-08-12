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
- [x] Reemplazo de enlaces vencidos sin perder segmentos parciales compatibles.
- [x] Headers y cookies específicos por descarga.
- [x] Perfiles proxy HTTP, HTTPS y SOCKS5.
- [x] Carpeta raíz configurable y subcarpetas por categoría dentro de Descargas del sistema.
- [x] Configuración individual por descarga: categoría, ruta, segmentos, proxy y credenciales.
- [x] Límite de velocidad por descarga con valor decimal y unidades KB/s, MB/s o GB/s, aplicado al tráfico agregado de todos sus segmentos.
- [x] Persistencia local atómica de cola, ajustes y perfiles.
- [x] Interfaz responsive con búsqueda, filtros, inspector y atajos.
- [x] Vista web persistente para probar la UX sin ejecutar transferencias reales.
- [ ] Reintentos automáticos con backoff y clasificación de errores.
- [ ] Verificación SHA-256 al completar.
- [ ] Integración con el almacén seguro del sistema para secretos persistentes.
- [x] Extensión de navegador Chrome/Firefox con captura de descargas y medios.
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

Para validar Rust en un contenedor Debian se requieren las dependencias de Tauri/WebKitGTK y
estos comandos dentro de `src-tauri/`:

```bash
cargo fmt --all --check
cargo check --locked
cargo test --locked --lib
```

## Integración continua

- `quality.yaml` valida frontend, pruebas, versiones, formato Rust, Clippy y `cargo check`.
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
WebExtensions compatibles con Chrome y Firefox: captura descargas del navegador,
observa respuestas multimedia y puede añadir un botón sobre elementos `<video>`.

Para conectarla, abre Preferencias en Fluxor, copia el token del puente local y
pégalo en el popup de la extensión. El puente solo escucha en `127.0.0.1` y las
operaciones de escritura requieren ese token.

Chrome permite instalar el ZIP desde `chrome://extensions` activando el modo de
desarrollador y usando "Cargar descomprimida" tras extraerlo. Firefox permite
cargar el contenido temporalmente desde `about:debugging`; la instalación
permanente requiere firmar el complemento mediante Mozilla Add-ons.
