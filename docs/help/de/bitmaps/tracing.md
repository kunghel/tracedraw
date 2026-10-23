# Bitmaps nachzeichnen

Das Nachzeichnen wandelt eine Bitmap in Vektorformen um, die Sie ohne
Qualitätsverlust bearbeiten, umfärben und skalieren können: ein eingescanntes
Logo, eine Skizze, ein Foto.

## Schnelles Nachzeichnen {#quick-trace}

Wählen Sie eine Bitmap aus und wählen Sie
[[menu.bitmaps > menu.bitmaps.quick_trace]]. Es verwendet die Vorgabe, die auf
der Seite [[options.bitmap_tracing]] der globalen Optionen gewählt ist
(anfangs Clipart).

[[menu.bitmaps.repeat_last_trace]] vektorisiert die ausgewählte Bitmap erneut wie die letzte Vektorisierung, ohne Dialogfeld, und [[menu.bitmaps.last_trace_settings]] öffnet das Dialogfeld mit deren Einstellungen. Auch das Kontextmenü einer Bitmap enthält diese Befehle, zusammen mit [[menu.bitmaps.centerline_trace]] und [[menu.bitmaps.outline_trace]].

## Das Dialogfeld Bitmap nachzeichnen {#trace}

[[menu.bitmaps > menu.bitmaps.outline_trace]] und
[[menu.bitmaps.centerline_trace]] listen Vorgaben auf; wählen Sie eine, um das
Dialogfeld zum Nachzeichnen zu öffnen.

- **Umrissnachzeichnung** füllt Farbflächen mit Formen:
  [[menu.bitmaps.trace_line_art]], [[menu.bitmaps.trace_logo]],
  [[menu.bitmaps.trace_detailed_logo]], [[menu.bitmaps.trace_clipart]],
  [[menu.bitmaps.trace_low_quality]] und [[menu.bitmaps.trace_high_quality]].
- **Mittellinien-Nachzeichnung** folgt Linien mit offenen Kurven:
  [[menu.bitmaps.trace_technical]] und [[menu.bitmaps.trace_line_drawing]].

![Das Dialogfeld Bitmap nachzeichnen](../../img/bitmaps-tracing.png)

Das Dialogfeld zeigt die Bitmap **vorher** und ihre Nachzeichnung **nachher**,
aktualisiert, während Sie die Einstellungen ändern:

- die Vorgabe-Schaltflächen oben;
- **Detail** (0 bis 100): mehr Detail behält kleinere Flächen;
- **Glättung** (0 bis 100, anfangs 25): glattere Kurven, weniger Knoten;
- **Eckenglättung** (0 bis 100): rundere Ecken;
- **Schwarzweiß** oder die Anzahl der **Farben** (1 bis 256);
- **Hintergrund entfernen** (anfangs ein): lässt die Hintergrundfarbe weg;
- **Originalbild löschen** (anfangs aus);
- **Vorschau** zählt die Kurven der vollständigen Nachzeichnung.

Klicken Sie auf [[dialog.ok]], um nachzuzeichnen. Die Formen werden über der
Bitmap platziert, die dunkelsten zuerst.
