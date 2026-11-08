# Símbolos

> *Tradução automática: esta página pode estar atrasada em relação à versão em inglês.*

Um **símbolo** é um objeto que você define uma vez e coloca muitas vezes. Cada
cópia, chamada **instância**, é um vínculo com a definição: edite a definição e
todas as instâncias mudam. Os símbolos deixam os arquivos pequenos e as partes
repetidas iguais. Eles ficam listados no painel [[docker.symbols]]
(<kbd>Ctrl+F3</kbd>).

![O painel Símbolos](../../img/objects-symbols.png)

## Criar um símbolo {#create}

1. Selecione os objetos que formam o símbolo.
2. Clique em [[docker.new_symbol]] no painel [[docker.symbols]], ou escolha
   [[menu.object > menu.object.symbol > menu.object.symbol_new]] ou
   [[menu.object > menu.object.create > menu.object.create_symbol]].

A seleção passa a ser a primeira instância do novo símbolo.

## Colocar instâncias {#insert}

O painel lista os símbolos do desenho, cada um com o número de instâncias.
Clique em [[docker.insert]] ao lado de um símbolo para colocar uma nova
instância, ou escolha
[[menu.object > menu.object.symbol > menu.object.symbol_insert]]. As instâncias
podem ser movidas, escalonadas e giradas como qualquer objeto.

Uma instância copiada em um desenho e colada em outro leva o símbolo junto (e
os símbolos que ela mostra), então aparece com os mesmos objetos lá. Colar de
novo reaproveita o símbolo já trazido.

## Editar um símbolo {#edit}

Clique em [[docker.edit_symbol]] ao lado de um símbolo no painel, escolha
[[menu.object.symbol_edit]] no menu de contexto de uma instância ou
[[menu.object > menu.object.symbol > menu.object.symbol_edit]], ou clique duas
vezes em uma instância com a ferramenta Seleção. Os objetos do símbolo aparecem
na página onde essa instância os mostra e o resto da página fica esmaecido:
mova, remodele, mude as cores, adicione ou exclua objetos como de costume.
Clique em [[clip.finish]] na barra no canto superior esquerdo da janela de
desenho, escolha
[[menu.object > menu.object.symbol > menu.object.symbol_finish]] ou pressione
<kbd>Esc</kbd> sem nada selecionado: todas as instâncias mostram a mudança.
Desfazer percorre a edição passo a passo como qualquer outra.

Um símbolo não pode conter uma instância de si mesmo; Concluir avisa e a edição
continua até essa instância ser removida.

## Transformar instâncias em objetos de novo {#revert}

Selecione instâncias e clique em [[docker.revert_to_objects]], ou escolha
[[menu.object > menu.object.symbol > menu.object.symbol_revert]]. Elas viram
objetos comuns que não acompanham mais o símbolo.
