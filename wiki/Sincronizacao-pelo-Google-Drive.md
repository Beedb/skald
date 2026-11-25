# Sincronização pelo Google Drive

Leve suas músicas e playlists entre o computador e o celular usando o **seu** Google Drive. Não há servidor do Skáld no meio.

## O que o Skáld pode ver no seu Drive

O Skáld pede só a permissão **"ver e gerenciar os arquivos que este app criou"**. Ele enxerga apenas a pasta **Skáld** que ele mesmo cria; o resto do seu Drive fica invisível para ele. A internet só é usada quando você liga a sincronização ou aperta **Sincronizar agora**.

## Conectar

1. **Configurações → Sincronização → Entrar com o Google**.
2. O navegador abre a página do Google: escolha a conta e autorize.
3. Volte ao Skáld. O cartão mostra a conta conectada e cria no Drive a pasta `Skáld`, com `musicas` e `dados`.

Faça isso em cada aparelho, com a **mesma conta**.

## Sincronizar

Clique em **Sincronizar agora** (nunca é automático). O Skáld:

1. **baixa** do Drive as músicas que este aparelho não tem, para `Música/Skáld/Drive` (essa pasta vira monitorada);
2. **envia** ao Drive as músicas deste aparelho que ainda não estão lá (com o nome original do arquivo);
3. confere o Drive de novo e baixa o que outro aparelho mandou enquanto isso;
4. junta **playlists, curtidas, histórico, apelidos de artistas e categorias** dos aparelhos.

O progresso aparece no cartão; **Parar** interrompe entre um arquivo e outro. No Android, uma notificação mantém a sincronização rodando com a tela apagada.

As músicas são reconhecidas pelo conteúdo, não pelo nome: a mesma música não é enviada duas vezes, mesmo com nomes diferentes.

### Só baixar

Desligue **Enviar músicas deste aparelho** para que este aparelho só receba músicas do Drive, sem mandar nada. Útil no celular, para não gastar espaço no Drive.

## Como as playlists se juntam

- Cada playlist é juntada inteira: vale a versão alterada **mais recentemente**.
- Playlist excluída num aparelho é excluída nos outros (a menos que tenha sido alterada depois).
- Música que o aparelho não tem fica fora da playlist até ser baixada.
- Histórico é somado; nada se perde.
- Categorias: vale o mais recente para cada música. Pôr, tirar, renomear ou excluir uma categoria num aparelho vale nos outros. Playlists inteligentes e pastas de playlists ficam em cada aparelho.

## O que nunca acontece

- **Nada é apagado**: nem do aparelho, nem do Drive.
- Remover uma música da biblioteca num aparelho não remove nos outros.
- O banco de dados inteiro nunca é copiado.

## Sair

**Sair** desconecta a conta (e revoga a permissão no Google, se houver internet). Nada no Drive é apagado.
