# Symbols panel

The Symbols panel (Ctrl+F3) lists the drawing's symbols, each with how
many instances of it the drawing holds: on every page and master layer,
inside groups and clip frames too. An instance inside a group was not
counted, so a symbol whose copies were all grouped said 0 (#3248,
`ui::dockers2::symbol_docker_tests::grouped_symbol_instances_are_counted`).

Each row shows a small picture of the symbol's objects (#3249). A
selected instance, or a group holding one, was outlined by a square
instead of the objects it shows, and a click inside a group of instances
missed it; the panel gained Cancel beside Finish while editing, and
Duplicate (#3437,
`ops2::tests::symbol_instances_outline_their_objects_cancel_and_duplicate`).
