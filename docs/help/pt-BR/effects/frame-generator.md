# Gerador de molduras

O Gerador de molduras desenha uma moldura decorativa em volta do seu
desenho: filetes para um certificado, arabescos clássicos para um convite,
degraus art déco, pérolas, grega, festonado, ondas entrelaçadas ou
guilloché, as linhas finas entrelaçadas de certificados, diplomas e vales.

## Desenhar uma moldura {#frame}

Abra o painel [[docker.frame_generator]] em
[[menu.effects > menu.effects.frame_generator]].

1. Selecione os objetos a emoldurar. Sem seleção, a moldura fica dentro da
   página.
2. Escolha o [[docker.frame_style]]; a prévia o mostra numa página pequena.
3. Ajuste as opções:
   - [[docker.frame_corner]]: retos, arredondados, recortados para dentro ou
     chanfrados (para filetes, arabescos clássicos, pérolas e festonado).
   - [[docker.frame_margin]]: o espaço entre os objetos e a moldura, ou
     entre a borda da página e a moldura.
   - [[docker.frame_width]]: a largura da faixa decorada.
   - [[docker.frame_line]]: a espessura das linhas finas.
   - [[docker.frame_repeat]]: o comprimento de uma pérola, grega, festão ou
     onda; cada lado recebe um número inteiro deles.
   - [[docker.frame_strands]]: no guilloché, quantas linhas finas se
     entrelaçam ao longo da faixa.
   - [[docker.frame_side_ornaments]]: um losango com dois pontos no meio de
     cada lado.
   - [[docker.frame_color]]: a cor das linhas e ornamentos.
4. Clique em [[docker.apply]].

A moldura vem como um grupo de curvas por cima, selecionado, e um desfazer
a remove. Desagrupe para mudar a cor ou mover as partes.
