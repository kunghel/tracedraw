# Calcar mapas de bits

El calco convierte un mapa de bits en formas vectoriales que puede editar,
cambiar de color y escalar sin perder calidad: un logotipo escaneado, un
boceto, una fotografía.

## Calco rápido {#quick-trace}

Seleccione un mapa de bits y elija [[menu.bitmaps > menu.bitmaps.quick_trace]].
Usa el ajuste preestablecido elegido en la página [[options.bitmap_tracing]]
de las opciones globales (Imágenes prediseñadas de forma predeterminada).

[[menu.bitmaps.repeat_last_trace]] vuelve a calcar el mapa de bits seleccionado como el último calco, sin el cuadro de diálogo, y [[menu.bitmaps.last_trace_settings]] abre el cuadro de diálogo con la configuración del último calco. El menú contextual de un mapa de bits también tiene estos comandos, con [[menu.bitmaps.centerline_trace]] y [[menu.bitmaps.outline_trace]].

## El cuadro de diálogo Calcar mapa de bits {#trace}

[[menu.bitmaps > menu.bitmaps.outline_trace]] y
[[menu.bitmaps.centerline_trace]] enumeran ajustes preestablecidos; elija uno
para abrir el cuadro de diálogo de calco.

- **Calco de contorno** rellena las áreas de color con formas:
  [[menu.bitmaps.trace_line_art]], [[menu.bitmaps.trace_logo]],
  [[menu.bitmaps.trace_detailed_logo]], [[menu.bitmaps.trace_clipart]],
  [[menu.bitmaps.trace_low_quality]] y [[menu.bitmaps.trace_high_quality]].
- **Calco de línea central** sigue las líneas con curvas abiertas:
  [[menu.bitmaps.trace_technical]] y [[menu.bitmaps.trace_line_drawing]].

![El cuadro de diálogo Calcar mapa de bits](../../img/bitmaps-tracing.png)

El cuadro de diálogo muestra el mapa de bits **antes** y su calco **después**,
actualizado a medida que cambia los ajustes:

- los botones de ajustes preestablecidos, arriba;
- **Detalle** (de 0 a 100): más detalle conserva áreas más pequeñas;
- **Suavizado** (de 0 a 100, 25 de forma predeterminada): curvas más suaves,
  menos nodos;
- **Suavidad de esquinas** (de 0 a 100): esquinas más redondeadas;
- **Blanco y negro**, o el número de **Colores** (de 1 a 256);
- **Eliminar fondo** (activado de forma predeterminada): omite el color de
  fondo;
- **Eliminar imagen original** (desactivado de forma predeterminada);
- **Vista previa** cuenta las curvas del calco completo.

Haga clic en [[dialog.ok]] para calcar. Las formas se colocan sobre el mapa de
bits, primero las más oscuras.
