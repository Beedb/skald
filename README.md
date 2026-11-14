<p align="center">
  <img src="docs/icone.png" alt="Ícone do Skáld" width="128">
</p>

<h1 align="center">Skáld</h1>

<p align="center">
  Reprodutor de música para Linux e Windows, feito para quem tem a própria coleção de arquivos.<br>
  Local, offline e sem conta.
</p>

---

O Skáld organiza e toca as músicas que estão no seu computador. Ele não faz streaming, não exige login e funciona sem internet. A conexão só é usada quando você pede, para buscar letras ou completar informações das músicas.

## Recursos

**Biblioteca**
- Importa pastas ou arquivos soltos: MP3, FLAC, OGG/Opus, M4A/AAC e WAV
- Monitora as pastas: músicas novas entram sozinhas e arquivos que sumiram ficam marcados como não encontrados, sem perder playlists nem histórico
- Reconhece arquivos movidos ou renomeados e avisa sobre duplicados (nada é apagado do disco)
- Visões de Artistas e Álbuns, busca por metadados e playlists
- Apelidos de artistas, para juntar grafias diferentes do mesmo nome, e divisão de participações (feat., `/`, `;`)
- Edição em lote e sugestões do MusicBrainz e do Cover Art Archive. Você escolhe se a alteração fica só no Skáld ou também nas tags do arquivo

**Reprodução**
- Fila com arrastar e soltar, aleatório (Shuffle Bag) e repetir fila ou faixa
- Gapless entre faixas do mesmo álbum e crossfade entre álbuns diferentes
- ReplayGain (por faixa ou álbum, com análise própria para arquivos sem tag)
- Equalizador com presets
- Letras sincronizadas (`.lrc` ou tag), com busca no LRCLIB
- Temporizador para dormir, com o volume baixando devagar
- Teclas de mídia e integração com o painel do sistema (MPRIS, só no Linux por enquanto), ícone na bandeja e notificação ao trocar de música

**Histórico e estatísticas**
- Histórico de reprodução: uma música conta como escutada a partir de 60% da duração, com no mínimo 20 s
- Máquina do Tempo: rankings de artistas, músicas, álbuns e gêneros por mês ou ano
- Retrospectivas mensais e anuais, exportáveis em PNG
- Mixes automáticos a partir do que você mais ouve

**Aparência**
- Vários temas, com modo claro, escuro ou seguindo o sistema
- Cores que acompanham a capa da música atual
- Player completo com capa, letra, fila e músicas relacionadas

## Instalação

Baixe o pacote do seu sistema na página de [Releases](../../releases/latest).

### Windows 10 e 11

1. Baixe `Skald_0.1.0_x64-setup.exe` e abra.
2. Não precisa ser administrador: o Skáld é instalado só para o seu usuário.
3. Se aparecer "O Windows protegeu o computador", clique em **Mais informações → Executar assim mesmo** (o instalador ainda não tem assinatura digital).

O GStreamer já vem dentro do instalador. Se faltar o WebView2 (já incluso no Windows 11 e no Windows 10 atualizado), o instalador o baixa na hora. Para desinstalar: Configurações → Aplicativos → Skald.

### Ubuntu, Debian, Linux Mint, Zorin OS, Pop!_OS…

```bash
sudo apt install ./Skald_0.1.0_amd64.deb
```

### Fedora

```bash
sudo dnf install ./Skald-0.1.0-1.x86_64.rpm
```

Para tocar M4A/AAC no Fedora, o pacote `gstreamer1-plugin-libav` precisa estar instalado. Normalmente o `dnf` já instala junto; se não instalar:

```bash
sudo dnf install gstreamer1-plugin-libav
```

Os pacotes são para computadores de 64 bits (x86_64).

## Onde ficam os seus dados

| O quê | Onde |
|---|---|
| Banco da biblioteca, playlists e histórico | `~/.local/share/com.skald.app/skald.db` |
| Capas e letras guardadas só no Skáld | `~/.local/share/com.skald.app/` |
| Miniaturas (pode apagar sem problema) | `~/.cache/com.skald.app/` |
| Retrospectivas exportadas | `Skáld/Retrospectivas/` dentro da pasta Música do sistema |

No Windows, o banco, as capas e as letras ficam em `%APPDATA%\com.skald.app\` e as miniaturas em `%LOCALAPPDATA%\com.skald.app\`.

Em Configurações → Dados dá para fazer e restaurar o backup do banco. Remover uma música da biblioteca apaga só o registro, nunca o arquivo.

## Compilar a partir do código

Requisitos: Node.js (versão em `.nvmrc`), Rust estável e as bibliotecas de desenvolvimento do Tauri 2 e do GStreamer. No Ubuntu/Debian:

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  librsvg2-dev libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
  gstreamer1.0-plugins-good gstreamer1.0-libav
```

Depois:

```bash
npm install
npm run tauri dev     # rodar em modo de desenvolvimento
npm run tauri build   # gerar o .deb e o .rpm em src-tauri/target/release/bundle/
```

No Windows: instale o Visual Studio Build Tools (C++), o Rust e os pacotes *runtime* e *development* do GStreamer MSVC x86_64 ([gstreamer.freedesktop.org](https://gstreamer.freedesktop.org/download/)). `npm run build:windows` copia para o instalador só a parte do GStreamer que o Skáld usa e gera o `.exe` em `src-tauri/target/release/bundle/nsis/`.

Feito com [Tauri 2](https://tauri.app), React + TypeScript, Rust, GStreamer e SQLite.

## Licença

[GPL-3.0](LICENSE). Você pode usar, estudar, modificar e redistribuir o Skáld, desde que as versões distribuídas continuem sob a mesma licença.
