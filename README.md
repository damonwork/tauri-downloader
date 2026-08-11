# tauri-downloader

> Un gestor de descargas moderno, robusto y fácil de usar — construido con **Tauri** (Rust + Web).
> Pensado como un sucesor espiritual de IDM, pero multiplataforma, extensible y sin la fricción típica.

---

## Visión

Ser el **"IDM, pero bien hecho"**: una sola herramienta que sirva tanto desde el navegador
(como extensión / web app) como desde el escritorio (instalador nativo), con la potencia de
**descargas multi-hilo, reanudar descargas, cookies por enlace, soporte para pegar `curl`,
rotación de proxies y recuperación de enlaces vencidos**, sin obligar al usuario a pelearse
con la configuración.

### Principios de diseño

1. **Robusto por defecto** — las descargas no se rompen: reanuda, valida checksums, refresca enlaces.
2. **Sencillo por encima** — el 90 % de los casos funciona con *pegar URL → Enter*.
3. **Avanzado cuando se necesita** — configuración por enlace, perfiles, scripts, proxies.
4. **Local-first** — todo el estado vive en el equipo del usuario; sin servidor propio obligatorio.
5. **Misma UX en web y escritorio** — Tauri permite compartir el frontend entre navegador y binario.

---

## Funcionalidades objetivo

### Núcleo
- [ ] Añadir descargas pegando una **URL** directa.
- [ ] Pegar un comando **`curl`** completo y extraer URL, headers, cookies y método automáticamente.
- [ ] Importar una lista de URLs desde texto / archivo.
- [ ] Captura desde el portapapeles (detección automática de enlaces y `curl`).
- [ ] Descargas **multi-hilo** (configurable: 1–32 hilos por archivo).
- [ ] **Reanudar** descargas parciales con validación de rangos HTTP (`Range`).
- [ ] Cola de descargas con prioridades, reintentos y *backoff* exponencial.
- [ ] Historial de descargas con búsqueda y filtros.

### Por enlace
- [ ] **Cookies por enlace** — editor visual (pares `nombre=valor`) + importar desde `curl`/Netscape.
- [ ] **Headers personalizados** por enlace (User-Agent, Referer, Authorization, …).
- [ ] **Refrescar enlace** cuando vence (re-detectar mirror / regenerar URL firmada).
- [ ] Renombrado de archivo destino con plantillas (`{host}`, `{date}`, `{n}`, …).
- [ ] Hash de verificación (MD5/SHA1/SHA256) automático al terminar.

### Red
- [ ] **Lista de proxies** configurable (HTTP, HTTPS, SOCKS5).
- [ ] Asignación de proxy por enlace o por perfil (directo / sistema / lista rotativa).
- [ ] Reglas tipo "si el host coincide con X, usa proxy Y".
- [ ] Detección de proxy del sistema (Windows / macOS / Linux).

### Interfaz
- [ ] UI web moderna (React + TypeScript) servida por Tauri.
- [ ] Tema claro / oscuro.
- [ ] Vista de cola, vista de activos, vista de historial.
- [ ] Notificaciones nativas al terminar / fallar.
- [ ] Atajos de teclado y *drag & drop* de archivos/URLs.

### Integración navegador (fase 2)
- [ ] Extensión Chrome/Firefox para "Enviar a tauri-downloader".
- [ ] Web app standalone que habla con el backend Tauri por WebSocket.

---

## Stack técnico

| Capa        | Tecnología                                         |
|-------------|----------------------------------------------------|
| Backend     | **Rust** + Tauri 2 + `reqwest` + `tokio`           |
| Frontend    | **React** + TypeScript + Vite + TailwindCSS        |
| Estado      | Zustand / TanStack Query                           |
| Persistencia| SQLite (vía `rusqlite` o `sqlx`) en `app_data_dir` |
| Empaquetado| Tauri bundler (deb, rpm, msi, dmg, AppImage)       |
| CI          | GitHub Actions (build multi-OS + release)          |

---

## Arquitectura (resumen)

```
┌─────────────────────────────────────────────────────┐
│                   Frontend (Web)                    │
│  React UI  ──►  Tauri Commands  ──►  Tauri Events   │
└────────────────────────┬────────────────────────────┘
                         │  IPC (Tauri)
┌────────────────────────▼────────────────────────────┐
│                 Backend (Rust)                      │
│  ┌──────────┐  ┌──────────�  ┌───────────────────┐  │
│  │ Download │  │  Queue   │  │  Proxy / Cookie   │  │
│  │  Engine  │◄─┤ Manager  │◄─┤   Resolver        │  │
│  └────┬─────�  └──────────┘  └───────────────────┘  │
│       │                                              │
│  ┌────▼─────┐  ┌──────────┐  ┌───────────────────┐  │
│  │ Chunked  │  │ Refresh  │  │   SQLite Store    │  │
│  │ Writer   │  │  Policy  │  │  (history,queue)  │  │
│  └──────────┘  └──────────┘  └───────────────────┘  │
└─────────────────────────────────────────────────────┘
```

---

## Estructura del repositorio (planeada)

```
tauri-downloader/
├── src-tauri/           # Backend Rust (Tauri)
│   ├── src/
│   │   ├── main.rs
│   │   ├── download/    # motor de descarga, chunks, resume
│   │   ├── queue/       # cola, prioridades, scheduler
│   │   ├── net/         # proxy, cookies, refresh
│   │   ├── curl/        # parser de curl
│   │   └── store/       # SQLite, migraciones
│   └── tauri.conf.json
├── src/                 # Frontend React
│   ├── components/
│   ├── pages/
│   ├── hooks/
│   └── store/
├── public/
├── docs/                # arquitectura, decisiones, capturas
├── .github/workflows/   # CI
├── README.md
└── LICENSE
```

---

## Cómo empezar (una vez inicializado Tauri)

```bash
# requisitos: rustup, node ≥ 18, pnpm
pnpm install
pnpm tauri dev      # modo desarrollo
pnpm tauri build    # generar instaladores
```

---

## Roadmap

- **v0.1 — MVP escritorio**: pegar URL / curl, descarga simple, reanudar, SQLite básico.
- **v0.2 — Multi-hilo y cookies por enlace**.
- **v0.3 — Proxies + perfiles**.
- **v0.4 — Extensión de navegador + web app**.
- **v1.0 — Release multiplataforma con auto-update**.

---

## Contribuir

Aún en fase temprana. Issues y PRs son bienvenidos, pero por favor abrí primero un *issue*
para discutir cambios grandes.

## Licencia

MIT (a confirmar).
