# Bitmapleri izleme

İzleme, bir bitmapi kaliteden ödün vermeden düzenleyebileceğiniz, yeniden
renklendirebileceğiniz ve ölçekleyebileceğiniz vektör şekillere dönüştürür:
taranmış bir logo, bir eskiz, bir fotoğraf.

## Hızlı izleme {#quick-trace}

Bir bitmap seçin ve [[menu.bitmaps > menu.bitmaps.quick_trace]] komutunu
seçin. Genel seçeneklerin [[options.bitmap_tracing]] sayfasında seçilen ön
ayarı kullanır (başlangıçta Küçük Resim).

[[menu.bitmaps.repeat_last_trace]] seçili bit eşlemi iletişim kutusu olmadan son izleme gibi yeniden izler; [[menu.bitmaps.last_trace_settings]] iletişim kutusunu son izlemenin ayarlarıyla açar. Bir bit eşlemin sağ tık menüsünde de [[menu.bitmaps.centerline_trace]] ve [[menu.bitmaps.outline_trace]] ile birlikte bu komutlar vardır.

## Bitmapi İzle iletişim kutusu {#trace}

[[menu.bitmaps > menu.bitmaps.outline_trace]] ve
[[menu.bitmaps.centerline_trace]] ön ayarları listeler; izleme iletişim
kutusunu açmak için birini seçin.

- **Anahat izleme** renk alanlarını şekillerle doldurur:
  [[menu.bitmaps.trace_line_art]], [[menu.bitmaps.trace_logo]],
  [[menu.bitmaps.trace_detailed_logo]], [[menu.bitmaps.trace_clipart]],
  [[menu.bitmaps.trace_low_quality]] ve [[menu.bitmaps.trace_high_quality]].
- **Orta çizgi izleme** çizgileri açık eğrilerle izler:
  [[menu.bitmaps.trace_technical]] ve [[menu.bitmaps.trace_line_drawing]].

![Bitmapi İzle iletişim kutusu](../../img/bitmaps-tracing.png)

İletişim kutusu bitmapi **önce**, izini de **sonra** olarak gösterir; ayarları
değiştirdikçe güncellenir:

- üstteki ön ayar düğmeleri;
- **Ayrıntı** (0 ile 100 arası): daha fazla ayrıntı daha küçük alanları
  korur;
- **Düzgünleştirme** (0 ile 100 arası, başlangıçta 25): daha düzgün eğriler,
  daha az düğüm;
- **Köşe yumuşaklığı** (0 ile 100 arası): daha yuvarlak köşeler;
- **Siyah beyaz** veya **Renk** sayısı (1 ile 256 arası);
- **Arka planı kaldır** (başlangıçta açık): arka plan rengini dışarıda
  bırakır;
- **Orijinal görüntüyü sil** (başlangıçta kapalı);
- **Önizleme** tam izlemenin eğrilerini sayar.

İzlemek için [[dialog.ok]] düğmesine tıklayın. Şekiller bitmapin üzerine, en
koyusu önce olmak üzere yerleştirilir.
