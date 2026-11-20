# Plugins

Um plugin acrescenta comandos próprios em [[menu.tools > menu.tools.plugins]]:
uma caixa de embalagem, um layout de corte, uma forma que você desenha
sempre, ou um recurso que faz falta. É um script, escrito em JavaScript
como os do painel [[docker.scripts]], que você instala uma vez e depois
roda pelo menu. Plugins podem ser compartilhados como arquivos `.js`.

## Para instalar um plugin {#install}

1. Escolha [[menu.tools > menu.tools.plugins > menu.tools.plugin_install]].
2. Escolha o arquivo `.js` do plugin. Os comandos aparecem em
   [[menu.tools > menu.tools.plugins]], cada plugin num submenu próprio
   (um plugin de um só comando mostra só esse comando).

Instalar um plugin com o nome de outro já instalado o substitui.

Os comandos de um plugin podem ter atalhos de teclado: procure-os pelo nome
em [[menu.tools > menu.tools.options > menu.tools.options_customization]] e dê teclas
a eles como a qualquer outro comando.

## Para escrever um plugin {#write}

1. Escolha [[menu.tools > menu.tools.plugins > menu.tools.plugin_new]]. O
   painel [[docker.scripts]] abre com um plugin de exemplo de dois comandos.
2. Mude o nome, a descrição e os comandos. Cada comando tem um `name` e
   uma função `run()`, que trabalha com o desenho como qualquer script
   (veja [Scripts e automação](scripts.md)).
3. Clique em [[docker.install_plugin]].

Um plugin define um objeto, `TraceDrawPlugin`:

```js
const TraceDrawPlugin = {
  name: 'Caixas',
  description: 'Caixas de embalagem.',
  version: '1.0',
  commands: [
    { name: 'Caixa quadrada', run() { ActiveLayer.CreateRectangle(20, 20, 50, 50); } },
  ],
};
```

As mudanças de cada comando são um passo de desfazer. O que ele imprime
vai para a saída do painel [[docker.scripts]]. Quando um comando para com
erro, a barra de status diz o motivo, e o que ele fez antes do erro fica.

## O Gerenciador de plugins {#manager}

[[menu.tools > menu.tools.plugins > menu.tools.plugin_manager]] lista os
plugins instalados com suas descrições e comandos.

- **[[dialog.plugins_on]]** Desligado, o plugin continua instalado, mas
  seus comandos saem do menu.
- **[[dialog.plugins_edit]]** Abre o plugin no painel [[docker.scripts]],
  para alterá-lo e instalá-lo de novo.
- **[[dialog.plugins_remove]]** Desinstala o plugin.

Os plugins ficam guardados com as suas preferências.
