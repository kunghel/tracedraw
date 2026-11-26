# The Export panel

The [[docker.export]] panel keeps a list of **assets** (pages and objects) and
exports them all at once, each in its own format, to one folder: the icons of
an app in several sizes, every page of a set of social media posts. Open it
from [[menu.window > menu.window.dockers]].

![The Export panel](../../img/web-graphics-export-docker.png)

## Adding assets {#assets}

- **Add pages** adds every page of the drawing, named after the page.
- **Add selection** adds the selected objects as one asset, cropped to them,
  named after the object.
- **Add slices** adds the area of each selected object as an asset that
  exports everything showing in that area, not only the object, cut to it.
  Set rows and columns beside the button to cut each area into a grid of
  equal pieces, one asset each, named with their row and column. Draw a
  rectangle with no fill and no outline over the part of the page you want
  and add it as a slice.
- New assets take the format and resolution set under **New assets** (PNG at
  150 dpi at first).

Each asset has a tick (only ticked assets are exported), its name (the file
name), its format, its resolution for bitmap formats (72 to 2400 dpi) and a
cross to remove it. The link under the name goes to the page or selects the
objects.

## Exporting {#export}

1. Click **Browse** to choose the folder (the first export asks for one).
2. Click **Export**. The button shows how many assets will be written.

Characters that files cannot have in their name become `_`, and two assets
with the same name get " (2)", " (3)" and so on. The status bar says how many
files were written. In the browser and on Android no folder can be chosen:
the files go out together as one zip file, downloaded in the browser and
saved where you choose on Android.
