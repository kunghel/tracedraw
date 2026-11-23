# Contributors

TraceDraw was created by Ighor Toth, its founder and lead maintainer, and
is written by its maintainers with Claude Code as co-author (see the
README). These people contributed code that is part of TraceDraw. Thank
you!

Everyone whose code is merged also appears in GitHub's contributor list:
when a contribution was recorded under another name (for example by the
coding assistant the contributor used), it is re-applied under the
contributor's own GitHub identity.

| Contributor | Contributions |
|---|---|
| [@SofiaAFigueredo](https://github.com/SofiaAFigueredo) | Save As starts with Embed fonts off (#901, for #886); a contour on text keeps the CPU low while dragging and pointing (#906, for #897); Area Fill closes hairline gaps between lines that nearly meet (#899); Bevel: Emboss draws its band around the outline, Level and Emboss colours, and its panel (#1135); a group's uniform transparency is drawn, exported and kept on Ungroup and in `.cdr` (#1386); a locked guideline shows no rotation handles (#1385); the multipage view repaints the pages beside the current one (#1384); Rotation under Transform in Print Preview (PR #2464, for #2449); a polygon or star edge takes a node or bends with the Shape tool (PR #2465, for #2451); the status bar fill drags onto an object (PR #2467, for #2448); spot colour dot and overprint ring on the swatches (PR #2468, for #2455) |
| [@belchiordameao-cmd](https://github.com/belchiordameao-cmd) | Publish to PDF leaves out pictures wholly outside the page and its bleed (#648, for #642); `.cdr` pages up to 100 m on a side keep their size (#651); `tracedraw-cli convert` reports what the `.cdr` reader skipped or guessed (#653); PDF/X embeds the drawing's CMYK profile (#649); outlines that scale with their object in PDF and SVG (#656); `.cdr` point lists of type 0x26 (#657) |
| [@rebonatto95](https://github.com/rebonatto95) | The Scripts panel's object model: pages, layers, text, Distribute, Combine and Break Apart, Replace Text, find and replace by properties, and its documentation page; MCP export of the selection (PR #2383) |
| [@zalmanleib](https://github.com/zalmanleib) | Click to select with drawing tools, shape brushes on the selection only, alignment keys while typing (#271) |
| [@notmarinho](https://github.com/notmarinho) | `.cdr`: a group's vector pattern transparency is read as its mask (PR #2490) |
| Bone NI | The interface in Lao (#2062) |
| [@viethuongdesign-ship-it](https://github.com/viethuongdesign-ship-it) | The interface in Vietnamese (#3692) |
| [@LaMka21](https://github.com/LaMka21) | Moving goes on after a right click during a drag (#174); files dropped on the window are opened or imported (#177); Paste takes what was copied last, files and pictures included (#183) |

## Other pull requests

Pull requests that were not merged, or are still in review, are work given
to the project too:

| Contributor | Pull requests |
|---|---|
| [@zalmanleib](https://github.com/zalmanleib) | Right-to-left text (#271; the rest of the pull request is merged) |
| [@bfs2021](https://github.com/bfs2021) | Repeat after Undo uses the last action still applied (#372; closed by its author in favour of the same fix on main) |
| [@LaMka21](https://github.com/LaMka21) | traceCUT sign making and cutting tools (#170, #245; kept for later), page tab right-click menu (#186; covered by #199 and #210) |

## Testing and reports

People who report bugs, review and test fixes in the nightly builds, send
sample files and compare with the reference program, in the issues and in
[GitHub Discussions](https://github.com/tracedraw/tracedraw/discussions),
help just as much; they are thanked in the issues and in
[`CHANGELOG.md`](CHANGELOG.md). Some went well beyond one report:

| Contributor | Help |
|---|---|
| [@archit995sipah-ai](https://github.com/archit995sipah-ai) | More than 240 reports that led to fixes, across printing and booklets, Print Preview, text and Hindi text in PDF, blends, fills, page navigation and `.cdr` files that keep their colours and pictures (#378, #771, #947, #986, #1057, #1268) |
| [@muhammadawais052400](https://github.com/muhammadawais052400) | More than 140 precise reports with steps to repeat them: the Pen tool, node and segment editing, Arabic (right-to-left) text, frame rates with many shapes and in the multipage view, Repeat after Undo (#240, #253, #359, #366, #474, #903) |
| [@2CLC](https://github.com/2CLC) | Tested build after build on a 2010 office PC (Intel HD Graphics, OpenGL 2.1) with logs and screenshots until TraceDraw ran on it with its own card, which is why older PCs are supported (#616); more than 60 other reports and many confirmations of fixes, from the Transformations panel to guidelines and text colours (#115, #376, #912) |
| [@vistech91](https://github.com/vistech91) | Glyphs, corners that stay editable, colour conversion between CMYK and RGB and the colour readout of the eyedropper (#469, #872, #990, #1254) |
| [@oksandao](https://github.com/oksandao) | Segment Delete, node control, Smart Fill on guidelines, fills that move with their object and text in other languages (#182, #314, #775, #899) |
| [@elmonopascual](https://github.com/elmonopascual) | Printing: spot colour separations, print preview and missing fonts; missing icons in the panels (#779, #846, #848, #1243) |
| [@Pink-ET](https://github.com/Pink-ET) | Toolbars and their customisation, the text cursor, the Object list, the update download on x86-64 and snapping marks (#632, #836, #1038, #1538) |
| [@markosc](https://github.com/markosc) | Export (WebP quality, profiles and presets), groups inside frames and the start of the app on Windows 11 (#544, #570, #612, #851) |
| [@aleksandrkirjanov-cloud](https://github.com/aleksandrkirjanov-cloud) | Perspective, the Dimension tools, the eyedropper, Drop Shadow and zoom (#458, #838, #935, #1039) |
| [@iAyadMed](https://github.com/iAyadMed) | Contours and their offset, Segment Delete, turning shapes and editing fills (#424, #528, #754, #933) |
| [@andresafu](https://github.com/andresafu) | Printing fitted to the page, text sizes in millimetres, boolean shaping and gradient handles (#529, #534, #539, #939) |
| [@giuliovale](https://github.com/giuliovale) | Hyphenation, text wrapped around a shape and CMYK separation options (#470, #571, #661) |
| [@atanczos](https://github.com/atanczos) | Clip frames with every shaping command, the Knife, the Eraser and Break Apart, groups of clip frames and the Shape tool on text, with `.cdr` samples (#1131, #1605, #1662, #1745) |
| [@johnreactor](https://github.com/johnreactor) | Convert to Bitmap and bitmap colour modes (CMYK, Lab, Indexed), ICC profiles, spot colours and overprint, snapping and `.cdr` text on a path (#1033, #1777, #2381, #2455, #2605) |
| [@abhisheksingh995-sipah](https://github.com/abhisheksingh995-sipah) | Blends, the Distort tools, Repeat, Print Preview fitting and master pages (#1563, #1566, #1576, #1577) |
| [@muhamadhamidabdullah](https://github.com/muhamadhamidabdullah) | `.cdr` text: envelopes, text on a path, text that moved on saving, spacing and quotation marks, with sample files and side-by-side checks (#2124, #2359, #2368, #2431, #2586) |
| [@geekfinite](https://github.com/geekfinite) | Clip frame editing, live blends in `.cdr`, the font manager and variable fonts, the browser version on iPad (#870, #980, #1311, #1562) |
| [@jckat2006](https://github.com/jckat2006) | Pen tablets on Linux, the Shift+F3 text case shortcut, active tabs and title bars (#1841, #2049, #2059, #2070) |
| [@gustavolang](https://github.com/gustavolang) | The B-spline tool, PDF import of one page, `.tdraw` file sizes and shortcuts (#1506, #2061, #2184, #2345) |
| [@fegrifo](https://github.com/fegrifo) | Shape tool selection, Weld of open curves, hit-testing, palettes and the eyedropper, in issues and Discussions (#1594, #1934, #2378, #2380) |
| [@makaka007](https://github.com/makaka007) | Rectangle corners and their limits, Cmd+Q on macOS, decimals in the property bar (#1669, #1670, #1757, #1761) |
| [@pharmankur](https://github.com/pharmankur) | `.cdr` gradients with many sample files and test rounds, the Navigator and pasting into other programs (#1257, #2053, #2288) |
| [@BRR34L0RD](https://github.com/BRR34L0RD) | Print Merge and prepress: Print Preview, separations and imposition, in Discussions (#1601, #1610) |
| [@tommywestbrook49](https://github.com/tommywestbrook49) | Text wrap, drop caps and other publishing work, in issues and Discussions (#2619, #2620, #2618) |

Many more people also reported, confirmed fixes or sent files. Thank you to
[@102089959](https://github.com/102089959), [@afrulfatkhur](https://github.com/afrulfatkhur), [@agenciadaviddesigner-alt](https://github.com/agenciadaviddesigner-alt), [@aguilargabriel848-debug](https://github.com/aguilargabriel848-debug), [@AhmedHassan8080](https://github.com/AhmedHassan8080), [@Akash9956321](https://github.com/Akash9956321), [@aldy823](https://github.com/aldy823), [@alexandrecleite](https://github.com/alexandrecleite), [@amorimgsx](https://github.com/amorimgsx), [@calalamus-p](https://github.com/calalamus-p), [@corelsnakeoficial](https://github.com/corelsnakeoficial), [@diegomontalvan548-wq](https://github.com/diegomontalvan548-wq), [@diegorafaelss](https://github.com/diegorafaelss), [@Dimiko63](https://github.com/Dimiko63), [@dwimartien](https://github.com/dwimartien), [@emingratz](https://github.com/emingratz), [@Fura123](https://github.com/Fura123), [@ivan13x](https://github.com/ivan13x), [@jaripcomputer18-source](https://github.com/jaripcomputer18-source), [@Jerry-1984](https://github.com/Jerry-1984), [@jmapublicidade-alt](https://github.com/jmapublicidade-alt), [@jucajoar-blip](https://github.com/jucajoar-blip), [@KaykeHu3](https://github.com/KaykeHu3), [@kemodang0238](https://github.com/kemodang0238), [@Kenyarte-44](https://github.com/Kenyarte-44), [@khad96](https://github.com/khad96), [@Kiritoodomoru](https://github.com/Kiritoodomoru), [@lucashikaru](https://github.com/lucashikaru), [@luiszotavio2020-a11y](https://github.com/luiszotavio2020-a11y), [@maceedu](https://github.com/maceedu), [@macos15](https://github.com/macos15), [@marqope](https://github.com/marqope), [@mkygraphics-dev](https://github.com/mkygraphics-dev), [@Mrj756](https://github.com/Mrj756), [@mrsbrds](https://github.com/mrsbrds), [@newteenpt](https://github.com/newteenpt), [@Pailledelune](https://github.com/Pailledelune), [@PauloVolkova](https://github.com/PauloVolkova), [@ppa1222wb-art](https://github.com/ppa1222wb-art), [@pwmaudio-maker](https://github.com/pwmaudio-maker), [@Raimengia](https://github.com/Raimengia), [@ratmanz0000](https://github.com/ratmanz0000), [@ropageek](https://github.com/ropageek), [@sanalvv](https://github.com/sanalvv), [@somakuna](https://github.com/somakuna), [@studiokrause](https://github.com/studiokrause), [@sujithta](https://github.com/sujithta), [@tebicap](https://github.com/tebicap), [@terefang](https://github.com/terefang), [@tybkboy](https://github.com/tybkboy), [@XuBaocai](https://github.com/XuBaocai), [@yztianya](https://github.com/yztianya), [@Zgred82](https://github.com/Zgred82), [@zhang-922](https://github.com/zhang-922).

To contribute, read [`CONTRIBUTING.md`](CONTRIBUTING.md). When your pull
request is merged, you are added here.
