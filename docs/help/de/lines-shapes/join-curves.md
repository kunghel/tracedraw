# Kurven verbinden

Das Andockfenster [[docker.join_curves]] schließt die Lücken zwischen den Enden
von Kurven, zum Beispiel bei Linien, die sich in einer Ecke treffen sollten,
aber kurz davor aufhören. Öffnen Sie es mit
[[menu.object > menu.object.join_curves]].

![Das Andockfenster Kurven verbinden](../../img/lines-shapes-join-curves.png)

## Kurven verbinden {#join}

1. Wählen Sie die Kurven aus. Offene Teile innerhalb einer Kurve werden
   ebenfalls verbunden.
2. Wählen Sie, wie die Enden verbunden werden:
   - **Verlängern**: Beide Enden laufen in ihrer Richtung weiter, bis sie sich
     als Ecke treffen. Treffen sich die Richtungen vor beiden Enden nie,
     verbindet sie eine gerade Linie.
   - **Fase**: eine gerade Linie zwischen den Enden.
   - **Abrundung**: wie Verlängern, dann wird die Ecke mit dem **Radius**
     abgerundet (anfangs 2 mm).
   - **Bézierkurve**: eine glatte Kurve, die in den Richtungen der Enden
     abgeht und ankommt.
3. Stellen Sie die **Lückentoleranz** ein (anfangs 5 mm). Weiter entfernte
   Enden werden nicht verbunden.
4. Klicken Sie auf [[docker.apply]].

Das nächstgelegene Endenpaar wird zuerst verbunden, dann das nächste, bis
innerhalb der Toleranz kein Paar mehr übrig ist; die beiden Enden eines Teils
schließen ihn. Die verbundene Kurve übernimmt Platz, Füllung und Umriss der
zuletzt ausgewählten Kurve, und die anderen Kurven werden entfernt. Liegen
keine Enden nah genug beieinander, ändert sich nichts, und die Statusleiste
weist darauf hin.

Sie können auch zwei Endknoten einer Kurve mit dem Werkzeug [[tool.shape]]
verbinden; siehe
[Kurven mit dem Formwerkzeug bearbeiten](shape-tool.md#property-bar). Wenn Sie
vom Ende einer ausgewählten Kurve aus zeichnen, wird sie ebenfalls fortgesetzt;
siehe [Eine Kurve fortsetzen](lines.md#continue).

## Werkzeug Verbinden {#join-tool}

Das Werkzeug [[tool.join]], im Werkzeugkasten beim Werkzeug [[tool.shape]],
verbindet die Endknoten offener Kurven von Hand.

- Zeichnen Sie einen Strich über die Endknoten: Sie werden paarweise
  verbunden, in der Reihenfolge, in der der Strich sie erreicht. Zwei Enden
  innerhalb des Auswahlabstands werden ein Knoten in der Mitte zwischen ihnen;
  weiter entfernte Enden verbindet ein gerades Segment.
- Klicken Sie einen Endknoten an (er wird gefüllt) und dann einen anderen, um
  diese beiden zu verbinden. Ein Klick daneben lässt den ersten los.
- Die beiden Enden einer Kurve schließen sie. Enden zweier Kurven ergeben eine
  Kurve mit Füllung und Umriss der Kurve des ersten Endes.

Alle Verbindungen eines Strichs sind ein Rückgängig-Schritt. Das Werkzeug
wirkt auf die sichtbaren, nicht gesperrten Kurven der Seite außerhalb von
Gruppen.
