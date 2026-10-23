# 描摹位图

描摹将位图变为矢量形状，你可以编辑、重新着色和缩放它们而不损失质量：扫描的徽标、草图、照片都可以。

## 快速描摹 {#quick-trace}

选择一个位图并选择 [[menu.bitmaps > menu.bitmaps.quick_trace]]。它使用全局选项中 [[options.bitmap_tracing]] 页面上选定的预设（起初为剪贴画）。

[[menu.bitmaps.repeat_last_trace]] 不打开对话框，按上次的方式再次描摹所选位图；[[menu.bitmaps.last_trace_settings]] 以上次的设置打开对话框。位图的右键菜单中也有这些命令，以及 [[menu.bitmaps.centerline_trace]] 和 [[menu.bitmaps.outline_trace]]。

## 描摹位图对话框 {#trace}

[[menu.bitmaps > menu.bitmaps.outline_trace]] 和 [[menu.bitmaps.centerline_trace]] 列出预设；选择一个即可打开描摹对话框。

- **轮廓描摹** 用形状填充颜色区域：[[menu.bitmaps.trace_line_art]]、[[menu.bitmaps.trace_logo]]、[[menu.bitmaps.trace_detailed_logo]]、[[menu.bitmaps.trace_clipart]]、[[menu.bitmaps.trace_low_quality]] 和 [[menu.bitmaps.trace_high_quality]]。
- **中心线描摹** 用开放曲线沿线条描摹：[[menu.bitmaps.trace_technical]] 和 [[menu.bitmaps.trace_line_drawing]]。

![描摹位图对话框](../../img/bitmaps-tracing.png)

对话框显示 **之前** 的位图和 **之后** 的描摹结果，随着你更改设置而更新：

- 顶部的预设按钮；
- **细节**（0 到 100）：细节越多，保留的小区域越多；
- **平滑**（0 到 100，起初为 25）：曲线更平滑，节点更少；
- **拐角平滑度**（0 到 100）：拐角更圆；
- **黑白**，或 **颜色** 数（1 到 256）；
- **删除背景**（起初打开）：省去背景颜色；
- **删除原始图像**（起初关闭）；
- **预览** 统计完整描摹结果的曲线数。

单击 [[dialog.ok]] 进行描摹。形状放在位图上方，最暗的在最先。
