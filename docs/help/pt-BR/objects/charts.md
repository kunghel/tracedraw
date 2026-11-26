# Gráficos a partir de dados

> *Tradução automática: esta página pode estar atrasada em relação à versão em inglês.*

O TraceDraw desenha um gráfico a partir de uma tabela de números: colunas,
barras, linhas, áreas empilhadas, pontos, pizza ou teia radar, com eixos,
rótulos e legenda. O gráfico entra como um grupo de objetos simples, por isso
cada coluna, linha, fatia e rótulo aceita qualquer preenchimento, contorno ou
efeito, e imprime nítido em qualquer tamanho.

## Inserir um gráfico {#insert}

Escolha [[menu.object > menu.object.insert > menu.object.insert_chart]] para
abrir [[chart.title]]. Escolha o tipo, digite ou cole a tabela, defina a
largura, a altura, a fonte e o tamanho dos rótulos e clique em OK. A
pré-visualização mostra o gráfico enquanto o altera. O gráfico fica no meio
da página, selecionado.

## Digitar a tabela {#data}

Uma linha da tabela por linha, as células separadas por tabulações (como uma
planilha copia), por ponto e vírgula ou por vírgulas. Com tabulações ou ponto
e vírgula, a vírgula dentro de um número é a casa decimal (`1,5`). Um número
pode terminar em `%`.

- A primeira linha nomeia as séries quando uma célula depois da primeira não
  é número.
- A primeira coluna nomeia as categorias quando uma das suas células não é
  número.
- Uma célula vazia não tem valor: nem coluna, nem ponto.

```
; 2024; 2025
Norte; 12; 15
Sul; 9; 11
```

## Os tipos {#kinds}

| Tipo | O que desenha |
|---|---|
| [[chart.kind_columns]] | colunas em pé, as séries de uma categoria lado a lado |
| [[chart.kind_stacked_columns]] | colunas em pé, as séries de uma categoria umas sobre as outras |
| [[chart.kind_bars]] | barras deitadas, as categorias à esquerda |
| [[chart.kind_stacked_bars]] | barras deitadas, as séries umas a seguir às outras |
| [[chart.kind_lines]] | uma linha com pontos por cada série |
| [[chart.kind_areas]] | cada série uma faixa sobre as anteriores |
| [[chart.kind_scatter]] | pontos: a primeira coluna é X, cada outra coluna o Y de uma série |
| [[chart.kind_pie]] | uma fatia por categoria, da primeira série |
| [[chart.kind_radar]] | um raio por categoria (três ou mais), cada série uma linha fechada |

[[chart.legend]] lista as séries (as categorias numa pizza) à direita.
[[chart.values]] escreve cada valor na sua coluna, barra, ponto ou fatia; a
pizza mostra percentagens.

## Alterar um gráfico {#edit}

Selecione o gráfico e escolha
[[menu.object > menu.object.insert > menu.object.insert_chart]] de novo: o
diálogo abre como [[chart.edit_title]] com a tabela e as definições do
gráfico. Depois de OK o gráfico é desenhado de novo no lugar, com o canto
superior esquerdo parado, num só passo de desfazer. Cores dadas às partes
voltam às cores do gráfico.

Um gráfico desagrupado ou separado deixa de ser gráfico, e o item do menu
insere um novo.
