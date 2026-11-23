# Transparência

A transparência deixa o que está embaixo de um objeto aparecer através dele.
Ela pode ser igual em todo o objeto, mudar ao longo dele como um gradiente ou
seguir um padrão ou uma textura.

## Aplicar transparência {#apply}

- Com a ferramenta [[tool.transparency]]: clique em um objeto e escolha um tipo
  de transparência na barra de propriedades, ou arraste através do objeto para
  uma transparência em gradiente, de opaco (onde você pressiona) a
  transparente (onde solta). Os nós do gradiente aparecem então no objeto,
  como na ferramenta [[tool.interactive_fill]]: arraste o início, o fim ou o
  nó redondo de inclinação; arraste a barra entre dois nós (o ponto médio)
  para mudar onde a mistura chega à metade; dê um duplo clique na linha para
  acrescentar um nó, e num nó para removê-lo.
- Na seção [[docker.transparency]] do painel [[docker.properties]].
- Com **Objeto > Aplicar transparência uniforme** (uma transparência uniforme
  no preenchimento e no contorno, com a quantidade usada por último, 50% de
  início) e **Objeto > Remover transparência**. Os dois podem receber um atalho
  de teclado em Personalização > Comandos.
- **Objeto > Achatar transparência** transforma os objetos transparentes
  selecionados num só bitmap opaco de como eles aparecem sobre o que está por
  baixo, com a forma deles, para impressoras e ficheiros que não misturam
  transparência. Os objetos por baixo ficam como estão.

## Configurações {#settings}

- O tipo: [[transparency.none]], [[transparency.uniform]] (de início),
  [[transparency.fountain]] (linear, radial, cônico, quadrado ou moldado, como
  os preenchimentos em gradiente), [[transparency.pattern]] ou
  [[transparency.texture]].
- A quantidade: 0 a 100% (50% de início). Em um gradiente, as paradas são tons
  de cinza: branco é opaco, preto é totalmente transparente.
- [[transparency.merge_mode]]: como as cores se misturam com o que está
  embaixo. Normal (de início) simplesmente deixa ver através; os outros, como
  Adicionar, Subtrair, Diferença, Multiplicar, Dividir, Se mais claro, Se mais
  escuro, Matiz, Saturação, Cor e Inverter, combinam as cores de outras formas.
- Se vale para o [[transparency.target_fill]], o
  [[transparency.target_outline]] ou [[transparency.target_all]].
- **Congelar**: mantém o que aparece através como está agora, mesmo que os
  objetos embaixo se movam.

Para removê-la, clique em [[toolbar.clear_transparency]] no fim da barra de
propriedades (ou escolha [[transparency.none]] como tipo).

No painel Propriedades, uma transparência de padrão também tem o tamanho, a
rotação e a inclinação dos ladrilhos e os botões de espelhar ladrilhos, e uma
textura tem a sua **Biblioteca de texturas** (nuvens, mármore, ruído, madeira)
com o tamanho e **Regenerar**. O botão **+** ao lado de **Editar transparência**
(**Salvar como nova**) guarda a transparência atual no seletor do seu tipo,
para qualquer desenho; um clique com o botão direito numa salva a remove.

Para copiar a transparência de outro objeto, use
[[menu.object > menu.object.copy_effect > menu.object.copy_transparency_from]].
Para áreas transparentes definidas pelo brilho de um objeto, veja
[Máscaras de opacidade](../objects/clip-frames.md#opacity-mask).
