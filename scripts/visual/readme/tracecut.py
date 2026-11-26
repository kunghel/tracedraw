# The sticker sheet in Tools > TraceCut > CutContour with a 3 mm bleed
# and Merge overlaps: the cut lines previewed round each sticker.
time.sleep(0.5)
click(700, 900); time.sleep(0.3)
combo("Control_L","a"); time.sleep(0.5)
click(436, 12); time.sleep(0.8)
click(483, 391); time.sleep(3.0)
click(481, 416); time.sleep(2.0)
dbl(827, 380); combo("Control_L","a"); typestr("3"); key("Tab"); time.sleep(0.5)
click(781, 407); time.sleep(0.5)
click(960, 740); time.sleep(4.0)
move(1500, 900); time.sleep(0.5)
shot("tracecut")
