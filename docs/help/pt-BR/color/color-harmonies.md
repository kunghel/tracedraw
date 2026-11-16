# Harmonias de cores e alterar estilos juntos

O painel [[docker.color_styles]] faz mais que guardar uma lista de cores.
Os estilos podem ser escolhidos e alterados juntos, agrupados em harmonias,
trocados, mesclados e transformados em degradês; o que acontecer a um
estilo, os objetos que usam a cor dele acompanham, numa só etapa de
desfazer. Veja [Estilos de cor](color-styles.md) para criar e aplicar
estilos.

## Mostrar os estilos {#view}

[[docker.cs_view_list]] mostra cada estilo numa linha com cor, nome e
botões; [[docker.cs_view_swatches]] os mostra como amostras de cor. Os
estilos soltos vêm primeiro, depois cada harmonia sob o seu nome.

## Escolher estilos {#pick}

- Na lista, marque a caixa no começo da linha.
- Nas amostras, clique numa amostra para escolhê-la, Ctrl+clique para
  acrescentar ou tirar uma, Shift+clique para escolher a sequência desde a
  última escolhida. Um clique duplo preenche os objetos selecionados.
- Clique no nome de uma harmonia para escolher todos os seus estilos.
- [[docker.cs_pick_all]] e [[docker.cs_pick_none]] escolhem todos ou
  nenhum.

Com estilos escolhidos, o painel oferece:

- [[docker.cs_fill_picked]] e [[docker.cs_outline_picked]]: o primeiro
  estilo escolhido nos objetos selecionados;
- [[docker.cs_swap]] (dois estilos escolhidos): cada estilo fica com a cor
  do outro, e os objetos também;
- [[docker.cs_merge]]: os estilos viram o primeiro escolhido; os outros
  são excluídos e seus objetos ficam com a cor dele;
- [[docker.cs_group]] e [[docker.cs_ungroup]]: põem os estilos numa nova
  harmonia ou os tiram da sua;
- [[docker.cs_select_objects]]: seleciona os objetos da página que os
  usam;
- [[docker.cs_delete_picked]]: exclui os estilos (os objetos mantêm as
  cores).

## Alterar estilos juntos {#adjust}

Em [[docker.cs_adjust]], cada estilo escolhido é um ponto na
[[docker.cs_wheel]]: arraste em volta da roda para girar todos, mantendo a
distância entre eles. Os controles [[docker.cs_hue]],
[[docker.cs_saturation]] e [[docker.cs_brightness]] mudam todos de uma
vez. Um arrasto é uma só etapa de desfazer.

Quando uma harmonia inteira está escolhida, [[docker.cs_rule]] a recalcula
a partir da primeira cor por outra regra ([[harmony.complementary]],
[[harmony.triadic]] e as demais).

## Degradês {#gradient}

[[docker.cs_new_gradient]] cria uma harmonia de tons do primeiro estilo
escolhido (ou da [[docker.base_color]] quando nenhum está), do mais claro
ao mais escuro: [[docker.cs_steps]] define quantos, [[docker.cs_spread]] a
distância entre os extremos.

## Mais {#more}

O botão **...** traz [[docker.cs_delete_unused]], [[docker.cs_sort_name]],
[[docker.cs_sort_hue]], [[docker.cs_to_cmyk]], [[docker.cs_to_rgb]] e
[[docker.cs_to_palette]]; os três últimos agem nos estilos escolhidos, ou
em todos. [[docker.cs_from_document]] cria um estilo de cada cor usada no
desenho.
