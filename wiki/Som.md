# Som

Tudo em **Configurações → Reprodução**.

## Crossfade

Mistura o fim de uma música com o começo da próxima. Ajuste a duração de **0 a 12 segundos** (0 desliga; o padrão é 5 s).

## Gapless (sem pausa entre faixas)

Músicas seguidas **do mesmo álbum** emendam sem nenhum silêncio e sem crossfade, para não estragar discos ao vivo e álbuns conceituais. Não precisa ligar nada.

## Normalização de volume (ReplayGain)

Deixa as músicas com volume parecido, sem você ficar mexendo no botão:

- **Desligada**;
- **Por faixa** (padrão): cada música no mesmo volume;
- **Por álbum**: mantém a diferença de volume entre faixas do mesmo disco.

O Skáld usa as tags de ReplayGain dos arquivos. Para músicas sem essas tags, ele **analisa o volume em segundo plano** (cerca de 1 s por música) e guarda o resultado só no banco; os arquivos não são alterados. O progresso aparece logo abaixo da opção.

## Saída de áudio

No computador, em **Configurações → Reprodução → Saída de áudio**, escolha por onde o som sai (caixas, fone, HDMI, Bluetooth…). Em **Padrão do sistema** o Skáld segue a saída que estiver escolhida no sistema. A troca vale na hora, com um corte curtinho no som. Se a saída escolhida estiver desligada ao abrir o Skáld, ele usa a padrão até ela voltar (escolha de novo na lista). No Android quem decide é o próprio sistema.

No Linux, se o fone **Bluetooth ou USB** desconectar (ou a saída escolhida sumir) com música tocando, o Skáld **pausa** em vez de seguir no alto-falante. Fone de fio no conector do computador não é percebido.

## Equalizador

Presets prontos: **Desligado**, **Grave**, **Agudo**, **Bateria**, **Voz** e **Instrumental**. A mudança vale na hora.

Em **Personalizado**, aparecem 10 controles, um por faixa de frequência (dos graves, 29 Hz, aos agudos, 15 kHz), de −12 a +12 dB. O ajuste vale enquanto você arrasta e fica guardado; **Zerar** volta tudo a 0. O volume é baixado na mesma medida do maior reforço, para o som não distorcer.
