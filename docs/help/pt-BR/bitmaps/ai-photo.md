# Comandos de foto com IA

Quando os recursos de IA estão ligados e o seu serviço de IA faz imagens
(veja Ferramentas > Opções > IA), o menu Imagem e o menu de contexto de
um bitmap ganham mais quatro comandos. Eles mandam a imagem selecionada,
e as suas palavras, ao seu próprio serviço de IA quando você os usa;
nada é enviado antes. Com a IA desligada, eles não aparecem.

## Girar um objeto em 3D {#turn}

1. Selecione a imagem de um objeto. Para ter só o objeto, remova o fundo
   antes.
2. Clique em [[menu.bitmaps > menu.bitmaps.ai_turn]].
3. Ajuste **[[aiphoto.turn_turn]]** (em torno do eixo vertical) e
   **[[aiphoto.turn_tilt]]** (para cima ou para baixo). Se quiser, escreva
   mais em **[[aiphoto.words]]**.
4. Marque **[[aiphoto.turn_keep_background]]** para receber a imagem
   inteira; desmarcado, vem só o objeto com fundo transparente.
5. Clique em [[dialog.ok]]. A nova vista fica por cima da imagem; a
   imagem continua por baixo.

O serviço de IA inventa os lados que estavam escondidos, então confira o
resultado.

## Remover um objeto {#remove}

1. Selecione uma foto.
2. Clique em [[menu.bitmaps > menu.bitmaps.ai_remove_object]].
3. Diga o que remover, por exemplo "o carro vermelho".
4. Clique em [[dialog.ok]]. O objeto sai, com a sombra, o reflexo, a
   fumaça e outros vestígios, e o fundo é preenchido. Desfazer traz a
   foto de volta.

Para remover objetos neste computador sem serviço de IA, use
[[menu.bitmaps > menu.bitmaps.remove_background_local]] ou o editor de
fotos.

## Fazer um mapa de profundidade {#depth}

[[menu.bitmaps > menu.bitmaps.ai_depth_map]] pede uma imagem em cinza de
quão longe está cada parte da foto: branco é mais perto. Ela fica por
cima da imagem, e o pincel e o carimbo do editor de fotos a usam para
seguir a forma 3D do objeto.

## Transformar um rascunho em curvas {#sketch}

1. Selecione um rascunho escaneado ou fotografado.
2. Clique em [[menu.bitmaps > menu.bitmaps.ai_sketch]].
3. Diga como deve ficar, por exemplo "vetorize este lettering com
   perfeição e preencha de preto".
4. Clique em [[dialog.ok]]. O serviço de IA redesenha o rascunho de forma
   limpa e o TraceDraw o vetoriza em curvas por cima do rascunho.
