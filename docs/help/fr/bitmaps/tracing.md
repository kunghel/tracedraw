# Vectoriser des images bitmap

La vectorisation transforme une image bitmap en formes vectorielles que vous
pouvez modifier, recolorer et mettre à l'échelle sans perte de qualité : un
logo numérisé, un croquis, une photo.

## Vectorisation rapide {#quick-trace}

Sélectionnez une image bitmap et choisissez
[[menu.bitmaps > menu.bitmaps.quick_trace]]. Elle utilise le préréglage
choisi dans la page [[options.bitmap_tracing]] des options globales (Clipart
au départ).

[[menu.bitmaps.repeat_last_trace]] vectorise à nouveau le bitmap sélectionné comme la dernière vectorisation, sans la boîte de dialogue, et [[menu.bitmaps.last_trace_settings]] ouvre la boîte de dialogue avec ses paramètres. Le menu contextuel d'un bitmap propose aussi ces commandes, avec [[menu.bitmaps.centerline_trace]] et [[menu.bitmaps.outline_trace]].

## La boîte de dialogue de vectorisation {#trace}

[[menu.bitmaps > menu.bitmaps.outline_trace]] et
[[menu.bitmaps.centerline_trace]] listent des préréglages ; choisissez-en un
pour ouvrir la boîte de dialogue de vectorisation.

- **Vectorisation par contour** remplit les zones de couleur avec des
  formes : [[menu.bitmaps.trace_line_art]], [[menu.bitmaps.trace_logo]],
  [[menu.bitmaps.trace_detailed_logo]], [[menu.bitmaps.trace_clipart]],
  [[menu.bitmaps.trace_low_quality]] et [[menu.bitmaps.trace_high_quality]].
- **Vectorisation par ligne médiane** suit les lignes avec des courbes
  ouvertes : [[menu.bitmaps.trace_technical]] et
  [[menu.bitmaps.trace_line_drawing]].

![La boîte de dialogue de vectorisation](../../img/bitmaps-tracing.png)

La boîte de dialogue montre l'image bitmap **avant** et sa vectorisation
**après**, mise à jour à mesure que vous changez les réglages :

- les boutons de préréglage en haut ;
- **Détail** (de 0 à 100) : plus de détail conserve des zones plus petites ;
- **Lissage** (de 0 à 100, 25 au départ) : des courbes plus douces, moins de
  nœuds ;
- **Lissage des angles** (de 0 à 100) : des angles plus arrondis ;
- **Noir et blanc**, ou le nombre de **Couleurs** (de 1 à 256) ;
- **Supprimer le fond** (actif au départ) : omet la couleur de fond ;
- **Supprimer l'image d'origine** (inactif au départ) ;
- **Aperçu** compte les courbes de la vectorisation complète.

Cliquez sur [[dialog.ok]] pour vectoriser. Les formes sont placées sur
l'image bitmap, les plus foncées en premier.
