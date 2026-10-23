# Wektoryzacja bitmap

Wektoryzacja zamienia bitmapę w kształty wektorowe, które można edytować,
przekolorowywać i skalować bez utraty jakości: zeskanowane logo, szkic, zdjęcie.

## Szybka wektoryzacja {#quick-trace}

Zaznacz bitmapę i wybierz [[menu.bitmaps > menu.bitmaps.quick_trace]]. Używa ona
ustawienia wstępnego wybranego na stronie [[options.bitmap_tracing]] opcji
globalnych (domyślnie Clipart).

[[menu.bitmaps.repeat_last_trace]] ponownie obrysowuje zaznaczoną bitmapę jak ostatnio, bez okna dialogowego, a [[menu.bitmaps.last_trace_settings]] otwiera okno dialogowe z ustawieniami ostatniego obrysowania. Menu kontekstowe bitmapy też ma te polecenia, razem z [[menu.bitmaps.centerline_trace]] i [[menu.bitmaps.outline_trace]].

## Okno dialogowe wektoryzacji bitmapy {#trace}

[[menu.bitmaps > menu.bitmaps.outline_trace]] i
[[menu.bitmaps.centerline_trace]] zawierają listy ustawień wstępnych; wybierz
jedno, aby otworzyć okno dialogowe wektoryzacji.

- **Wektoryzacja konturu** wypełnia obszary koloru kształtami:
  [[menu.bitmaps.trace_line_art]], [[menu.bitmaps.trace_logo]],
  [[menu.bitmaps.trace_detailed_logo]], [[menu.bitmaps.trace_clipart]],
  [[menu.bitmaps.trace_low_quality]] i [[menu.bitmaps.trace_high_quality]].
- **Wektoryzacja linii środkowej** podąża za liniami krzywymi otwartymi:
  [[menu.bitmaps.trace_technical]] i [[menu.bitmaps.trace_line_drawing]].

![Okno dialogowe wektoryzacji bitmapy](../../img/bitmaps-tracing.png)

Okno dialogowe pokazuje bitmapę **przed** i jej wektoryzację **po**,
aktualizowaną w miarę zmiany ustawień:

- przyciski ustawień wstępnych u góry;
- **Szczegółowość** (od 0 do 100): więcej szczegółów zachowuje mniejsze obszary;
- **Wygładzanie** (od 0 do 100, domyślnie 25): gładsze krzywe, mniej węzłów;
- **Gładkość narożników** (od 0 do 100): bardziej zaokrąglone narożniki;
- **Czarno-biały** lub liczba **Kolorów** (od 1 do 256);
- **Usuń tło** (domyślnie włączone): pomija kolor tła;
- **Usuń obraz oryginalny** (domyślnie wyłączone);
- **Podgląd** liczy krzywe pełnej wektoryzacji.

Kliknij [[dialog.ok]], aby wykonać wektoryzację. Kształty są umieszczane nad
bitmapą, od najciemniejszego.
