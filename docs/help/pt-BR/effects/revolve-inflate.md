# Revolver e inflar 3D

Revolver e inflar 3D dá a um desenho plano um visual 3D sombreado feito de
formas vetoriais comuns: gire um perfil para fazer um vaso ou uma garrafa
com o seu rótulo em volta, ou infle uma forma como uma almofada.

## Fazer um objeto 3D {#make}

Abra o painel [[docker.shape3d]] em
[[menu.effects > menu.effects.shape3d]].

1. Selecione uma curva ou forma. A cor do preenchimento (ou do contorno) é a
   cor que recebe o sombreado.
2. Escolha o modo:
   - [[docker.shape3d_revolve]] gira a curva em volta de um eixo vertical na
     borda esquerda, como um torno de oleiro: desenhe metade do contorno de
     um vaso, garrafa ou copo (meio círculo faz uma bola). Escolha uma
     [[docker.shape3d_position]] (frente, fora do eixo ou isométrica) ou
     ajuste [[docker.shape3d_tilt]] (quanto você olha de cima),
     [[docker.shape3d_turn]] e [[docker.shape3d_roll]]. Ajuste
     [[docker.shape3d_segments]], [[docker.shape3d_sweep]] (menos de 360 o
     deixa aberto), [[docker.shape3d_offset]] (o eixo longe da curva faz um
     anel), [[docker.shape3d_axis_right]] e [[docker.shape3d_cap]] (fecha as
     pontas).
   - [[docker.shape3d_inflate]] infla a forma; [[docker.shape3d_depth]]
     define a altura.
3. Escolha a [[docker.shape3d_surface]]: [[docker.shape3d_surface_plastic]]
   (iluminada, com reflexo), [[docker.shape3d_surface_diffuse]],
   [[docker.shape3d_surface_flat]] ou [[docker.shape3d_surface_wireframe]].
   Ajuste a luz: [[docker.shape3d_light_angle]],
   [[docker.shape3d_light_height]], [[docker.shape3d_shine]],
   [[docker.shape3d_light_color]], [[docker.shape3d_intensity]],
   [[docker.shape3d_softness]] (uma luz suave chega mais longe) e
   [[docker.shape3d_ambient]] (desligada, o lado escuro fica preto).
4. Para aplicar uma arte na superfície, selecione a arte (um logotipo, um
   rótulo, texto convertido em curvas) e clique em [[docker.shape3d_art]];
   depois selecione a curva de novo, ou escolha um dos símbolos do
   documento em [[docker.shape3d_symbol]]. Posicione com [[docker.shape3d_art_u]]
   (50 é a frente), [[docker.shape3d_art_v]] e [[docker.shape3d_art_size]].
   A arte virada para trás fica escondida. [[docker.shape3d_fit]] estica a
   arte sobre toda a superfície (listras em volta de uma bola);
   [[docker.shape3d_stretch]] estica ao longo da curva;
   [[docker.shape3d_shade_art]] desligado mantém as cores da arte. Com
   [[docker.shape3d_hide_surface]] só a arte é desenhada, sem a superfície.
5. Clique em [[docker.apply]]. [[docker.shape3d_keep]] mantém a curva
   original.

O resultado é um grupo de formas, selecionado, e um desfazer o remove.

## Mudar depois {#edit}

O objeto 3D guarda a curva e os ajustes. Selecione-o e o painel mostra os
ajustes: mude o que quiser e clique em [[docker.shape3d_update]] para
refazê-lo no lugar. [[docker.shape3d_release]] o transforma de volta na
curva, para editar a própria curva e aplicar de novo.

## Esfera de meio-tom {#halftone-sphere}

[[docker.shape3d_halftone_sphere]] cria uma bola de pontos de meio-tom com
um clique, sem seleção: defina quantos [[docker.shape3d_halftone_dots]]
atravessam e clique em [[docker.shape3d_halftone_create]]. Os pontos,
grandes no meio e pequenos na borda, são embrulhados na frente de uma
esfera de 80 mm e iluminados pelos ajustes de luz acima (e pela inclinação
e giro). Só os pontos são desenhados, como curvas vetoriais que você pode
recolorir ou editar.
