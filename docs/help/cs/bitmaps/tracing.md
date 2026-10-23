# Vektorizace bitmap

Vektorizace promění bitmapu ve vektorové tvary, které můžete upravovat,
přebarvovat a měnit jejich velikost bez ztráty kvality: naskenované logo,
skicu, fotografii.

## Rychlá vektorizace {#quick-trace}

Vyberte bitmapu a zvolte [[menu.bitmaps > menu.bitmaps.quick_trace]]. Použije
se předvolba zvolená na stránce [[options.bitmap_tracing]] v globálních
možnostech (výchozí Clipart).

[[menu.bitmaps.repeat_last_trace]] obtáhne vybranou bitmapu znovu jako poslední obtažení, bez dialogového okna, a [[menu.bitmaps.last_trace_settings]] otevře dialogové okno s nastavením posledního obtažení. Tyto příkazy má i místní nabídka bitmapy, spolu s [[menu.bitmaps.centerline_trace]] a [[menu.bitmaps.outline_trace]].

## Dialogové okno vektorizace bitmapy {#trace}

[[menu.bitmaps > menu.bitmaps.outline_trace]] a
[[menu.bitmaps.centerline_trace]] obsahují seznam předvoleb; výběrem jedné
otevřete dialogové okno vektorizace.

- **Vektorizace obrysu** vyplní barevné oblasti tvary:
  [[menu.bitmaps.trace_line_art]], [[menu.bitmaps.trace_logo]],
  [[menu.bitmaps.trace_detailed_logo]], [[menu.bitmaps.trace_clipart]],
  [[menu.bitmaps.trace_low_quality]] a [[menu.bitmaps.trace_high_quality]].
- **Vektorizace středové čáry** sleduje čáry otevřenými křivkami:
  [[menu.bitmaps.trace_technical]] a [[menu.bitmaps.trace_line_drawing]].

![Dialogové okno vektorizace bitmapy](../../img/bitmaps-tracing.png)

Dialogové okno ukazuje bitmapu **před** a její vektorizaci **po**, aktualizovanou
při změně nastavení:

- tlačítka předvoleb nahoře;
- **Detail** (0 až 100): více detailů zachová menší oblasti;
- **Vyhlazení** (0 až 100, výchozí 25): hladší křivky, méně uzlů;
- **Vyhlazení rohů** (0 až 100): kulatější rohy;
- **Černobílá**, nebo počet **barev** (1 až 256);
- **Odstranit pozadí** (výchozí zapnuto): vynechá barvu pozadí;
- **Odstranit původní obrázek** (výchozí vypnuto);
- **Náhled** spočítá křivky úplné vektorizace.

Kliknutím na [[dialog.ok]] vektorizaci provedete. Tvary se umístí přes
bitmapu, nejtmavší jako první.
