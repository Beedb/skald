# Baixar e instalar

Baixe sempre pelo site: **[beedb.github.io/skald](https://beedb.github.io/skald/#baixar)**. O botão do seu sistema já aparece em destaque. As versões antigas e as notas de cada uma ficam na [página de versões](https://github.com/Beedb/skald/releases).

## Linux: Debian, Ubuntu, Zorin, Mint (.deb)

1. Baixe o arquivo `Skald_<versão>_amd64.deb`.
2. Dê dois cliques nele e clique em **Instalar** (Central de Programas ou instalador de pacotes).
3. Ou, pelo terminal, na pasta em que o arquivo foi baixado:

   ```
   sudo apt install ./Skald_0.3.0_amd64.deb
   ```

   O `apt` instala junto o que faltar (GStreamer, que toca o áudio, e o ícone da bandeja).
4. Abra o **Skáld** pelo menu de aplicativos.

## Linux: Fedora (.rpm)

```
sudo dnf install ./Skald-0.3.0-1.x86_64.rpm
```

> O pacote `.rpm` ainda não foi testado num Fedora de verdade. Se algo falhar, conte nas [issues do GitHub](https://github.com/Beedb/skald/issues).

## Android (.apk)

O Skáld ainda não está na Play Store; a instalação é pelo arquivo `.apk` (celulares com processador de 64 bits, quase todos os atuais).

1. No celular, abra o site e toque em **Android**.
2. Abra o arquivo baixado (pela notificação ou pela pasta Downloads).
3. O Android pede para **permitir instalar apps desta fonte** (do navegador ou do gerenciador de arquivos): permita e volte.
4. Toque em **Instalar**. Se o Play Protect avisar que o app é desconhecido, toque em **Mais detalhes → Instalar mesmo assim**.
5. Ao abrir, permita o acesso às músicas.

## Windows 10 ou mais novo (.exe)

> A versão 0.3.0 para Windows sai em breve; enquanto isso, o site oferece a 0.2.0.

1. Baixe `Skald_<versão>_x64-setup.exe` e abra.
2. O instalador ainda não tem assinatura digital paga, então o Windows SmartScreen pode avisar. Clique em **Mais informações → Executar assim mesmo**.
3. A instalação é só para o seu usuário (não pede senha de administrador) e já traz tudo de que o Skáld precisa para tocar.

## Atualizar para uma versão nova

Instale a versão nova **por cima** da que você tem, do mesmo jeito que instalou a primeira:

- **Linux:** abra o `.deb` novo (ou `sudo apt install ./Skald_<nova>_amd64.deb`).
- **Android:** abra o `.apk` novo e toque em **Atualizar**. **Não desinstale antes**: desinstalar apaga os dados do app (playlists, histórico).
- **Windows:** rode o instalador novo.

Biblioteca, playlists, curtidas, histórico e configurações continuam. O banco de dados é atualizado sozinho ao abrir.

Para saber quando sai uma versão nova, veja [Sistema e avisos](Sistema-e-avisos#saber-quando-sai-uma-versão-nova).

## Desinstalar

- **Linux:** `sudo apt remove skald`
- **Android:** segure o ícone → **Desinstalar**.
- **Windows:** Configurações → Aplicativos → Skáld → Desinstalar.

Desinstalar não apaga suas músicas. Os dados do Skáld (playlists, histórico) ficam em:

| Sistema | Pasta |
|---|---|
| Linux | `~/.local/share/com.skald.app/` |
| Windows | `%APPDATA%\com.skald.app\` |
| Android | dentro do app (somem ao desinstalar; faça um [backup](Backup) antes) |
