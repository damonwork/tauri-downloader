# Revisión del flujo de descargas

Revisión realizada el 14 de noviembre de 2026 sobre la interfaz web, el código del motor Rust y la extensión. La vista web simula transferencias: sus velocidades no acreditan rendimiento de red real.

## Mejoras aplicadas

- El formulario principal conserva enlace, archivo y categoría. El máximo de segmentos queda en configuración avanzada, con explicación de que depende del servidor. No permite enviar una solicitud vacía o inválida.
- Se aumenta el tamaño de textos en los puntos principales del flujo y el contraste del aviso de modo web.
- El progreso usa porcentajes enteros truncados y reserva el 100 % para el estado completado. Recibir todos los bytes no equivale a terminar de combinar y guardar el archivo.
- El tiempo restante redondea con acarreo de minutos a horas y oculta valores no calculables. Los tamaños y velocidades inválidos no producen NaN/Infinity.
- Fila e inspector comparten la decisión de recuperación: reintentar conserva parciales si se puede continuar; reiniciar solicita confirmación antes de descartarlos cuando no se pueden reanudar. El inspector permite pausar una descarga en cola.
- La vista estrecha conserva velocidad y tiempo restante; el inspector incluye tiempo restante.
- La extensión comparte la promesa de un envío simultáneo. Solo devuelve éxito a las otras capturas cuando el puente lo confirma. Antes, una captura duplicada podía devolver éxito mientras fallaba el envío original, y background.js podía cancelar la descarga del navegador sin haberla entregado a Fluxor.

## Validación realizada

- `npm run build`: comprobación TypeScript y compilación correctas.
- `npm test`: 32 pruebas aprobadas, incluidas regresiones de tiempo restante, valores inválidos, progreso y recuperación.
- `npm run verify:extension`: aprobado. Incluye envíos concurrentes exitosos y fallidos, un único POST al puente y espera de confirmación antes de informar éxito. Es una simulación de las APIs, no una instalación de la extensión en Chrome/Firefox.
- Chromium automatizado a 1440×1000 y 390×844: formulario básico/avanzado, bloqueo de solicitud inválida, 99 % antes de completar, confirmación de reinicio destructivo, conservación del progreso al reintentar y velocidad/tiempo visibles en móvil. Sin errores JavaScript ni desbordamiento horizontal en los tamaños comprobados.

## Correcciones del motor verificadas con HTTP real

- `forceSingleStream` evita HEAD, pruebas de un byte y segmentación. La prueba comprueba una sola petición GET `Range: bytes=0-`.
- El flujo único solicita rangos y solo declara reanudación tras validar 206 y Content-Range. Se comprueban ambos extremos y el tamaño total incluso desde offset cero; no se acepta una fracción como archivo completo.
- Si HEAD falla o no anuncia rangos, se consulta el rango real. Una cabecera Accept-Ranges engañosa no basta para declarar reanudación.
- Se distingue un rechazo de autenticación de un rechazo de rangos. Solo estados HTTP transitorios se marcan recuperables.
- El limitador comparte presupuesto entre segmentos y vuelve a consultar su valor durante las esperas. Subir o quitar un límite ya no deja un bloque esperando según el valor anterior.
- En flujo único se publican muestras mientras el limitador espera y se escriben bloques adaptados al límite, evitando que el progreso se quede inmóvil durante un bloque grande.
- Ante un corte o pausa se vacían las escrituras pendientes de Tokio antes de devolver el resultado. Se detectó y corrigió una carrera en la que el parcial seguía creciendo después del error.
- Los segmentos se detienen mediante cancelación cooperativa para poder terminar sus escrituras antes de reconciliar los parciales.
- La extensión usa negociación automática con el mismo criterio que las URLs manuales; conserva una opción explícita de conexión única para servidores restrictivos.
- El inspector confirma “Aplicado” cuando cambia el valor recibido del gateway, en lugar de confirmarlo al emitir la solicitud.

## Validación ampliada

El motor tiene una compilación sin GUI mediante `--no-default-features`; el escritorio mantiene su compilación predeterminada. Se añadieron a CI las pruebas HTTP sin GUI y se mantienen las comprobaciones del escritorio.

Comandos locales: `cargo clippy --locked --no-default-features --all-targets -- -D warnings` y `cargo test --locked --no-default-features`. Las pruebas HTTP usan TCP local real y verifican los bytes de archivos finales y parciales. Cubren flujo único, segmentación, HEAD rechazado, rangos sin cabecera Accept-Ranges, rangos anunciados pero ignorados, límites Content-Range inválidos, cortes, reanudación, cambio de ETag, estados 401/403/404/429/503, cancelación rápida, aumento o eliminación del límite, límite agregado, cuerpo sin tamaño, chunked truncado, destino existente, pausa con parcial estable y telemetría durante la espera del limitador.

## Alcance y límites

La prueba web sigue siendo una simulación de cola. La extensión se verifica mediante sus APIs simuladas, incluidos envíos simultáneos y modos de conexión; esto no equivale a probarla instalada en todas las versiones de Chrome y Firefox.

En esta máquina faltan las bibliotecas gráficas GTK/WebKitGTK: no se ha ejecutado aquí la aplicación de escritorio ni el puente IPC completo. La compilación y las pruebas de escritorio se dejan a los jobs Linux/Windows existentes de GitHub Actions. El servidor de pruebas HTTP no representa todos los CDN o proxies externos; un resultado correcto en esta matriz no garantiza que cualquier origen desconocido funcione.

No se ha cambiado el borrador del entorno ni publicado una versión de la aplicación.
