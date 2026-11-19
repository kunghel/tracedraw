# Formulas

TraceDraw sets mathematical formulas: fractions, roots, powers and indices,
Greek letters, sums and integrals with their limits, and brackets that grow
with what they hold. A formula comes in as one curve object, so it prints
sharp at any size and takes any fill, outline or effect.

## Inserting a formula {#insert}

Choose [[menu.object > menu.object.insert > menu.object.insert_formula]] to
open [[formula.title]]. Type the formula in the field; the preview under it
shows the result as you type. Choose the font and the size, then click OK. The
formula is placed in the middle of the page and selected.

## Writing a formula {#syntax}

Formulas use the backslash notation common in mathematics papers:

| Type | To get |
|---|---|
| `x^2`, `x_1`, `x_i^2` | a power, an index, both |
| `e^{-x^2}`, `a_{n+1}` | braces group what goes up or down |
| `\frac{a}{b}` | a fraction |
| `\sqrt{x}`, `\sqrt[3]{x}` | a square root, a cube root |
| `\alpha`, `\beta`, `\pi`, `\Omega` | Greek letters |
| `\sum_{i=1}^{n}`, `\prod`, `\int_a^b`, `\oint` | sums, products and integrals with limits |
| `\lim_{x \to 0}`, `\max`, `\sin`, `\log` | names of functions, upright |
| `\left( ... \right)`, `\left[ ... \right]`, `\left\{ ... \right.` | brackets as tall as what they hold (`.` for none) |
| `\pm`, `\times`, `\cdot`, `\le`, `\ge`, `\ne`, `\approx`, `\to`, `\infty`, `\in` | signs and relations |
| `\vec{v}`, `\bar{x}`, `\hat{a}`, `\overline{AB}` | accents and a line above |
| `\text{if } x > 0` | words in upright letters |
| `\,` `\;` `\quad` | small, medium and wide spaces |
| `\\` | a new line |

Single letters are set in italic, numbers and names of functions upright,
with the spaces around `+`, `=` and other signs made for you. A command the
dialog does not know is shown as typed, and the dialog lists it under the
preview.

## Changing a formula {#edit}

Select the formula and choose
[[menu.object > menu.object.insert > menu.object.insert_formula]] again: the
dialog opens as [[formula.edit_title]] with the formula's text, font and size.
After OK the formula changes in place, keeping its colour, outline, size and
position (its top left corner stays put), in one step to undo.

A formula converted to curves, broken apart or combined with other objects is
no longer a formula, and the menu item inserts a new one.
