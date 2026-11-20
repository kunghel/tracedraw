# Como um grupo junta os seus membros

> *Tradução automática: esta página pode estar atrasada em relação à versão em inglês.*

Um grupo normalmente desenha os membros um após o outro sobre o que está por
baixo, e a transparência dele vai para cada membro, por isso membros que se
sobrepõem aparecem uns através dos outros. Duas opções em
[[menu.object > menu.object.group]] mudam isso nos grupos selecionados.

## Desenhar um grupo como um só {#as-one}

Com [[menu.object.group_as_one]] ligado, o grupo é desenhado à parte primeiro
e depois posto como uma só imagem:

- a transparência vai para o grupo inteiro, por isso onde os membros se
  sobrepõem os de baixo não aparecem através dos de cima;
- os modos de mesclagem dos membros misturam-se só com os membros do próprio
  grupo, não com o que está por baixo do grupo.

## Membros que ocultam os de baixo {#hide-beneath}

Com [[menu.object.group_knockout]] ligado, cada membro oculta os membros por
baixo dele onde está, mesmo onde é transparente: um membro transparente mostra
o que está por baixo do grupo, não os outros membros. Use para formas
transparentes que se sobrepõem sem escurecer onde se encontram.

As duas opções podem estar ligadas juntas, e ambas acompanham o grupo quando
é copiado e salvo. Desagrupar remove-as.

## Em arquivos exportados {#export}

Em PDF, um grupo com [[menu.object.group_knockout]], ou um grupo transparente
desenhado como um só, entra como imagem com a sua transparência. Em SVG, um
grupo desenhado como um só mantém a transparência no grupo, e um grupo com
[[menu.object.group_knockout]] entra como imagem.
