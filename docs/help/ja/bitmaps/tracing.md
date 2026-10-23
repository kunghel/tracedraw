# ビットマップのトレース

トレースは、ビットマップを編集、色の変更、拡大縮小しても品質が落ちないベクターの図形に変換します。スキャンしたロゴ、スケッチ、写真などに使います。

## クイックトレース {#quick-trace}

ビットマップを選択し、[[menu.bitmaps > menu.bitmaps.quick_trace]] を選びます。グローバルオプションの [[options.bitmap_tracing]] ページで選んだプリセット (最初はクリップアート) を使います。

[[menu.bitmaps.repeat_last_trace]] は選択したビットマップをダイアログを開かずに前回と同じ設定でトレースし、[[menu.bitmaps.last_trace_settings]] は前回の設定でダイアログボックスを開きます。ビットマップの右クリックメニューにも、[[menu.bitmaps.centerline_trace]] と [[menu.bitmaps.outline_trace]] とともにこれらのコマンドがあります。

## ビットマップのトレースダイアログボックス {#trace}

[[menu.bitmaps > menu.bitmaps.outline_trace]] と [[menu.bitmaps.centerline_trace]] にはプリセットが一覧されています。一つを選ぶとトレースのダイアログボックスが開きます。

- **アウトライントレース** は、色の領域を図形で塗りつぶします: [[menu.bitmaps.trace_line_art]]、[[menu.bitmaps.trace_logo]]、[[menu.bitmaps.trace_detailed_logo]]、[[menu.bitmaps.trace_clipart]]、[[menu.bitmaps.trace_low_quality]]、[[menu.bitmaps.trace_high_quality]]。
- **センターライントレース** は、線を開いたカーブでなぞります: [[menu.bitmaps.trace_technical]] と [[menu.bitmaps.trace_line_drawing]]。

![ビットマップのトレースダイアログボックス](../../img/bitmaps-tracing.png)

ダイアログボックスには、**変換前** のビットマップと **変換後** のトレースが表示され、設定を変えると更新されます:

- 上部のプリセットボタン。
- **詳細** (0 から 100): 詳細を上げると、より小さな領域まで残します。
- **スムージング** (0 から 100、最初は 25): カーブを滑らかにし、ノードを減らします。
- **角の滑らかさ** (0 から 100): 角を丸くします。
- **白黒**、または **色** の数 (1 から 256)。
- **背景を削除** (最初はオン): 背景色を除外します。
- **元の画像を削除** (最初はオフ)。
- **プレビュー** は、トレース全体のカーブの数を数えます。

[[dialog.ok]] をクリックするとトレースします。図形はビットマップの上に、暗いものから順に置かれます。
