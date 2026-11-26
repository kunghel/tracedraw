# Espalhar símbolos

Espalhar símbolos preenche uma forma, uma curva ou uma linha de texto com
muitas cópias de um símbolo: estrelas num coração, flores numa letra, seu
logotipo num selo. As cópias nunca se amontoam, e cada uma ganha seu próprio
tamanho e giro.

## Espalhar símbolos {#scatter}

Abra o painel [[docker.symbol_scatter]] em
[[menu.effects > menu.effects.symbol_scatter]].

1. Escolha os símbolos na lista [[docker.scatter_symbols]]: um símbolo pronto
   (estrela, coração, círculo, brilho, flor, triângulo, losango, gota), todos
   misturados, ou objetos seus. Para usar os seus, selecione um ou mais
   objetos e clique em [[docker.scatter_use_selection]]; cada cópia é um
   deles, escolhido ao acaso.
2. Selecione os objetos a preencher. Texto funciona como está, e os furos (o
   miolo de um O, um anel) ficam vazios.
3. Ajuste as opções:
   - [[docker.scatter_spacing]]: a menor distância entre duas cópias (5 mm
     de início). Um espaçamento menor faz mais cópias.
   - [[docker.scatter_size]]: a menor e a maior cópia, em porcentagem do
     espaçamento (50 % a 90 % de início). Acima de 100 % as cópias se
     sobrepõem.
   - [[docker.scatter_rotation]]: quanto cada cópia pode girar para cada
     lado (180 graus gira à vontade; 0 as mantém em pé).
   - [[docker.scatter_inside]]: cada cópia fica inteira dentro do contorno,
     reduzida ou omitida quando não cabe. Desligado, só o centro precisa
     estar dentro.
   - [[docker.scatter_color]]: as cópias mantêm as próprias cores (os
     símbolos prontos pegam a cor do objeto preenchido), pegam a cor do
     objeto preenchido, cores aleatórias da paleta do documento ou uma cor
     que você escolhe.
   - [[docker.scatter_seed]]: a mesma semente dá a mesma disposição;
     [[docker.scatter_shuffle]] usa a próxima.
   - [[docker.scatter_keep]]: desligado remove os objetos preenchidos.
4. Clique em [[docker.apply]].

As cópias vêm como um grupo por cima, selecionado, e um desfazer as remove.
Cada Aplicar faz no máximo 5000 cópias; aumente o espaçamento para objetos
grandes.

Marque **Pincel no desenho** no fim do painel para retocar à mão um grupo de
cópias. Selecione o grupo, escolha um **Pincel** e arraste sobre as cópias:
as que estão sob o pincel mudam, mais as mais perto do centro, e o arrasto
inteiro é um passo de desfazer.

- **Mover** leva as cópias junto com o ponteiro.
- **Juntar** puxa-as para o pincel; segure <kbd>Alt</kbd> para espalhá-las.
- **Aumentar** deixa-as maiores; <kbd>Alt</kbd> deixa-as menores.
- **Girar** gira-as; <kbd>Alt</kbd> gira para o outro lado.
- **Tingir** leva as cores delas para a cor embaixo da lista.
- **Esmaecer** deixa-as mais transparentes; <kbd>Alt</kbd>, mais opacas.

**Raio** é até onde o pincel alcança e **Força** quanto cada arrasto muda.
Com a caixa marcada e um grupo selecionado, os arrastos no desenho vão para o
pincel; um clique continua selecionando.
