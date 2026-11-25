# Sincronização sem nuvem (Syncthing)

Sem espaço no Google Drive? Dá para sincronizar direto entre os seus aparelhos, sem limite de tamanho e sem passar por nuvem nenhuma, usando o [Syncthing](https://syncthing.net) (grátis). Quem leva os arquivos de um aparelho ao outro é o Syncthing; o Skáld só lê e grava arquivos numa pasta.

## Instalar o Syncthing

- **Linux**: `sudo apt install syncthing` e depois `systemctl --user enable --now syncthing`. O painel abre em `http://127.0.0.1:8384`.
- **Android**: instale o **Syncthing-Fork** (F-Droid ou Play Store). Na Samsung, tire o app da otimização de bateria, senão ele para de sincronizar.
- **Windows**: SyncTrayzor ou o instalador do site.

Pareie os aparelhos trocando o **ID** de cada um (no celular dá para ler o QR code do painel).

## Músicas

No Syncthing, compartilhe a sua pasta de música (por exemplo `~/Music` no PC e `Music` no celular). No Skáld de cada aparelho, essa pasta precisa estar importada: ela fica monitorada e as músicas que chegam entram sozinhas.

> **Atenção:** o Syncthing espelha exclusões (apagou num aparelho, apaga no outro). Ligue **Versionamento de arquivos → Lixeira simples** na pasta, ou marque o celular como **Somente receber**.

## Playlists, curtidas, histórico, apelidos e categorias

1. Crie uma pasta dentro da que o Syncthing sincroniza, por exemplo `Music/Skáld/dados`.
2. No Skáld: **Configurações → Sincronização → Dados por pasta (Syncthing) → Escolher pasta…**
3. Repita em cada aparelho, apontando para a mesma pasta sincronizada.

Cada aparelho grava ali o arquivo `aparelho-<id>.json` e lê os dos outros. O Skáld junta tudo **ao abrir, a cada 5 minutos e ao fechar**, ou quando você aperta **Sincronizar dados agora**. As regras são as mesmas da [sincronização pelo Google Drive](Sincronizacao-pelo-Google-Drive#como-as-playlists-se-juntam): vale a playlist alterada mais recentemente e o histórico é somado.

Cópias de conflito do Syncthing (`.sync-conflict`) também são lidas, sem problema. **Desligar** só para de usar a pasta; nenhum arquivo é apagado.

Dá para usar junto com o Google Drive ou no lugar dele.
