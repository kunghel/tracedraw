# Ricalcare le bitmap

Il ricalco trasforma una bitmap in forme vettoriali che puoi modificare,
ricolorare e scalare senza perdere qualità: un logo scansionato, uno schizzo,
una foto.

## Ricalco rapido {#quick-trace}

Seleziona una bitmap e scegli [[menu.bitmaps > menu.bitmaps.quick_trace]].
Usa il preset scelto nella pagina [[options.bitmap_tracing]] delle opzioni
globali (Clipart in partenza).

[[menu.bitmaps.repeat_last_trace]] vettorizza di nuovo la bitmap selezionata come l'ultima vettorizzazione, senza finestra di dialogo, e [[menu.bitmaps.last_trace_settings]] apre la finestra di dialogo con le sue impostazioni. Anche il menu contestuale di una bitmap ha questi comandi, con [[menu.bitmaps.centerline_trace]] e [[menu.bitmaps.outline_trace]].

## La finestra di dialogo Ricalca bitmap {#trace}

[[menu.bitmaps > menu.bitmaps.outline_trace]] e
[[menu.bitmaps.centerline_trace]] elencano i preset; scegline uno per aprire la
finestra di dialogo del ricalco.

- **Ricalco del contorno** riempie le aree di colore con forme:
  [[menu.bitmaps.trace_line_art]], [[menu.bitmaps.trace_logo]],
  [[menu.bitmaps.trace_detailed_logo]], [[menu.bitmaps.trace_clipart]],
  [[menu.bitmaps.trace_low_quality]] e [[menu.bitmaps.trace_high_quality]].
- **Ricalco della linea centrale** segue le linee con curve aperte:
  [[menu.bitmaps.trace_technical]] e [[menu.bitmaps.trace_line_drawing]].

![La finestra di dialogo Ricalca bitmap](../../img/bitmaps-tracing.png)

La finestra di dialogo mostra la bitmap **prima** e il suo ricalco **dopo**,
aggiornato mentre cambi le impostazioni:

- i pulsanti dei preset in alto;
- **Dettaglio** (da 0 a 100): più dettaglio mantiene le aree più piccole;
- **Smussatura** (da 0 a 100, 25 in partenza): curve più morbide, meno nodi;
- **Morbidezza degli angoli** (da 0 a 100): angoli più arrotondati;
- **Bianco e nero**, o il numero di **Colori** (da 1 a 256);
- **Rimuovi sfondo** (attivo in partenza): esclude il colore di sfondo;
- **Elimina immagine originale** (disattivato in partenza);
- **Anteprima** conta le curve del ricalco completo.

Fai clic su [[dialog.ok]] per ricalcare. Le forme vengono posizionate sopra la
bitmap, a partire dalla più scura.
