# Playlists e curtidas

As playlists guardam só **referências** às músicas, nunca cópias. A mesma música pode estar em várias playlists, e remover de uma playlist não mexe no arquivo.

## Curtidas

Toque no **♡** (no player ou ao lado da música na lista) para curtir. As curtidas ficam na playlist fixa **Curtidas**, que não pode ser renomeada nem excluída.

## Criar uma playlist

Na barra lateral, em **Playlists**, clique em **Nova playlist**, digite o nome e aperte **Enter** (**Esc** cancela).

## Pôr músicas na playlist

- Clique com o botão direito numa música (ou numa seleção de várias) → **Adicionar à playlist**.
- Ou abra a playlist e clique em **Adicionar músicas**: busque por título, artista ou álbum, marque quantas quiser e confirme. As que já estão na playlist aparecem marcadas.

## Dentro da playlist

- **Tocar** toca a playlist do começo.
- A ordem pode ser **Personalizada** (a sua), por **Título**, **Artista** ou **Duração**.
- Em cada música: tirar da playlist e mudar a posição.

## Menu ⋮ da playlist

- **Tocar em seguida** e **Adicionar à fila**: põem a playlist inteira na fila.
- **Renomear**.
- **Escolher capa**: usa uma imagem sua. **Usar capa automática** volta ao mosaico das capas das músicas.
- **Exportar M3U**: gera um arquivo `.m3u` que outros reprodutores abrem.
- **Excluir**: apaga só a playlist; as músicas continuam na biblioteca.

## Playlists inteligentes

Em vez de escolher as músicas uma a uma, você dá regras e o Skáld monta a playlist sozinho, e a mantém atualizada (música nova que combina entra; a que deixa de combinar sai). Na barra lateral: **Playlists → Nova playlist inteligente**.

- Regras por **artista**, **gênero**, **álbum**, **título** (contém, não contém, é, não é), **ano**, **duração**, **vezes escutada** (pelo menos, no máximo, igual), **categoria**, **curtida**, **adicionada**, **tocada** ou **não tocada nos últimos N dias**.
- Junte as regras com **todas** (precisa atender a todas) ou **qualquer** (basta uma).
- **Ordem**: embaralhada (fica igual enquanto as músicas forem as mesmas), mais tocadas, tocadas ou adicionadas recentemente, artista, título ou ano.
- **No máximo N músicas**: deixe vazio para todas.
- Enquanto edita, o Skáld mostra quantas músicas entram.

Exemplos: “Esquecidas” = curtida é sim + não tocada nos últimos 180 dias; “Samba dos anos 70” = gênero contém samba + ano pelo menos 1970 + ano no máximo 1979.

A playlist inteligente toca, vai para a fila, exporta M3U e pode ir numa pasta como as outras, mas as músicas não se adicionam nem se tiram à mão: use **Editar regras**. Ela não vai na sincronização (as regras dependem do que cada aparelho tem e tocou).

## Pastas

Para organizar muitas playlists, use **Nova pasta** na barra lateral (abaixo das playlists). Para pôr uma playlist numa pasta, abra a playlist → menu **⋮** → escolha a pasta (ou **Fora de pasta**). Clique na pasta para abrir ou fechar. Com o botão direito (no celular, toque longo) na pasta: **Renomear pasta** ou **Excluir pasta**. Excluir a pasta não exclui as playlists: elas voltam para fora de pasta. As pastas ficam só neste aparelho (não vão na sincronização).

## Importar M3U

Na barra lateral, em **Playlists → Importar M3U…**, escolha um arquivo `.m3u` ou `.m3u8` (de outro reprodutor ou exportado pelo Skáld em outro aparelho). O Skáld cria uma playlist com o nome do arquivo e põe nela as músicas **que já estão na biblioteca**: procura pelo caminho, depois pelo nome do arquivo (serve para playlists feitas em outro computador) e por fim por "Artista - Título". As que não forem achadas aparecem no aviso; nada é copiado nem importado.

## Mixes do Início

Em **Início → Seu mix**, o Skáld monta mixes com o que você mais ouve: um geral e um por gênero (ou por artista, se as músicas não tiverem gênero). São sorteados de novo a cada vez e não são gravados como playlists.
