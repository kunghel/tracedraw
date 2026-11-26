# The poster as a bitmap in Image > Edit Inside: a rectangle selection
# round the mountains and Hue/Saturation/Lightness previewed inside it,
# from the Effects panel of the picture editing mode.
time.sleep(0.5)
click(636, 500); time.sleep(0.5)
click(272, 12); time.sleep(0.8)
click(313, 143); time.sleep(4)
click(17, 163); time.sleep(0.5)
drag(390, 360, 860, 570, steps=20); time.sleep(0.8)
click(224, 12); time.sleep(1.0)
click(311, 275); time.sleep(2.0)
drag(1368, 256, 1300, 256, steps=10); time.sleep(3.0)
move(1500, 700); time.sleep(1)
shot("picture")
