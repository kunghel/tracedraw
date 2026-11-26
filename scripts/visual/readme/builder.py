# The weather icon's outlines with the Shape Builder: a drag across the
# cloud's parts lights them up to merge them, the Hints panel beside.
click(700, 900); time.sleep(0.3)
combo("Control_L","a"); time.sleep(0.5)
hold(19, 196, 0.9); time.sleep(0.5)
click(116, 308); time.sleep(0.6)
combo("Shift_L","F2"); time.sleep(1.5)
# merge the cloud: drag across its parts
pts=[(420,520),(520,470),(620,420),(720,390),(800,440),(860,520),(780,590),(600,600),(480,580)]
move(*pts[0]); xtest.fake_input(d, X.ButtonPress, 1); d.sync(); time.sleep(0.15)
for (a,b),(c,e) in zip(pts,pts[1:]):
    for i in range(1,9):
        move(a+(c-a)*i//8, b+(e-b)*i//8); time.sleep(0.03)
time.sleep(1.0)
shot("builder")
xtest.fake_input(d, X.ButtonRelease, 1); d.sync()
