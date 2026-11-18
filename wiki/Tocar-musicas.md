# Tocar músicas

## O player

A barra de baixo mostra a música atual e os controles:

- **Aleatório**, **Anterior**, **Tocar/Pausar**, **Próxima**, **Repetir**.
- **Barra de progresso**: clique em qualquer ponto para ir até ele ou arraste. Com o foco na barra, as setas voltam ou avançam 5 s, e **Home**/**End** vão para o início ou o fim.
- **♡**: curte a música (ela entra em [Curtidas](Playlists-e-curtidas)).
- **Volume** e silenciar (no computador).
- **Temporizador** (☾), **fila** e **player completo** (↗).

"Anterior" depois de 3 s de música volta ao começo dela; antes disso, vai para a música anterior.

## Player completo

Clique na capa ou no botão de player completo. Mostra a capa grande sobre um fundo com as cores dela e um painel com as abas:

- **Fila**: o que vem a seguir.
- **Letra**: a letra, com a linha atual em destaque quando é sincronizada (veja abaixo).
- **Relacionados**: o resto do álbum e outras músicas dos mesmos artistas.
- **Sobre**: álbum, gênero, faixa, qualidade, tamanho e caminho do arquivo.

O botão no canto direito esconde o painel e centraliza a capa. **Esc** ou **Voltar** fecha o player completo.

## Fila

Abra pelo botão de fila no player.

- **Tocando agora**: a música atual.
- **Próximas na fila**: o que você mandou tocar com "Tocar em seguida" ou "Adicionar à fila". **Arraste** para mudar a ordem. Essa fila manual sempre toca antes do resto.
- **A seguir**: o resto do álbum, playlist ou lista que você está ouvindo.

Clique numa música da fila para tocá-la; a fila segue a partir dela. **Limpar fila** tira tudo o que viria depois da música atual.

Álbuns e playlists têm também **Tocar em seguida** e **Adicionar à fila** para colocar todos de uma vez.

## Aleatório

Liga e desliga no botão **Aleatório**. O Skáld usa uma "sacola embaralhada": cada música toca uma vez antes de qualquer uma repetir, e a fila manual sempre vem antes do aleatório.

## Repetir

O botão **Repetir** alterna entre três modos:

1. **Desligado**: para no fim.
2. **Fila**: recomeça a lista quando ela acaba.
3. **Faixa**: repete a música atual.

## Continuar tocando

Quando a fila ou a playlist acaba (e o repetir está desligado), o Skáld continua sozinho com músicas relacionadas: primeiro do **mesmo artista**, depois do **mesmo gênero**, depois da **biblioteca inteira** em ordem aleatória. A playlist não muda.

Desligue em **Configurações → Reprodução → Continuar tocando**.

## Temporizador

O botão ☾ pausa a música depois de **15, 30, 45, 60 ou 90 minutos** ou **no fim desta música**. Nos últimos 10 s o volume desce devagar. **Desligar** cancela. O tempo restante aparece no botão.

## Retomar de onde parou

Ao fechar e abrir o Skáld, a última música aparece no player pausada no ponto em que parou, com a mesma fila. É só apertar Tocar.

## Letras

Na aba **Letra** do player completo, o Skáld mostra, nesta ordem:

1. um arquivo `.lrc` com o mesmo nome da música, na mesma pasta;
2. uma letra que você salvou pelo Skáld;
3. a letra gravada nas tags do arquivo.

Letras **sincronizadas** (com tempos) destacam a linha atual; clique numa linha para ir até aquele trecho.

Sem letra? Clique em **Buscar letra na internet (LRCLIB)**. O Skáld envia só título, artista, álbum e duração ao [lrclib.net](https://lrclib.net), mostra a letra encontrada e pergunta onde guardar:

- **só no Skáld** (não mexe nos seus arquivos);
- ou como arquivo **`.lrc` ao lado da música** (só para letras sincronizadas; nunca substitui um `.lrc` que já exista).

Para buscar letras de muitas músicas de uma vez, use **Completar informações → Letras** (veja [Organizar a biblioteca](Organizar-a-biblioteca#letras-em-lote)).

## Teclas de mídia

As teclas ⏯ ⏭ ⏮ do teclado, do fone Bluetooth e os controles do sistema funcionam com o Skáld:

- **Linux:** pelo MPRIS (aparece no painel de som do GNOME/Zorin/KDE).
- **Windows:** nos controles de mídia do Windows.
- **Android:** na notificação, na tela de bloqueio e no fone.

O estado aparece em **Configurações → Sistema → Teclas de mídia e controles do sistema**.
