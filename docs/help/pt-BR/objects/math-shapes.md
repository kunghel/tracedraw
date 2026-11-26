# Formas Matemáticas

O painel [[docker.math_shapes]] desenha a partir da matemática: um plano
cartesiano com eixos, grade e números e as curvas das suas fórmulas nele, ou
um círculo, triângulo, polígono, elipse ou arco feito a partir das suas
medidas, com as medidas escritas ao lado. O que ele faz são curvas e textos
comuns, que você pode editar, recolorir ou exportar. Abra-o em
[[menu.object > menu.object.create > menu.object.math_shapes]] ou em
[[menu.window > menu.window.dockers]].

Os dois botões no alto alternam entre [[math.mode_graph]] e
[[math.mode_figure]]. A prévia no alto do painel mostra exatamente o que
[[math.create]] desenha.

## Gráficos {#graph}

**1.** Escolha um ponto de partida em [[math.preset]], como uma parábola,
ondas de seno, uma rosácea, uma espiral ou as cônicas, ou comece do plano
vazio.

**2.** Cada curva tem o seu cartão. Escolha o [[math.kind]]:

- [[math.kind_function]]: digite a fórmula, como `2x^2 - 3x + 1`, `sin(x)`
  ou `sqrt(x)`. Fica um vão onde a função não é definida, e a curva se
  interrompe nas assíntotas em vez de ligá-las.
- [[math.kind_parametric]]: uma fórmula para x e outra para y, ambas em `t`.
- [[math.kind_polar]]: a distância da origem como fórmula do ângulo, como
  `3cos(4t)` para uma rosácea.
- [[math.kind_implicit]]: qualquer equação, como `x^2 + y^2 = 9`.
- [[math.kind_points]]: pontos como `A(1, 2); B(-3, 4)`, ligados ou não.

Uma fórmula com erro fica vermelha e diz o que está errado; as outras curvas
continuam sendo desenhadas. [[math.help]], no fim do painel, lista os
operadores, funções e constantes.

**3.** Dê a cada curva uma cor, largura de linha, tracejado, um
preenchimento abaixo dela ou dentro dela, e [[math.curve_label]] para
escrever a fórmula ao lado. [[math.marks]] marca onde uma função cruza os
eixos e os seus pontos mais alto e mais baixo. [[math.add_curve]] põe mais
curvas no mesmo plano.

**4.** As letras a, b, c, d e k numa fórmula são [[math.parameters]]:
arraste os controles deslizantes e veja a curva mudar.

**5.** Em [[math.plane]], defina os intervalos de x e y, o tamanho de uma
unidade em milímetros (ou o tamanho do plano inteiro), os eixos com setas e
nomes, as marcas e os números (números em múltiplos de π para ondas), a
grade e uma [[math.minor]] mais fina como papel milimetrado, cada uma com a
sua cor e largura. Desligue [[math.show_plane]] para desenhar só as curvas,
como ornamentos.

**6.** Clique em [[math.create]]. O gráfico entra como um grupo no meio da
janela: grade, eixos, números e cada curva em grupos próprios.
[[math.smooth]] transforma cada curva em curvas Bézier suaves com poucos
nós.

## Figuras {#figure}

Escolha uma [[math.figure]] e diga o que é [[math.given]]:

- Um círculo pelo raio, diâmetro, circunferência ou área.
- Um triângulo por três lados, dois lados e o ângulo entre eles, dois
  ângulos e o lado entre eles, um triângulo retângulo, isósceles ou
  equilátero. Medidas que não formam triângulo são avisadas em vermelho.
- Um polígono regular pelo lado ou pelo raio circunscrito, ou uma estrela.
- Uma elipse pelos semieixos, um arco, setor ou segmento.

Em [[math.show]], escolha o que desenhar e escrever: nomes dos vértices,
comprimentos, ângulos com seus arcos, o centro, raio, apótema, alturas,
medianas, bissetrizes, as circunferências inscrita e circunscrita,
diagonais e focos. A lista em [[math.facts]] dá todas as medidas
calculadas: lados, ângulos, perímetro, área e mais.

## Alterar depois {#update}

Selecione um grupo feito pelo painel: as configurações voltam para o
painel. Altere-as e clique em [[math.update]]: o grupo é desenhado de novo
no mesmo lugar, num só passo de desfazer.
