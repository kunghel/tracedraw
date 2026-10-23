# 描繪點陣圖

描繪會將點陣圖轉換為向量形狀，讓您可以編輯、重新上色和縮放而不損失品質：掃描的標誌、草圖、相片都適用。

## 快速描繪 {#quick-trace}

選取點陣圖並選擇 [[menu.bitmaps > menu.bitmaps.quick_trace]]。它會使用全域選項的 [[options.bitmap_tracing]] 頁面上所選擇的預設集（預設為美工圖案）。

[[menu.bitmaps.repeat_last_trace]] 不開啟對話方塊，依上次的方式再次描圖所選點陣圖；[[menu.bitmaps.last_trace_settings]] 以上次的設定開啟對話方塊。點陣圖的右鍵選單中也有這些命令，以及 [[menu.bitmaps.centerline_trace]] 和 [[menu.bitmaps.outline_trace]]。

## 描繪點陣圖對話方塊 {#trace}

[[menu.bitmaps > menu.bitmaps.outline_trace]] 和 [[menu.bitmaps.centerline_trace]] 會列出預設集；選擇其中一個即可開啟描繪對話方塊。

- **外框描繪**以形狀填滿色彩區域：[[menu.bitmaps.trace_line_art]]、[[menu.bitmaps.trace_logo]]、[[menu.bitmaps.trace_detailed_logo]]、[[menu.bitmaps.trace_clipart]]、[[menu.bitmaps.trace_low_quality]] 和 [[menu.bitmaps.trace_high_quality]]。
- **中心線描繪**以開放曲線沿著線條描繪：[[menu.bitmaps.trace_technical]] 和 [[menu.bitmaps.trace_line_drawing]]。

![描繪點陣圖對話方塊](../../img/bitmaps-tracing.png)

對話方塊會顯示**描繪前**的點陣圖和**描繪後**的結果，並在您變更設定時更新：

- 頂端的預設集按鈕；
- **細節**（0 到 100）：細節越多，保留的區域越小；
- **平滑度**（0 到 100，預設為 25）：曲線更平滑、節點更少；
- **轉角平滑度**（0 到 100）：轉角更圓；
- **黑白**，或**色彩**數量（1 到 256）；
- **移除背景**（預設開啟）：省略背景色彩；
- **刪除原始影像**（預設關閉）；
- **預覽**會計算完整描繪結果的曲線數量。

按一下 [[dialog.ok]] 進行描繪。形狀會放置在點陣圖上方，最暗的在最前面。
