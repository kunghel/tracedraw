# Mehrseitenansicht

> *Maschinelle Übersetzung: Diese Seite kann hinter der englischen Fassung zurückliegen.*

[[menu.view > menu.view.multipage]] legt alle Seiten der Zeichnung gleichzeitig
auf dem Desktop aus, sodass Sie Seiten vergleichen und nebeneinander
bearbeiten können.

## Die Anordnung {#layout}

Die Seiten liegen in Seitenreihenfolge in einem Raster aus vier Spalten; eine
neue Seite kommt an den nächsten Platz, und die Seiten davor bleiben, wo sie
waren. Unter jeder Seite stehen ihre
Nummer und ihr Name; die der aktuellen Seite in der normalen Textfarbe, die
anderen abgeblendet.

## Space between pages {#page-spacing}

The pages sit 30 mm apart at first. To change it, choose
[[menu.layout > menu.layout.page_spacing]] and pick
[[menu.layout.page_spacing_0]], [[menu.layout.page_spacing_5]],
[[menu.layout.page_spacing_10]], [[menu.layout.page_spacing_20]],
[[menu.layout.page_spacing_30]] or [[menu.layout.page_spacing_50]], or
[[menu.layout.page_spacing_custom]] to type any distance in the drawing's
units. [[options.page_spacing]] on the Layout page of
[Document options](document-options.md#layout) sets it too. The spacing is
saved with the drawing, [[menu.edit.undo]] puts the previous one back, and
**Save as default** in the document options keeps it for new drawings.

## Die Seiten wählen {#choose-pages}

Bei vielen Seiten, oder um nur einige zu vergleichen, wählen Sie, welche
Seiten die Ansicht zeigt: [[menu.view > menu.view.multipage_pages]] oder die
Schaltfläche mit zwei Seiten und einem Häkchen in der Seitenleiste, gleich nach
der Schaltfläche der Mehrseitenansicht (hervorgehoben, solange ein
Seitenbereich gewählt ist). Wählen Sie im Dialogfeld
[[dialog.multipage_all_pages]] oder [[dialog.multipage_some_pages]] und geben
Sie Seitennummern und Bereiche durch Kommas getrennt ein, etwa `1-3, 7`. Das
Dialogfeld zeigt, wie viele Seiten das sind; ein Bereich ohne Seite der
Zeichnung wird nicht übernommen. **OK** schaltet die Ansicht mit diesen Seiten
ein, in Seitenreihenfolge im Raster zusammengerückt.

Die aktuelle Seite wird immer gezeigt: Eine Seite, zu der Sie mit den
Seitenregistern oder dem Navigator wechseln, kommt hinzu, solange sie aktuell
ist. Die gewählten Seiten werden wie der Seitenabstand mit der Zeichnung
gespeichert, und [[menu.edit.undo]] stellt die vorige Wahl wieder her.

## In der Mehrseitenansicht arbeiten {#working}

- Die aktuelle Seite wird wie gewohnt bearbeitet: Lineale, Hilfslinien, Gitter
  und die Statusleiste beziehen sich auf sie.
- Klicken oder ziehen Sie auf einer anderen Seite, um sie zur aktuellen zu
  machen und sofort dort zu arbeiten.
- Auch die Seitenregister und der Navigator wechseln die aktuelle Seite.
- [[menu.view.zoom_to_fit]] (<kbd>F4</kbd>) passt alle Seiten ins Fenster ein.

Die Schaltfläche mit vier kleinen Seiten in der Seitenleiste, gleich nach den
Seitensteuerelementen, schaltet die Ansicht ebenfalls ein und aus; sie
leuchtet, solange die Ansicht an ist.

Wählen Sie erneut [[menu.view.multipage]], um wieder zu einer Seite auf einmal
zurückzukehren. [[menu.view.page_sorter]] zeigt die Seiten stattdessen als
Miniaturen; die beiden Ansichten schließen einander aus.
