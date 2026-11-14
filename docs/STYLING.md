# Estilos de Fluxor

La aplicación Vue y el popup de la extensión usan Tailwind CSS 4. El tema se configura en CSS mediante `@theme`; no hay un archivo `tailwind.config.js` ni estilos cargados desde un CDN.

## Dónde poner cada cambio

| Ubicación | Responsabilidad |
| --- | --- |
| `src/styles.css` | Entrada de Vite y escaneo limitado al código de la aplicación. |
| `src/styles/theme.css` | Colores semánticos, familias tipográficas, tamaño de texto secundario, puntos de ruptura, animaciones y la cuadrícula compartida de descargas. |
| `src/styles/base.css` | Valores de base, foco visible, barras de desplazamiento y respeto a movimiento reducido. |
| `src/styles/components.css` | Controles repetidos: `ui-button`, `ui-icon-button`, `ui-input`, `ui-label`, navegación y partes de diálogos. También las clases de transición que Vue aplica en ejecución. |
| `src/components/*.vue` | Utilidades de distribución, espaciado y estado propias del componente. |
| `browser-extension/styles.css` | Entrada independiente del popup: comparte el tema y controles, escanea su HTML y JavaScript. |
| `browser-extension/overlay.css` | Botón sobre vídeos. Sin preflight, con tokens prefijados y salida aislada de los estilos de la página visitada. |

Para cambiar los colores del producto, modifica los tokens del tema. Usa `bg-surface`, `text-muted`, `border-line`, `text-danger`, etc. en lugar de repetir valores hexadecimales. Los valores arbitrarios se reservan para geometría específica, gradientes y animaciones que no encajen en la escala habitual.

Usa las primitivas existentes cuando un control tenga el mismo propósito. Por ejemplo:

```html
<button class="ui-button ui-button-primary" type="submit">Guardar cambios</button>
<input class="ui-input" name="file-name" />
```

La distribución de una pantalla o un componente sigue estando junto a su plantilla. No crees una clase global para cada contenedor ni copies un bloque de estilos de otro diálogo. Añade una primitiva con `@apply` solo cuando varias partes de la interfaz necesiten el mismo comportamiento visual.

## Estados y adaptación

- Las clases completas deben aparecer en el código. Evita construir nombres como `bg-${color}`: Tailwind no puede detectar esas cadenas al compilar. Usa un mapa con cadenas completas o variantes de estado.
- Las clases semánticas como `download-row`, `file-copy`, `active` y `failed` también sirven de anclas para pruebas y variantes. No las elimines sin revisar sus referencias.
- `mobile`, `dialog`, `narrow` e `inspector` son puntos de ruptura nombrados en el tema. Las consultas `@max-…/workspace` responden al espacio disponible en el panel, incluso cuando el inspector está abierto.
- `download-columns` conserva alineadas las filas y sus cabeceras. Cambia su definición compartida al ajustar las columnas.
- Los porcentajes de progreso siguen usando `:style` porque son valores calculados en ejecución, no una colección finita de utilidades.
- Los diálogos mantienen fijos cabecera y acciones; su contenido puede desplazarse. Al cambiar un formulario, comprueba tanto pantallas pequeñas como ventanas de poca altura.

## Formato y comprobaciones

```bash
npm ci
npm run format
npm run format:check
npm run build
npx playwright install chromium # una vez, para las pruebas visuales
npm run test:ui
npm test
npm run verify:extension
npm run package:extension
```

Prettier usa `prettier-plugin-tailwindcss` y conoce la entrada CSS del proyecto para ordenar también las clases del tema. CI comprueba el formato y la generación de los paquetes de la extensión.

`tests/ui/` contiene las pruebas de navegador sobre la compilación de producción. `npm test` se limita a las pruebas unitarias en `src/`. En una máquina con Chromium ya instalado se puede ejecutar `PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH=/ruta/a/chromium npm run test:ui`.

Al revisar un cambio visual, comprueba la lista con y sin inspector, el formulario avanzado, las tres pestañas de preferencias, proxies y la confirmación de reinicio. Verifica foco, estados deshabilitados, desplazamiento, movimiento reducido y ausencia de desbordamiento horizontal.

## Extensión del navegador

`npm run build:extension:css` compila `browser-extension/generated/popup.css` y `overlay.css`. Esa carpeta está ignorada por Git. El empaquetado vuelve a generar el CSS y lo incluye en ambos ZIP; elimina las entradas CSS de desarrollo del paquete.

Para trabajar con la extensión descomprimida, ejecuta primero esa compilación o usa `npm run dev:extension:css` en otra terminal. El popup referencia únicamente el CSS generado local. El botón de vídeo se carga desde `content_scripts.css`, sin insertar CSS desde JavaScript. `scripts/scope-extension-overlay.mjs` limita también los valores de base y propiedades internas que genera Tailwind al botón: no se deben aplicar a los controles de la página visitada. Una prueba de navegador comprueba ese aislamiento.

La base CSS de Tailwind 4 requiere Chrome 111+, Firefox 128+ o Safari 16.4+. Los manifests reflejan ese mínimo para Chrome y Firefox.

La aplicación de escritorio conserva la CSP existente; esta migración no necesita permitir hojas externas ni scripts adicionales en ejecución.
