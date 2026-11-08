# Symbole

> *Maschinelle Übersetzung: Diese Seite kann hinter der englischen Fassung zurückliegen.*

Ein **Symbol** ist ein Objekt, das Sie einmal definieren und viele Male
platzieren. Jede Kopie, **Instanz** genannt, ist eine Verknüpfung mit der
Definition: Bearbeiten Sie die Definition, ändert sich jede Instanz. Symbole
halten Dateien klein und wiederholte Teile einheitlich. Sie sind im
Andockfenster [[docker.symbols]] (<kbd>Ctrl+F3</kbd>) aufgelistet.

![Das Andockfenster Symbole](../../img/objects-symbols.png)

## Ein Symbol erstellen {#create}

1. Wählen Sie die Objekte aus, aus denen das Symbol bestehen soll.
2. Klicken Sie im Andockfenster [[docker.symbols]] auf [[docker.new_symbol]],
   oder wählen Sie [[menu.object > menu.object.symbol > menu.object.symbol_new]]
   oder [[menu.object > menu.object.create > menu.object.create_symbol]].

Die Auswahl wird zur ersten Instanz des neuen Symbols.

## Instanzen platzieren {#insert}

Das Andockfenster listet die Symbole der Zeichnung auf, jedes mit der Anzahl
seiner Instanzen. Klicken Sie neben einem Symbol auf [[docker.insert]], um eine
neue Instanz zu platzieren, oder wählen Sie
[[menu.object > menu.object.symbol > menu.object.symbol_insert]]. Instanzen
lassen sich wie jedes Objekt verschieben, skalieren und drehen.

Eine Instanz, die in einer Zeichnung kopiert und in einer anderen eingefügt
wird, bringt ihr Symbol mit (und die Symbole, die sie zeigt), sodass sie dort
dieselben Objekte zeigt. Erneutes Einfügen verwendet das bereits mitgebrachte
Symbol.

## Ein Symbol bearbeiten {#edit}

Klicken Sie im Andockfenster neben einem Symbol auf [[docker.edit_symbol]],
wählen Sie [[menu.object.symbol_edit]] im Kontextmenü einer Instanz oder
[[menu.object > menu.object.symbol > menu.object.symbol_edit]], oder
doppelklicken Sie mit dem Auswahlwerkzeug auf eine Instanz. Die Objekte des
Symbols erscheinen dort auf der Seite, wo diese Instanz sie zeigt, und der Rest
der Seite wird blass: Verschieben, umformen, umfärben, hinzufügen oder löschen
Sie Objekte wie gewohnt. Klicken Sie auf [[clip.finish]] in der Leiste oben
links im Zeichenfenster, wählen Sie
[[menu.object > menu.object.symbol > menu.object.symbol_finish]] oder drücken
Sie <kbd>Esc</kbd>, wenn nichts ausgewählt ist: Jede Instanz zeigt die
Änderung. Rückgängig geht schrittweise durch die Bearbeitung wie durch jede
andere.

Ein Symbol kann keine Instanz von sich selbst enthalten; Beenden meldet das,
und die Bearbeitung geht weiter, bis diese Instanz entfernt ist.

## Instanzen wieder in Objekte umwandeln {#revert}

Wählen Sie Instanzen aus und klicken Sie auf [[docker.revert_to_objects]], oder
wählen Sie [[menu.object > menu.object.symbol > menu.object.symbol_revert]].
Sie werden zu gewöhnlichen Objekten, die dem Symbol nicht mehr folgen.
