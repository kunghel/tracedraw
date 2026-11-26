# Charts from data

TraceDraw draws a chart from a table of numbers: columns, bars, lines,
stacked areas, points, a pie or a radar web, with its axes, labels and a
legend. The chart comes in as one group of plain objects, so every column,
line, slice and label takes any fill, outline or effect, and it prints sharp
at any size.

## Inserting a chart {#insert}

Choose [[menu.object > menu.object.insert > menu.object.insert_chart]] to
open [[chart.title]]. Pick the kind, type or paste the table, set the width,
the height, the font and the size of the labels, then click OK. The preview
shows the chart as you change it. The chart is placed in the middle of the
page and selected.

## Typing the table {#data}

One row a line, the cells parted by tabs (as a spreadsheet copies them), by
semicolons or by commas. With tabs or semicolons a comma inside a number is
its decimal point (`1,5`). A number may end in `%`.

- The first row names the series when a cell after its first is not a
  number.
- The first column names the categories when one of its cells is not a
  number.
- An empty cell is no value: no column, no point.

```
; 2024; 2025
North; 12; 15
South; 9; 11
```

## The kinds {#kinds}

| Kind | What it draws |
|---|---|
| [[chart.kind_columns]] | upright columns, the series of a category side by side |
| [[chart.kind_stacked_columns]] | upright columns, the series of a category on one another |
| [[chart.kind_bars]] | level bars, the categories down the left |
| [[chart.kind_stacked_bars]] | level bars, the series one after another |
| [[chart.kind_lines]] | a line with points through each series |
| [[chart.kind_areas]] | each series a band on the ones before it |
| [[chart.kind_scatter]] | points: the first column is X, each other column the Y of a series |
| [[chart.kind_pie]] | a slice per category, from the first series |
| [[chart.kind_radar]] | a spoke per category (three or more), each series a closed line |

[[chart.legend]] lists the series (the categories for a pie) at the right.
[[chart.values]] writes each value on its column, bar, point or slice; a pie
shows percentages.

## Changing a chart {#edit}

Select the chart and choose
[[menu.object > menu.object.insert > menu.object.insert_chart]] again: the
dialog opens as [[chart.edit_title]] with the chart's table and settings.
After OK the chart is drawn again in place, its top left corner staying put,
in one step to undo. Colours given to the parts are drawn again in the
chart's own colours.

A chart that is ungrouped or broken apart is no longer a chart, and the menu
item inserts a new one.
