# Alinhar e distribuir objetos

Alinhar coloca objetos em fila pelas bordas ou pelos centros; distribuir os
espaça por igual. Use o painel [[docker.align_distribute]], os comandos de
[[menu.object > menu.object.align_distribute]] ou as teclas abaixo.

![O painel Alinhar e distribuir](../../img/objects-align-distribute.png)

## Alinhar {#align}

1. Selecione os objetos. Selecione por último aquele com o qual os outros devem
   se alinhar.
2. Na parte [[docker.align]] do painel, escolha [[docker.align_to]]:
   - [[docker.align_active]] (de início): o objeto selecionado por último; com
     um só objeto selecionado, a página;
   - [[docker.align_page_edge]]: as bordas da página;
   - [[docker.align_page_center]]: o centro da página;
   - [[docker.align_grid]]: cada objeto na linha mais próxima da grade do
     documento.
3. Clique em um dos seis botões: bordas esquerdas, centros em uma linha
   vertical, bordas direitas, bordas superiores, centros em uma linha
   horizontal ou bordas inferiores.

[[docker.align_use_outline]]: com isso ativado, as bordas externas dos
contornos se encontram, em vez dos próprios caminhos.

As teclas sempre alinham pelo objeto selecionado por último:

| Tecla | Comando |
|---|---|
| <kbd>L</kbd> | [[menu.object.align_left]] |
| <kbd>R</kbd> | [[menu.object.align_right]] |
| <kbd>T</kbd> | [[menu.object.align_top]] |
| <kbd>B</kbd> | [[menu.object.align_bottom]] |
| <kbd>E</kbd> | [[menu.object.align_centers_h]] |
| <kbd>C</kbd> | [[menu.object.align_centers_v]] |
| <kbd>P</kbd> | [[menu.object.center_to_page]] |

## Distribuir {#distribute}

1. Selecione três ou mais objetos (dois ou mais para distribuir pela página).
2. Na parte [[docker.distribute]], escolha [[docker.distribute_to]]:
   - [[docker.distribute_selection]] (de início): o primeiro e o último objeto
     ficam onde estão, e os outros se movem entre eles;
   - [[docker.distribute_page]]: os objetos se espalham pela página inteira, o
     primeiro tocando um lado e o último o outro;
   - [[docker.distribute_spacing]]: o primeiro objeto fica e os outros o
     seguem à distância de [[docker.distribute_gap]] (entre os objetos para
     os espaços, senão entre suas bordas ou centros).
3. Clique em um dos oito botões: bordas esquerdas, centros, espaços ou bordas
   direitas na horizontal, ou bordas superiores, centros, espaços ou bordas
   inferiores na vertical. Distribuir os espaços dá a mesma distância entre os
   objetos, sejam quais forem os tamanhos.

Com [[docker.distribute_spacing]], depois que um botão espalhou a seleção,
você pode mudar o espaçamento e ver os objetos acompanharem: gire a roda do
mouse sobre a caixa ou arraste o controle deslizante abaixo dela. Solte o
controle, ou afaste o ponteiro da caixa, para manter o resultado como um
único passo de desfazer; <kbd>Esc</kbd> devolve o espaçamento e os objetos.

Com poucos objetos, os botões ficam claros e o painel diz quantos são
necessários. Cada alinhamento ou distribuição é um passo de desfazer.

Para colocar cópias a distâncias iguais de uma vez, veja
[Duplicar e repetir](step-and-repeat.md).

## Organizar em grade {#grid}

[[menu.object > menu.object.align_distribute > menu.object.arrange_in_grid]]
coloca os objetos selecionados em linhas e colunas, na ordem em que você os
selecionou, a partir do canto superior esquerdo da seleção. Escolha quantos
objetos vão em cada linha e o espaço entre as colunas e entre as linhas.
Cada célula tem a largura do objeto mais largo e a altura do mais alto, e
cada objeto fica no centro da sua célula, então objetos de formas e
tamanhos diferentes ficam alinhados. É um único passo de desfazer.

A janela tem mais escolhas. Ordem preenche as células na ordem em que
você selecionou os objetos ou como eles estão agora, de cima para baixo
e da esquerda para a direita. Células deixa todas as células do mesmo
tamanho ou ajusta cada coluna ao seu objeto mais largo e cada linha ao
mais alto. Posição na célula é um quadrado de nove lugares: clique em um
canto, no meio de um lado ou no centro para dizer onde cada objeto fica
na célula. Com a prévia ligada, o desenho mostra para onde cada objeto
vai, como caixas em contorno, antes de você clicar em OK; Preencher com
cópias mostra as cópias do mesmo jeito e as conta enquanto você muda os
espaços.

## Encaixar objetos num caminho {#fit-to-path}

O painel [[docker.fit_to_path]] coloca objetos ao longo de uma linha, de uma
curva ou do contorno de uma forma, com cópias, espaçamento, posição e rotação
à sua escolha. Selecione os objetos e, por último, o caminho (Shift+clique), e
escolha [[menu.object > menu.object.fit_objects_to_path]]. Veja
[Encaixar objetos num caminho](fit-objects-to-path.md) para cada opção.
