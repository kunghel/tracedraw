# Bend with pins

Bend with Pins bends objects as if they were a sheet of rubber held down at
a few points. Stick pins in the objects, drag them, and the objects follow:
one pin moves them, two turn them, more bend them. Near each pin a part keeps
its shape and only turns, so arms, tails, ribbons and lettering bend
naturally.

## Bending objects {#bend}

1. Select the objects (curves, shapes, text or groups; not bitmaps).
2. Choose [[menu.effects > menu.effects.bend_pins]] to open [[pins.title]].
3. Click the objects in the preview to stick pins: one where a part should
   stay, one on each part to move.
4. Drag a pin to its new place. The preview shows the objects as they were
   in grey and bent in blue. Right click a pin to take it out;
   [[pins.clear]] takes them all out.
5. Click OK.

The objects become curves (their envelope, perspective and distortion made
part of the curves) and bend, in one step to undo. Every side is cut into
short curves first, so a long straight side bends smoothly.

## How far a pin holds {#reach}

[[pins.reach]] sets how fast a pin's hold fades with distance. Low values
spread the bend over the whole object; high values keep each part close to
the pin nearest it, with the bend between the pins.
