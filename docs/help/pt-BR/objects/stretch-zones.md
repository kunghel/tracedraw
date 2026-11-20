# Zonas de esticar de um símbolo

> *Tradução automática: esta página pode estar atrasada em relação à versão em inglês.*

Um símbolo pode manter os cantos quando uma instância é esticada: uma
moldura, um botão ou uma etiqueta mantém então os cantos arredondados,
entalhes e ornamentos no seu tamanho, por mais larga ou alta que a instância
fique.

## Definir as zonas {#set}

Selecione uma instância e escolha
[[menu.object > menu.object.symbol > menu.object.symbol_stretch_zones]].
Ligue [[stretch_zones.on]] e defina a distância de cada linha aos lados do
símbolo: [[stretch_zones.left]], [[stretch_zones.right]],
[[stretch_zones.top]] e [[stretch_zones.bottom]]. O esboço mostra a caixa do
símbolo com as quatro linhas. Clique em OK.

As quatro linhas dividem o símbolo em nove zonas. Quando uma instância é
esticada:

- as quatro zonas dos cantos mantêm o tamanho;
- as zonas dos lados esticam só ao longo do seu lado;
- a zona do meio ocupa o resto, nos dois sentidos.

Todas as instâncias do símbolo acompanham, no ecrã, na impressão e nos
arquivos exportados. As curvas são cortadas onde cruzam uma linha, por isso
uma forma que atravessa uma linha dobra com o esticar ali. Imagens dentro do
símbolo, e a largura dos contornos, esticam por inteiro. Uma instância menor
que os seus cantos também estica por inteiro.

## Desligar as zonas {#off}

Abra o diálogo de novo e desligue [[stretch_zones.on]]: as instâncias voltam
a esticar por inteiro. As zonas ficam com o símbolo quando ele é editado.
