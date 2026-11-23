# El panel Efectos

Un objeto puede llevar varios efectos dinámicos a la vez, aplicados uno tras
otro. El panel [[docker.effects]] enumera los efectos del objeto seleccionado
en el orden en que se aplican, de arriba abajo. Ábralo con
[[menu.window > menu.window.dockers > menu.window.dockers_effects]].

![El panel Efectos](../../img/effects-effects-docker.png)

## Gestionar los efectos {#stack}

Cada fila de la lista tiene:

- una casilla que activa o desactiva el efecto. Un efecto desactivado
  conserva su lugar y sus ajustes, pero no se dibuja, ni se exporta, ni se
  imprime;
- el nombre del efecto, atenuado mientras está desactivado;
- flechas arriba y abajo que adelantan o retrasan el efecto, lo que cambia el
  resultado (por ejemplo, una silueta de una envoltura, o una envoltura de una
  silueta);
- una cruz que quita el efecto.

Cada cambio es un paso de deshacer. Cada efecto se sigue editando con su
propia herramienta o panel.

## Otros comandos de efectos {#commands}

- [[menu.effects > menu.effects.flatten]] convierte los efectos de los objetos
  seleccionados en objetos normales.
- [[menu.effects > menu.effects.merge_shapes]] agrupa los objetos
  seleccionados y da al grupo un [[effect.offset_path]] en vivo que los une
  en un solo contorno, redondeado donde se juntan y pintado como el objeto de
  abajo: mueva un objeto del grupo y las formas se funden o se separan.
  Ajuste **Suavizar** en los efectos de capa arriba del panel.
- [[menu.effects.picker]] muestra una lista de efectos que aplicar a la
  selección.
- [[menu.object > menu.object.clear_effect]] quita el efecto de la selección.
- [[menu.object > menu.object.copy_effect]] copia un efecto de otro objeto, y
  [[menu.object.clone_effect]] crea un clon que lo sigue; vea
  [Copiar y clonar efectos](copy-and-clone.md).

## Aspecto hecho a mano y marcos

Los efectos de capa arriba del panel también incluyen estos, todos en vivo:

- [[effect.roughen]] vuelve a dibujar el contorno con pequeñas ondas al azar.
- [[effect.jitter_nodes]] mueve al azar los nodos de la curva.
- [[effect.scribble]] dibuja el relleno como una línea en zigzag suelta, del
  color del relleno; el contorno se mantiene.
- [[effect.shape_frame]] cambia el contorno por un rectángulo, un rectángulo
  redondeado o una elipse alrededor del objeto.
- [[effect.trim_marks]] dibuja líneas en las esquinas del objeto, por fuera.
- [[effect.feather]] desvanece el objeto hacia su borde.

**Variación** da otro patrón a los efectos al azar.
