# Fórmulas

> *Tradução automática: esta página pode estar atrasada em relação à versão em inglês.*

O TraceDraw monta fórmulas matemáticas: frações, raízes, potências e
índices, letras gregas, somatórios e integrais com os seus limites, e
parênteses que crescem com o que contêm. A fórmula entra como um único objeto
de curva, por isso imprime nítida em qualquer tamanho e aceita qualquer
preenchimento, contorno ou efeito.

## Inserir uma fórmula {#insert}

Escolha [[menu.object > menu.object.insert > menu.object.insert_formula]] para
abrir [[formula.title]]. Digite a fórmula no campo; a prévia embaixo mostra o
resultado enquanto você digita. Escolha a fonte e o tamanho e clique em OK. A
fórmula é colocada no meio da página e fica selecionada.

## Escrever uma fórmula {#syntax}

As fórmulas usam a notação com barra invertida comum em textos de matemática:

| Digite | Para obter |
|---|---|
| `x^2`, `x_1`, `x_i^2` | uma potência, um índice, os dois |
| `e^{-x^2}`, `a_{n+1}` | as chaves agrupam o que sobe ou desce |
| `\frac{a}{b}` | uma fração |
| `\sqrt{x}`, `\sqrt[3]{x}` | uma raiz quadrada, uma raiz cúbica |
| `\alpha`, `\beta`, `\pi`, `\Omega` | letras gregas |
| `\sum_{i=1}^{n}`, `\prod`, `\int_a^b`, `\oint` | somatórios, produtórios e integrais com limites |
| `\lim_{x \to 0}`, `\max`, `\sin`, `\log` | nomes de funções, em letra reta |
| `\left( ... \right)`, `\left[ ... \right]`, `\left\{ ... \right.` | parênteses da altura do que contêm (`.` para nenhum) |
| `\pm`, `\times`, `\cdot`, `\le`, `\ge`, `\ne`, `\approx`, `\to`, `\infty`, `\in` | sinais e relações |
| `\vec{v}`, `\bar{x}`, `\hat{a}`, `\overline{AB}` | acentos e uma linha em cima |
| `\text{se } x > 0` | palavras em letra reta |
| `\,` `\;` `\quad` | espaços pequeno, médio e largo |
| `\\` | uma nova linha |

Letras soltas ficam em itálico, números e nomes de funções em letra reta, e
os espaços em volta de `+`, `=` e outros sinais são feitos para você. Um
comando que a janela não conhece aparece como foi digitado, e a janela o
lista embaixo da prévia.

## Alterar uma fórmula {#edit}

Selecione a fórmula e escolha de novo
[[menu.object > menu.object.insert > menu.object.insert_formula]]: a janela
abre como [[formula.edit_title]] com o texto, a fonte e o tamanho da fórmula.
Depois do OK a fórmula muda no lugar, mantendo a cor, o contorno, o tamanho e
a posição (o canto superior esquerdo fica onde estava), num único passo de
desfazer.

Uma fórmula convertida em curvas, separada ou combinada com outros objetos
deixa de ser fórmula, e o item do menu insere uma nova.
