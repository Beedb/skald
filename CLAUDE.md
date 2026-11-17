# Skáld

Reprodutor de áudio desktop para Linux, focado em arquivos locais e offline. Planejamento completo em `docs/skald-v0.3.md` (ler só a seção necessária à tarefa).

## Stack

- Interface: React + TypeScript
- App desktop: Tauri 2
- Backend: Rust
- Áudio: GStreamer
- Banco: SQLite
- Distribuição: `.deb` (Linux primeiro)

## Regras fixas

- Idioma da interface e dos textos ao usuário: português do Brasil.
- Nunca apagar arquivos de áudio do disco. Remover da biblioteca = remover só o registro.
- Arquivo ausente: manter registro, marcar como não encontrado, preservar playlists e histórico.
- Playlists guardam referências às músicas, nunca cópias.
- Nunca sobrescrever metadados silenciosamente.
- Tudo deve funcionar offline.
- Música é a única categoria ativa; o banco já tem `category` (padrão `music`) e `resume_position` (nulo) para o futuro.
- Listas grandes usam virtualização/paginação.

## Decisões principais

- Formatos: MP3, FLAC, OGG/Opus, M4A/AAC, WAV.
- Duplicados: detectar por hash e avisar, sem apagar.
- Aleatório: Shuffle Bag; fila manual sempre antes do aleatório.
- Repetir: desligado, fila, faixa.
- Crossfade desligado entre faixas do mesmo álbum (gapless).
- Música "escutada": mais de 60% da duração e no mínimo 20 s.
- Fechar a janela encerra o app (opção de bandeja nas configurações).
- Distribuição: `.deb` (Debian/Ubuntu/Zorin), `.rpm` (Fedora), `.exe` NSIS (Windows 10+) e `.apk` (Android).
- Retomar ao reabrir (última música, posição e fila) vale em todas as plataformas.
- Quando a fila ou a playlist acaba, tocar relacionadas: 1º mesmo artista, 2º mesmo gênero, 3º biblioteca inteira em aleatório (Shuffle Bag). Opção "Continuar tocando" ligada por padrão; repetir fila/faixa tem prioridade; faixas automáticas não alteram a playlist.

## Estética

- O visual usa as variáveis CSS de `docs/temas.css` (atributo `data-tema` no `:root`).
- Nunca escrever cor fixa no código; sempre `var(--...)`.
- Tema padrão: carmesim (preto com vermelho; brasa e os demais seguem em Aparência). Fontes locais via `@fontsource` (`--fonte-marca` Cinzel, `--fonte-titulo` Lora); ícones de `lucide-react`.

## Fora de escopo por enquanto

Pastas de playlists, playlists inteligentes, importar M3U, seletor de saída, equalizador manual, atalhos personalizáveis, ícones pixel art e audiolivros (podcasts: ver Fase 10).

## Como trabalhar (economizar tokens)

- Uma funcionalidade pequena por sessão. Plano de 3 a 5 linhas antes de codificar.
- Mostrar só o trecho alterado, não o arquivo inteiro.
- Não reler o projeto todo: abrir apenas os arquivos necessários.
- Commit a cada etapa que funciona.
- Erros de compilação: analisar o erro exato antes de propor mudanças.

## Ambiente

- Node via nvm (versão em `.nvmrc`; o Node do sistema é 18 e não serve ao Vite 8): `. ~/.nvm/nvm.sh && nvm use`.
- Rust em `~/.cargo/bin` (pode faltar no PATH de shells não interativos).
- Rodar: `npm run tauri dev`. Gerar `.deb`: `npm run tauri build`.
- Banco: `~/.local/share/com.skald.app/skald.db`. Migrações em `src-tauri/src/db.rs` (`PRAGMA user_version`; nunca editar migração antiga, só acrescentar).

## Ordem do MVP

Marcar `[x]` ao concluir.

- [x] Projeto base: Tauri 2 + React + TS rodando, SQLite conectado
- [x] Importar pasta e ler metadados (`lofty`)
- [x] Listar músicas na biblioteca
- [x] Tocar uma música (GStreamer): play, pausa, volume, progresso
- [x] Fila de reprodução
- [x] Mini-player e player completo
- [x] Busca por metadados
- [x] Playlists
- [x] Histórico de reprodução

Depois do MVP: monitoramento de pastas, shuffle, repetir, estatísticas, MPRIS, gapless/crossfade, ReplayGain, equalizador, letras, retrospectivas.

## Fase 2

- [x] 10. Monitoramento de pastas (`notify`)
- [x] 11. Shuffle Bag e modo repetir
- [x] 12. MPRIS e teclas de mídia
- [x] 13. Estatísticas (painel e critério de 60%)
- [x] 14. Gapless
- [x] 15. Crossfade
- [x] 16. ReplayGain
- [x] 17. Equalizador (presets)
- [x] 18. Letras (.lrc e tag)
- [x] 19. Máquina do Tempo e retrospectivas (19.1 Máquina do Tempo; 19.2 retrospectivas mensal/anual; 19.3 capas e exportação PNG)

## Fase 3 (base da biblioteca)

- [x] 20. Hash, duplicados e arquivos movidos por hash
- [x] 21. Excluir da biblioteca (nunca apaga o arquivo)
- [x] 22. Importar arquivos soltos
- [x] 23. Migração `watched_folder` com categoria + barra de categorias (só Música ativa)

## Fase 4 (interface)

- [x] 24. Temas (usar `docs/temas.css`) + opção claro/escuro/seguir sistema
- [x] 25. Barra lateral recolhível
- [x] 26. Visões de Artistas e Álbuns
- [x] 27. Tela inicial
- [x] 28. Cores dinâmicas da capa

## Fase 5 (uso diário)

- [x] 29. Fila (arrastar, limpar, adicionar álbum/playlist)
- [x] 30. Temporizador
- [x] 31. Bandeja, iniciar com o sistema, notificação ao trocar de música
- [x] 32. Backup do banco e limpar cache de capas

## Fase 6 (pesado)

- [x] 33. Apelidos de artistas
- [x] 34. Edição em lote
- [x] 35. MusicBrainz e Cover Art Archive

## Plataformas

- **Linux** (PC do Ded, `pcdoded`): `~/Programação/Skald`. Alvos `deb` e `rpm` em `src-tauri/tauri.conf.json`.
- **Android** (já existe): projeto em `src-tauri/gen/android`, GStreamer via ndk-build, `scripts/android.sh`, detalhes em `docs/android.md`. Já tem permissão de áudio, seletor de pastas, acesso aos `.lrc`, assinatura de release, layout para tela estreita e reprodução em segundo plano com notificação de mídia (MediaSession).
- **Windows 10+** (máquina `DESKTOP-8N3RCMR`): pasta `D:\DedePrograms\Skald` (sem acentos nem espaços). `src-tauri/tauri.windows.conf.json` já define NSIS (instalação por usuário, pt-BR) e GStreamer embutido (`gstreamer/bin`, `gstreamer/plugins`). Ambiente: PowerShell comum (`Set-ExecutionPolicy -Scope CurrentUser RemoteSigned`), Rust MSVC, Build Tools C++, Node 26 via nvm-windows (`nvm use` exige administrador), GStreamer 1.28.8 MSVC em `D:\programs\msvc_x86_64` (variáveis `GSTREAMER_1_0_ROOT_MSVC_X86_64`, `PKG_CONFIG_PATH` e PATH). Código só de Linux fica isolado com `#[cfg(target_os)]`; o Linux nunca pode quebrar. Instalador: `npm run build:windows` → `src-tauri/target/release/bundle/nsis/Skald_<versão>_x64-setup.exe`. Banco em `%APPDATA%\com.skald.app\skald.db`. Push com `git -c credential.helper= -c "credential.helper=!gh auth git-credential" push` (o git do Windows não tem credencial; o `gh` tem).
- Os dois PCs sincronizam pelo GitHub (`git pull` / `git push`). Editar o CLAUDE.md num lado e dar pull no outro.

## Fase 7 (ajustes de uso)

Mesmas etapas (36 a 43) acompanhadas por plataforma. O código é compartilhado: implementar uma vez e marcar `[x]` em cada plataforma só depois de testar nela.

Etapas:
- 36. Retomar ao reabrir: salvar faixa atual, posição e fila; ao abrir, mostrar a última música no player, pausada no ponto em que parou.
- 37. Fila clicável: clicar numa música da fila toca ela (a fila segue dali).
- 38. Continuar sozinho: ao fim da fila ou playlist, tocar relacionadas (ver Decisões principais) + opção "Continuar tocando" nas Configurações.
- 39. Playlist: botão "Adicionar músicas" (busca + seleção múltipla) e menu sanduíche (⋮) em popup com: Tocar em seguida, Adicionar à fila, Renomear, Escolher capa, Exportar M3U, Excluir.
- 40. Configurações mais bonitas: seções em cartões (Aparência, Reprodução, Biblioteca, Sistema, Dados), ícones, espaçamento, tudo com variáveis de `docs/temas.css`.
- 41. Barra de progresso: clicar ou arrastar muda o tempo (clicar no meio vai para o meio da música).
- 42. Só Android: foco de áudio (AudioFocus). Pausar quando outra mídia toca (ex.: vídeo do YouTube) e pausar a outra mídia quando o Skáld toca.
- 43. Só Android: um toque numa música já toca (em vez de dois toques).

### 7.1 Linux

- [x] 36. Retomar ao reabrir
- [x] 37. Fila clicável
- [x] 38. Continuar sozinho
- [x] 39. Playlist: adicionar músicas e menu ⋮
- [x] 40. Configurações mais bonitas
- [x] 41. Barra de progresso clicável/arrastável

### 7.2 Android

- [x] 36. Retomar ao reabrir
- [x] 37. Fila clicável
- [x] 38. Continuar sozinho
- [x] 39. Playlist: adicionar músicas e menu ⋮
- [x] 40. Configurações mais bonitas
- [x] 41. Barra de progresso clicável/arrastável
- [x] 42. Foco de áudio (AudioFocus)
- [x] 43. Um toque já toca

### 7.3 Windows

- [x] 36. Retomar ao reabrir
- [x] 37. Fila clicável
- [x] 38. Continuar sozinho
- [x] 39. Playlist: adicionar músicas e menu ⋮
- [x] 40. Configurações mais bonitas
- [x] 41. Barra de progresso clicável/arrastável

## Fase 8 (Windows 10+)

Conferir o que já existe antes de cada etapa e marcar `[x]` só com teste feito.

- [x] 44. Ambiente e build (`npm run tauri dev` abre a janela).
- [x] 45. Tocar de verdade: caminhos e URIs (`file:///C:/...`), gapless e crossfade.
- [x] 46. Plugins GStreamer: equalizer-10bands, rganalysis, libav (M4A/AAC).
- [x] 47. Pastas e arquivos: `audio_dir`, `app_data_dir`, monitoramento (`notify`), arquivos movidos.
- [x] 48. Controle de mídia: SMTC no lugar do MPRIS.
- [x] 49. Sistema: autostart no registro, bandeja, notificação (toast).
- [x] 50. Aparência: fontes, temas e cores dinâmicas no WebView2.
- [x] 51. Instalador `.exe` (NSIS) com GStreamer embutido, testado num Windows limpo. (Na tag v0.2.0 o job `teste-limpo` do CI instalou num `windows-latest` sem GStreamer/Rust/Node, abriu o Skáld por 25 s e conferiu banco e registro de plugins: passou. Falta só uma olhada à mão num Windows limpo, se quiser.)
- [x] 52. CI no GitHub Actions (`windows-latest`) gerando o instalador a cada tag. (Tag v0.2.0: instalador, teste-limpo e release passaram em ~8 min; o `.exe` foi anexado sozinho.)
- [x] 53. Paridade final: todas as funções das etapas 1 a 43. (1 a 35 pelos testes do Rust no Windows, 95 passando; 36 a 41 testadas no app instalado v0.2.0; 42 e 43 são só do Android.)

## Fase 9 (avisos de novas versões) — concluída no Linux

Objetivo: a pessoa cadastra o e-mail e é avisada quando sai uma versão nova. O Skáld é offline e sem servidor, então a lista e o envio ficam num serviço de fora; o app nunca guarda nem envia e-mail.

Decisões (2026-11-17):
- **Serviço**: Buttondown, plano grátis (até 100 inscritos), página `https://buttondown.com/skald` (conta do usuário). O RSS para e-mail é pago (US$ 9/mês), então o aviso sai pela API: `.github/workflows/aviso-email.yml` roda quando uma release é publicada e cria um **rascunho** (`status: "draft"`) com as notas da release e o link de download; o usuário confere e envia no Buttondown. Segredo `BUTTONDOWN_API_KEY` (chave "GitHub Actions": só Emails leitura/escrita, envio desligado, sem acesso aos inscritos). Teste manual: `gh workflow run aviso-email.yml -f tag=vX -f teste=true` (assunto com "[TESTE]"). Configurado: confirmação por e-mail, boas-vindas, sem UTM nem rastreio de abertura/clique, cor `#d61f30`.
- **Verificação no app**: entra junto, desligada por padrão ("nada fala com a internet sem pedir"). Ligada, ao abrir consulta `https://api.github.com/repos/Beedb/skald/releases/latest` (`ureq`, mesmo agente de `enriquecer`) e, se a versão for maior que a do app, mostra "Saiu a versão X" com o link da release. Sem internet: fica calado.
- Links abrem no navegador com `tauri-plugin-opener` (todas as plataformas).

Etapas (numeradas depois das da Fase 11):
- 66. Conta no Buttondown + página de inscrição. **Feita** (`https://buttondown.com/skald`).
- 67. Rascunho automático do e-mail a cada release (API, não RSS). **Feita** (teste com v0.2.0 criou o rascunho).
- 68. Link de inscrição no README e no modelo de notas da release ("Como lançar uma versão"). **Feita.**
- 69. Cartão "Novidades" (primeiro das Configurações): botão "Receber avisos…" abre `buttondown.com/skald` com `openUrl` do `tauri-plugin-opener`; a permissão `opener:allow-open-url` em `capabilities/default.json` só aceita `buttondown.com/skald*` e `github.com/Beedb/skald*`.
- 70. Verificar atualização ao abrir (opção no cartão Novidades, desligada por padrão). `novidades.rs` (`maior` compara versões numéricas; sufixo `-beta` não conta; `SKALD_VERSAO_FINGIDA` simula outra versão só em debug; ensaio: `cargo test ensaio_novidades -- --ignored --nocapture`), configuração `verificar_versao`, comandos `set_update_check`/`update_check` (async; desligado ou sem internet → nada). `src/AvisoVersao.tsx`: faixa `.aviso` no topo com "Ver novidades e baixar" (`openUrl`) e ✕ (localStorage `skald-versao-dispensada` guarda a versão fechada); ligar a opção dispara o evento de janela `verificar-versao`.

| Etapa | Linux | Android | Windows |
|---|---|---|---|
| 66 | [x] | — | — |
| 67 | [x] | — | — |
| 68 | [x] | — | — |
| 69 | [x] | [ ] | [ ] |
| 70 | [x] | [ ] | [ ] |

## Fase 10 (podcasts) — planejada

Decisões (2026-11-16): arquivos locais primeiro, assinatura por RSS depois (última etapa); nada é apagado do disco, nem episódio baixado pelo Skáld; "ouvido" = faltando ≤ 2 min ou ≥ 95% (ou marcado à mão); todas as plataformas, marcando `[x]` por plataforma depois de testar.

Modelo:
- `podcast_show` (título, autor, capa, descrição, pasta, `feed_url` opcional) e `podcast_episode` (programa, `guid`, título, data de publicação, descrição, duração, `media_id` nulo enquanto não baixado, `ouvido_em`, posição de retomada). O áudio é uma linha de `media` com `category = 'podcast'` (player, fila, MPRIS e retomar ao reabrir seguem iguais).
- Diferenças da música: retomar posição por episódio; estados novo/em andamento/ouvido; pular −15 s/+30 s e velocidade 0,8×–2× sem mudar o tom (`scaletempo`); sem aleatório, crossfade nem ReplayGain de álbum; ao fim, "Continuar tocando" vai para o próximo não ouvido do mesmo programa; podcasts fora de retrospectivas, mixes, Máquina do Tempo e relacionadas de música (histórico guarda a categoria).

Etapas:
- 54. Migração (`podcast_show`, `podcast_episode`) e importar pasta como Podcasts (programa = álbum/pasta; data e descrição das tags).
- 55. Telas Podcasts (grade de programas) e Programa (episódios do mais novo ao mais antigo, estado, filtro "não ouvidos"); item da lateral ativo.
- 56. Retomar por episódio e marcar como ouvido (automático e à mão).
- 57. Velocidade e pular −15/+30 s.
- 58. Próximo episódio ao terminar, "Continuar ouvindo" no Início, podcasts separados das estatísticas de música.
- 59. Assinar por RSS (internet só ao atualizar ou baixar, a pedido), download para a pasta do Skáld e tocar offline.

| Etapa | Linux | Android | Windows |
|---|---|---|---|
| 54 | [ ] | [ ] | [ ] |
| 55 | [ ] | [ ] | [ ] |
| 56 | [ ] | [ ] | [ ] |
| 57 | [ ] | [ ] | [ ] |
| 58 | [ ] | [ ] | [ ] |
| 59 | [ ] | [ ] | [ ] |

## Fase 11 (widget, Android Auto e sincronização pelo Drive) — antes da Fase 10

Decisões (2026-11-17): widget só no Android (relógio e app Wear OS ficam para o futuro); carro só pelo Android Auto (Waze fora); sincronização feita pelo próprio Skáld, que sobe e desce músicas e dados para o Google Drive, em todas as plataformas.

- **Widget** (60 e 61): redimensionável de 2×1 a 5×1 (só o player: capa, título/artista, anterior/tocar-pausar/próxima) e de 2×2 a 5×2 (player em cima, playlists embaixo; toque na playlist toca). Kotlin com `AppWidgetProvider`; layouts por tamanho (`RemoteViews(Map<SizeF, RemoteViews>)` no Android 12+, `onAppWidgetOptionsChanged` antes; `minSdk` 24). Atualizado por `Midia.atualizar`; botões vão por `Java_com_skald_app_Midia_comando`, como a notificação.
- **Android Auto** (62): `MidiaService` vira `MediaBrowserServiceCompat` (`onGetRoot`/`onLoadChildren`: Playlists, Álbuns, Artistas, Tocadas recentemente; tocar por id), `automotive_app_desc.xml` e meta-data no manifesto. Testar com o Desktop Head Unit (DHU) e no carro; fora da Play Store o Android Auto exige "fontes desconhecidas" nas opções de desenvolvedor dele.
- **Sincronização pelo Drive** (63 a 65):
  - Conta: OAuth com PKCE (desktop: loopback; Android: redirecionamento próprio), escopo `drive.file` (o Skáld só vê o que ele mesmo criou; sem verificação restrita do Google). O projeto no Google Cloud e o ID do cliente são criados pelo usuário. Internet só quando a pessoa liga a sincronização ou pede para sincronizar.
  - Pasta visível `Skáld` no Drive: `musicas/` (arquivos identificados pelo hash BLAKE3) e `dados/` (playlists, curtidas, histórico, apelidos).
  - Músicas: sobe as que o Drive não tem e desce as que o aparelho não tem para a pasta do Skáld (vira pasta monitorada). Nunca apaga nada: nem do disco, nem do Drive; remover da biblioteca num aparelho não remove nos outros.
  - Dados: identidade da música = hash; playlists com UUID, `atualizada_em` e marca de exclusão (vence a mais recente, itens por referência); curtidas por instante; histórico é união (hash + início). Precisa migração nova (UUIDs e datas). O `.db` nunca é copiado inteiro.

Etapas:
- 60. Widget Yx1 (player). **Feita**: `WidgetPlayer.kt` (`AppWidgetProvider`; `RemoteViews` por tamanho no Android 12+, `OPTION_APPWIDGET_MIN_WIDTH` antes; ≥ 180 dp usa `widget_largo` com capa ≤ 192 px, título e artista, menor usa `widget_estreito` só com botões), `res/xml/widget_player.xml` (2 a 5 células, 1 linha), cores do carmesim em `values/colors.xml` (`tema_*`), ícones do lucide em `drawable/widget_*`. `Midia.atualizar` chama `WidgetPlayer.atualizar` só quando música, capa ou estado mudam; o estado fica em SharedPreferences `widget_player` (com o processo morto mostra pausado). Botões: broadcast `WIDGET_ACAO` → `Midia.comando` se o Rust está carregado (`Midia.contexto`), senão abre o app (o Android permite: `BAL_ALLOW_TOKEN`). Depois de "Forçar parada" o 1º toque só redesenha o widget (limitação do Android). Instalar por cima da release com o APK release (`scripts/android.sh build --apk --target aarch64`), nunca desinstalar.
- 61. Widget Yx2 (player + playlists). **Feita**: com altura ≥ 100 dp usa `widget_alto` (≥ 180 dp de largura) ou `widget_alto_estreito`: player em cima e uma faixa de capas quadradas pequenas (como no Spotify; `adjustViewBounds` + recorte quadrado), 3 capas em 2 células, 4 em 3, 5 a partir de 4; vagas sem playlist ficam invisíveis. `midia_android::publicar_playlists` (ao iniciar e a cada `EVENTO_PLAYLISTS`, via `app.listen`) manda até 5 playlists não vazias, tocadas mais recentemente primeiro, como `id<TAB>capa` (escolhida ou miniatura da primeira música) para `Midia.playlists` → SharedPreferences. Toque: `Midia.comando("playlist", id)` → `play_playlist` (ordem personalizada). Testado no S24+: 5×2 e toque na capa.
- 62. Android Auto. **Feita**: `MidiaService` é `MediaBrowserServiceCompat` (exported, intent-filter `MediaBrowserService`; `res/xml/automotive_app_desc.xml`). Árvore: raiz → Playlists, Álbuns (grade: dica `CONTENT_STYLE_PLAYABLE_HINT` no item da pasta), Artistas, Tocadas recentemente (30). `Midia.navegar(pai)` (JNI → `midia_android::navegar`, numa thread com `result.detach()`) devolve `id<TAB>título<TAB>subtítulo<TAB>capa<TAB>p|t`; capas por `FileProvider` (+ `files-path`) com `grantUriPermission` para `com.google.android.projection.gearhead`. Tocar: `onPlayFromMediaId` → `Midia.comando(id)` → `tocar_do_carro` (`playlist:`, `album:<json ChaveAlbum>`, `artista:<json>`, `recentes:<i>`). App fechado: a raiz mostra "Abra o Skáld no celular"; item escolhido no carro fica em `MidiaService.pendente` e toca quando o Rust sobe (`Midia.pronto`, ligado pela 1ª `Midia.playlists` → `motorPronto`: `notifyChildrenChanged` + toca o pendente). **Motor sem tela não é possível com o Tauri**: abrir a atividade pelo serviço é barrado (`BAL_BLOCK`), e subir o Tauri sem tela (`WryLifecycleObserver` → `Rust.onFirstActivityCreate`) trava em `register_android_plugin` (`rx.recv()`, espera uma atividade; o `PathPlugin` do próprio Tauri também). Só daria com o motor (banco, player, fila) fora do Tauri no Android: reestruturação grande, fica para o futuro. Restou: `Motor.preparar` (GStreamer com o applicationContext, `skald_lib`, `Midia.iniciarNativo(dataDir, cacheDir)`), `caminhos.rs` (pastas sem o plugin de caminhos no Android) e `midia_android` sem `ndk_context` (JavaVM e classe `Midia` guardadas em `iniciarNativo`). Teste: DHU 2.1 (`sdkmanager --channel=3 "extras;google;auto"`, precisa de `libc++1`), no Android Auto: modo desenvolvedor, "Fontes desconhecidas", "Iniciar servidor da unidade principal"; `adb forward tcp:5277 tcp:5277` e `tail -f /dev/null | ./desktop-head-unit` (sem stdin ele fecha). Testado no DHU com o S24+ (Android Auto 18).
- Google Cloud (feito em 2026-11-17, sem faturamento): projeto `skald-515017`, Google Drive API ativada, Google Auth Platform com público Externo, escopo só `drive.file` (não confidencial), status **Testando** (login vale 7 dias; só usuários de teste cadastrados). Publicar depois exige no Branding página inicial, política de privacidade e domínio autorizado (plano: GitHub Pages `beedb.github.io/skald`), sem logotipo (logotipo obriga verificação). Clientes OAuth: "Skald desktop" (App para computador; JSON em `~/Android/chaves/google-oauth-desktop.json`, fora do repositório, nunca versionar nem colar no chat) e Android (`com.skald.app`, SHA-1 da chave de release `75:43:71:DF:D7:44:A1:74:92:A7:12:AC:E5:A9:39:92:6A:E8:77:8C`; sem JSON).
- 63. Conta Google e pasta `Skáld` no Drive (Configurações → Sincronização). **Feita no Linux**: `drive.rs` (OAuth com PKCE S256, retorno em `http://127.0.0.1:<porta livre>` com `TcpListener`, espera até 5 min, "Cancelar" via `sync_cancel`; troca o código em `oauth2.googleapis.com/token`; e-mail por `about?fields=user`; acha ou cria `Skáld`, `Skáld/musicas`, `Skáld/dados`). Conta em `<dados>/google.json` (modo 600, fora do banco para não ir no backup .db): refresh token, e-mail, nome e IDs das pastas. Sair revoga no Google (se houver internet) e apaga o arquivo; nada no Drive é apagado. Credenciais entram na compilação por `build.rs` (`SKALD_GOOGLE_CLIENT_ID`/`_SECRET` ou o JSON em `~/Android/chaves/google-oauth-desktop.json`, ou `SKALD_GOOGLE_OAUTH_JSON`); sem elas o cartão diz "Indisponível nesta versão". Falta: segredos no CI do Windows e login no Android.
- 64. Subir e descer músicas. **Envio testado no Linux; baixar falta testar (precisa de outro aparelho)**. Botão "Sincronizar agora" (cartão Sincronização, só com conta conectada; nunca automático). `drive::sincronizar_musicas`: lista `Skáld/musicas` (hash em `appProperties.hash`; os primeiros envios usavam `<hash>.<ext>` como nome e seguem reconhecidos), primeiro baixa, depois sobe (se a opção "Enviar músicas deste aparelho", `sync_enviar`, padrão ligada, estiver ligada; desligada = só baixar) e no fim lista o Drive de novo e baixa o que outro aparelho subiu durante a sincronização; sobe com o nome original do arquivo (envio retomável numa parte só, agente sem prazo total) as presentes de música cujo hash o Drive não tem (uma por hash), baixa as que nenhum registro do aparelho tem (`conhecidos` inclui ausentes) para `<Música>/Skáld/Drive` (`.parte` + confere o BLAKE3 + renomeia; nome repetido vira "x (2).mp3") e importa essa pasta com `importar_pasta` (vira monitorada). Token renovado a cada 45 min. "Parar" (`sync_cancel`) entre arquivos; falhas por arquivo não param o resto (`ResumoSync`). Progresso: evento `sincronizacao-progresso` (`listando`/`subindo`/`baixando`). Nunca apaga nada. Limitação: música removida da biblioteca (`excluded_path`) volta a ser baixada se estiver no Drive.
- 65. Sincronizar playlists, curtidas, histórico e apelidos. **Feita (falta testar entre PC e celular)**. Migração v14: `playlist.uuid` (Curtidas = `curtidas`; `playlists::criar` gera um) e `playlist_removida(uuid, removida_em)` (gravada em `playlists::excluir`). `updated_at` agora sobe também em `remover_item`, `mover_item` e ao descurtir (remover da biblioteca não conta). `sincdados.rs`: `exportar` (itens por hash), `mesclar` (playlist inteira: vence a de `atualizada_em` maior, empate fica com a deste aparelho; exclusão vale se ≥ alteração; Curtidas nunca sai; histórico = união por (início, hash ou título+artista) com o maior tempo ouvido; apelidos = união, o local vence), `aplicar` (numa transação; músicas que o aparelho não tem ficam fora da playlist). `drive::sincronizar_dados`: cada aparelho grava `Skáld/dados/aparelho-<id>.json` (`settings::aparelho_id`) e lê os de todos; arquivo com `versao` maior é ignorado. Roda no fim de "Sincronizar agora", depois das músicas. Resultado também pelo evento `sincronizacao-fim` (a tela retoma pelo `sincronizando` de `sync_status`). Android: `SyncService` (foreground `dataSync`, notificação com o progresso) ligado por `TarefaLonga` em lib.rs via `Midia.segundoPlano`; sem ele a Samsung (Freecess) congela o app e o retorno do login não chega. Música baixada no Android vai para a Música pública (`caminhos::musica`, passada pelo Kotlin em `iniciarNativo`). Não sincroniza: capa escolhida da playlist, remoção de apelido.

| Etapa | Linux | Android | Windows |
|---|---|---|---|
| 60 | — | [x] | — |
| 61 | — | [x] | — |
| 62 | — | [x] | — |
| 63 | [x] | [ ] | [ ] |
| 64 | [~] envio testado | [ ] | [ ] |
| 65 | [ ] | [ ] | [ ] |

## Estado atual

**MVP, Fase 2 (etapas 10 a 19), Fase 3 (20 a 23), Fase 4 (24 a 28), Fase 5 (29 a 32) e Fase 6 (33 a 35) concluídos; Fase 7 concluída no Linux (36 a 41), no Android (36 a 43) e no Windows (36 a 41); Fase 8 (Windows) concluída (44 a 53). Fase 9 concluída no Linux (66 a 70). Fase 11: widget (60, 61) e Android Auto (62) concluídos no Android; sincronização pelo Drive (63 a 65) feita, em teste entre Linux e Android. Versão atual: 0.2.0 (release v0.2.0); o `.deb` instalado no PC do Ded é o código de 2026-11-17 (etapa 65), ainda com o número 0.2.0.**

### Fase 7 (36 a 43; testada no Linux e no Android)

- **36 Retomar**: `Fila` e `Origem` serializáveis; configuração `retomar` (JSON `Retomar { fila, posicao_ms, origem }`) gravada por `acompanhar_retomar` (troca de faixa/estado e a cada 5 s tocando, numa thread) e no `RunEvent::Exit`. `retomar_reproducao` (thread no setup) restaura a fila (aleatório/repetir seguem as configurações) e chama `Player::carregar_pausado` (Paused, espera até 5 s, busca a posição). Sessão do histórico fica em `RetomarControle.sessao_pendente` e só começa ao tocar. Comando `player_state` (a tela pede ao abrir); `toggle_pause` sem faixa carregada toca a atual da fila.
- **37 Fila clicável**: `Fila::pular_manual` (as anteriores saem) e `pular_contexto` (manual continua antes); comando `queue_jump(manual, posicao)`; classe `.clicavel` em `Fila.tsx`.
- **38 Continuar tocando**: `colecao::continuacao` (presentes, grupos embaralhados: mesmo artista via `ARTISTAS`, mesmo gênero, resto); `Fila::continuar_com`; usado em `avancar` e em `preparar_proxima` (`Pendente.continuou`; gapless/crossfade valem), origem zerada. Configuração `continuar_tocando` (padrão ligada), comando `set_continue`, opção em Reprodução.
- **39 Playlist**: `src/AdicionarMusicas.tsx` (modal com busca em `list_media`/`count_media`, páginas de 200, virtualizada, seleção múltipla; já presentes ficam marcadas e desabilitadas; `playlist_add`). `PlaylistView` com Tocar, Adicionar músicas e ⋮ (`.menu-contexto` com Tocar em seguida, Adicionar à fila, Renomear, Escolher capa, Usar capa automática, Exportar M3U, Excluir; `.desativado` com playlist vazia).
- **40 Configurações**: componentes `Cartao`/`Linha`/`Interruptor` em `Configuracoes.tsx` (classes `cfg-*`, `.interruptor` = checkbox `role="switch"`); cartão Biblioteca (contagem via `db_status`, pastas monitoradas com "Parar de monitorar"). Empilha em ≤ 720 px.
- **41 Progresso**: `Progresso` em `PlayerBar.tsx` é uma barra própria (`.barra-progresso`, `role="slider"`, pointer capture, dica de tempo, setas ±5 s, Home/End; `pendente` segura o ponto buscado até o player confirmar ou 2 s). Busca com `SeekFlags::ACCURATE`; `AsyncDone` no deck ativo gera aviso (posição atualiza pausado).
- **42 Foco de áudio (Android)**: `Foco.kt` (`AudioFocusRequestCompat`, `AUDIOFOCUS_GAIN`, uso mídia/música), chamado em `Midia.atualizar` só quando o estado muda (`ultimo`). Tocando: pede o foco (negado → pausa). `LOSS`: pausa e solta; `LOSS_TRANSIENT`: pausa e retoma no `GAIN` (`retomarNaVolta`); "pode abaixar" fica com o ducking automático do Android. Parado: solta o foco.
- **43 Um toque (Android)**: `tocarAo` em `src/android.ts` (`onClick` no Android, `onDoubleClick` no desktop) em álbum, histórico e playlist; na biblioteca o toque toca se não houver seleção. `BotaoCurtir` e as ações da linha da playlist param a propagação.
- Android testado num Galaxy S24+ (SM-S926B) com `adb` (depuração USB autorizada; `adb install -r` às vezes falha com `INSTALL_FAILED_VERIFICATION_FAILURE` na segunda tentativa seguida, mas a primeira instala).
- **Falha intermitente do `cargo test`** (resolvida): era o `equalizer-10bands` criado pela primeira vez em duas threads ao mesmo tempo ("cannot register existing type 'GstIirEqualizerBand'" e SIGSEGV/0xc0000005); acontecia sempre que o plugin ainda não estava carregado (no Windows, com o registro de teste apagado: 2 de 2 falhas). `player::gst_iniciar` cria um equalizador uma vez (`Once`) logo após o `gst::init`; 14 de 14 rodadas ok, metade sem registro. Junto: `impl Drop for Player` (para a thread de `vigiar` e põe os decks em Null; antes o player de um teste seguia tocando até o fim do processo) e `teste_util::pasta(nome)` (pasta `<temp>/<nome>-<pid>` apagada antes: o Windows reaproveita PIDs e uma pasta não apagada, com arquivo aberto, fazia o teste de backup achar o banco velho).

### Redesenho visual (a partir das imagens do usuário, concluído)

- R1 tema carmesim padrão + fontes/ícones;
- R2 `Lateral.tsx` nova (marca Cinzel, ícones lucide, grupos Biblioteca/Playlists expansíveis em localStorage `skald-lateral-grupos`, Podcasts/Audiolivros/Outros desativados "Em breve" no lugar da antiga barra `Categorias`, música atual no rodapé) e topo com busca centralizada dentro de `.principal`.
- R3 `PlayerBar` em três colunas (faixa, controles+progresso, volume/temporizador/fila/player completo), ícones lucide, `.trilha` com preenchimento por `--p`.
- R4 `mixes.rs` (geral + até 5 por gênero mais ouvido; sem gêneros nas tags, completa com artistas via `separar_mapeado`; mínimo 5 músicas; sorteio ponderado por escutas, até 100; nada gravado), `home_data.mixes`, comando `play_mix(tipo, valor)`; `Inicio.tsx` com seções ✦ (Seu mix em cartões com mosaico de capas, Recentes, Playlists, Adicionados) e "Ver tudo".
- R5 `PlayerCompleto.tsx`: capa desfocada no fundo (`.pc-fundo`), capa + título Lora + curtir/"…" (menu da música), progresso e `Controles grande`; painel com abas Fila/Letra/Relacionados/Sobre (`src/Relacionados.tsx`, comando `related_tracks` → `colecao::relacionadas`: resto do álbum + até 20 dos artistas). Fila com miniaturas e ícone na atual. Botão no canto direito oculta o painel (localStorage `skald-player-painel-oculto`; classe `.sem-painel` centraliza capa, títulos e controles). Prévia sem Tauri: `vite` com simulação de `__TAURI_INTERNALS__` (fora do repositório).

### Publicação, Windows e Android (detalhes técnicos)

- **Publicação**: repositório Beedb/skald público (GPL-3.0, README). CLAUDE.md é versionado (sincroniza os PCs); docs/skald-v0.3.md não (.gitignore). Releases: v0.1.0 e **v0.2.0** (atual, "Latest"; Fase 7), ambas com `.deb`, `.rpm`, `.exe` e `.apk` (`.rpm` com dependências do Fedora, ainda não testado num Fedora).
- **Como lançar uma versão**: subir o número em `package.json`/`package-lock.json` (`npm version X --no-git-tag-version`), `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` e no User-Agent `CLIENTE` de `enriquecer.rs` (o `versionName` do Android vem do `tauri.conf.json`); `npm run tauri build` (gera `.deb` e `.rpm` em `src-tauri/target/release/bundle/`); commit "Versão X", tag anotada `vX` e push da tag; `gh release create vX --notes-file …` com o `.deb` e o `.rpm`; as notas terminam com "📬 Receba um aviso por e-mail a cada versão nova: https://buttondown.com/skald" (essa linha não vai para o e-mail). A tag dispara `.github/workflows/windows.yml`, que anexa o `.exe` sozinho. O `.apk` sai de `scripts/android.sh build --apk --target aarch64` e é anexado à mão como `skald-X-android-arm64.apk`. Publicar a release dispara `aviso-email.yml` (rascunho no Buttondown): enviar o rascunho só depois que `.exe` e `.apk` estiverem anexados.
- **Windows** (feito pelo usuário, commits 728685b e 0eb0405): instalador NSIS currentUser com GStreamer embutido (`scripts/gstreamer-windows.ps1`, `npm run build:windows`, `tauri.windows.conf.json`; `gstreamer_embutido()` em lib.rs), `.exe` na release v0.1.0. O panic ao abrir era `MediaControls::new` sem HWND.
- **Controles de mídia** (etapa 48, commit 00686ef): `mpris.rs` vale para Linux e Windows (souvlaki em todo desktop; `mpris_stub.rs` só para iOS); no Windows `PlatformConfig.hwnd` = `get_webview_window("main").hwnd().0` e `INDISPONIVEL` muda o texto do erro. O SMTC mantém o campo anterior quando vem `None`: no Windows `campo()` manda "" e, sem capa, vai o ícone `icons/128x128.png` (`gravar_arquivo` no cache `mpris/`). Configurações diz "Teclas de mídia e controles do sistema". Testado: play/pausa, próxima, anterior, título/capa; "anterior" depois de 3 s volta ao início. Teste: script com `GlobalSystemMediaTransportControlsSessionManager` (WinRT pelo PowerShell).
- `scripts/gstreamer-windows.ps1` copia 24 plugins e as DLLs de que dependem, achadas com `dumpbin /dependents` (30 DLLs, ~33 MB; `src-tauri/gstreamer/` no `.gitignore`). `gstreamer_embutido()` (só se existir `gstreamer-1.0/` ao lado do exe): `GST_PLUGIN_SYSTEM_PATH_1_0`, registro em `%LOCALAPPDATA%\com.skald.app\gstreamer-registry.bin`, `GST_REGISTRY_FORK=no`.
- **Autostart no Windows**: `sistema::registro` (`HKCU\...\Run`, `winreg`).
- **Toast** (testado): `sistema::app_id_windows` usa o AppUserModelID `com.skald.app` (o do atalho `Skald.lnk` que o instalador cria no Menu Iniciar; conferir com `Get-StartApps`); sem o atalho fica o id do PowerShell do notify-rust. A capa vai como imagem dentro do toast. Teste: `cargo test --lib ensaio_notificacao -- --ignored` e conferir em `[Windows.UI.Notifications.ToastNotificationManager]::History.GetHistory("com.skald.app")`.
- **Testes no Windows** (etapas 45/46): o `cargo test` põe `target\debug` no PATH, onde o Tauri copia o GStreamer embutido; essa DLL não acha plugins sozinha e os testes pulavam calados. `player::gst_iniciar()` (usado também em `rganalise`) aponta, só em teste no Windows, para a pasta `gstreamer-1.0` embutida, com registro em `%TEMP%`; os WAVs de teste são escritos em Rust (o embutido não tem `audiotestsrc`) numa pasta com espaço e acento. Gapless, crossfade e mesmo álbum passam tocando de verdade. `ensaio_formatos` toca uma música de cada extensão do banco (`SKALD_DB_ENSAIO`) e cada arquivo de `SKALD_FORMATOS_DIR`: MP3, FLAC, M4A/AAC, OGG, Opus e WAV ok com o conjunto embutido.
- **Bandeja e toast testados** pelo usuário no app instalado (`%LOCALAPPDATA%\Skald`).
- **Etapa 47** (pastas): o SQL assumia `/` como separador. `library::SEP_SQL` e `library::PASTA_SQL` (Windows aceita `\` e `/`) são usados em `colecao` (álbum por pasta sem `album_artist`), `rganalise::album`, categoria da pasta monitorada em `import::gravar` e no critério "título vindo do nome do arquivo" de `reconciliar_movidos`. `watch::processar` recebe o `Mutex<Connection>` (testável); `watch::testes::monitora_pasta_de_verdade` usa o `notify` real numa pasta com acento: novo, renomeado, movido para subpasta (mesmo registro, por hash) e apagado (fica como não encontrado). `audio_dir` = `C:\Users\<u>\Music`.
- **Etapa 50** (aparência no WebView2, Edge 157): testada pela depuração remota (`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333` e protocolo CDP com o WebSocket do Node): Cinzel/Lora carregam (pesos sob demanda), os 49 temas mudam as cores, cores da capa entram (`--acento`/`--capa` no `:root`). Corrigido (vale para todas as plataformas): nos temas `papel` e `vidro` (lateral escura, conteúdo claro) a marca, o cartão da música atual e a barra do player ficavam escuro sobre escuro; `App.css` define `--marca-cor`, `--player-fundo`, `--lateral-cartao`, `--lateral-forte` só nesses temas (os demais usam o fallback). Contraste medido com as cores renderizadas: nenhum dos 49 temas abaixo de 3 (marca) / 4,5 (texto do player).
- **Testes no Windows (pegadinhas)**: `Command::new("gst-launch-1.0")` não acha o programa (o Rust só acrescenta `.exe` a nomes sem extensão, e "1.0" conta como uma); `teste_util::gst_launch` usa `gst-launch-1.0.exe`, tira do PATH a cópia embutida e troca `\` por `/` no `location=`. WAV de teste em `teste_util::wav`. `enriquecer::testes::grava_tags_enquanto_toca`: gravar tags com o arquivo aberto pelo player funciona no Windows.
- **CI** (`.github/workflows/windows.yml`, etapa 52): `windows-latest` instala o GStreamer 1.28.8 (instalador Inno `.exe` único, `/VERYSILENT /TYPE=full /DIR=`, SHA-256 conferido, em cache), roda `npm run build:windows`; job `teste-limpo` numa máquina nova sem GStreamer instala com `/S`, abre o Skáld, confere processo vivo após 25 s, banco e registro do GStreamer embutido, e guarda uma captura da tela; job `release` (só tag `v*`) anexa o `.exe` à Release. Manual: `gh workflow run windows.yml`.
- **Ainda não testados** no Windows: exportação de retrospectivas pelo app (a função `stats::salvar_png` tem teste).
- **Fase 8 concluída em 2026-11-16**: 51 pelo job `teste-limpo` (execuções 43940034298 e a da tag v0.2.0; captura mostra a interface certa; o teste abre o app mas não toca música). 53: 36 a 41 testadas no app instalado pela depuração remota do WebView2 (`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333`, `Runtime.evaluate` chamando `window.__TAURI_INTERNALS__.invoke` e `Input.dispatchMouseEvent` para a barra): retomar após matar o processo (mesma faixa, pausada em 1:01, fila preservada), `queue_jump`, continuar tocando (mesmo artista, com crossfade; repetir fila tem prioridade), Adicionar músicas, menu ⋮ (fila, renomear, M3U em UTF-8 sem BOM; capa e excluir abrem diálogos nativos, só os comandos foram testados), cartões das Configurações, clique a 10/50/90% e arrasto. Pegadinha do CDP: aperto e soltura do mouse na mesma conexão perdem o `pointerup`; mandar cada evento numa conexão. Para testar com a biblioteca real: copiar o banco antes, criar playlist temporária e restaurar aleatório/repetir no fim.
- **Instalador sem assinatura** (SmartScreen avisa).
- **Pendente**: `panic = "abort"` no release anula o `catch_unwind` da bandeja.
- **Android** (preparação, ver `docs/android.md`): `tauri android init` em `src-tauri/gen/android`; áudio segue no GStreamer 1.28.9 (SDK em `~/Android/gstreamer/1.28.9`, NDK 27.3): `app/jni/Android.mk` gera `libgstreamer_android.so` (ndk-build rodado de dentro de `app/build/gstreamer` por `scripts/android.sh` e pela tarefa Gradle `gstreamerNdkBuild`), `build.rs` liga o Rust a ela, `SYSTEM_DEPS_*` desligam o pkg-config, `MainActivity` chama `GStreamer.init`. Bandeja/autostart/notificação em `#[cfg(desktop)]`.
- Permissão + pasta Music: ponte `window.SkaldAndroid` em `MainActivity.kt` (`temPermissao`/`pedirPermissao` → evento `skald-permissao`/`pastaMusica`), `src/android.ts`, botão "Importar Music" chama `import_folder`; "Arquivos" escondido no Android. APK debug aarch64 compila; ainda não testado em aparelho. Build: `scripts/android.sh build --apk --debug --target aarch64`.
- **Android segundo plano**: `midia_android.rs` (= `mod mpris` no Android, JNI → `Midia.atualizar`; `Java_com_skald_app_Midia_comando` volta), `Midia.kt` + `MidiaService.kt` (foreground `mediaPlayback`, MediaSession, MediaStyle), `androidx.media:media`. Layout estreito (≤ 720 px) com gaveta e margens `--seguro-*` vindas de `SkaldAndroid.insets()`. Chave de release em `~/Android/chaves/`.

### Funcionalidades por etapa (da mais recente para a mais antiga)

- **Etapa 35** (lote): `enriquecer::incompletas` (presentes sem artista, álbum, ano ou gênero; comando `incomplete_list`). `src/Completar.tsx` (vista `completar`, item ✎ na lateral): filtro por campo, lista virtualizada, "Buscar sugestões" chama `metadata_lookup` uma por 1,1 s (limite do MusicBrainz), aceita só candidato com pontuação ≥ 90 e duração a até 5 s; sugere apenas os campos vazios, como chips marcáveis; "Aplicar" usa `metadata_apply` (só Skáld ou também arquivos), sem capa. Parar interrompe; sair da tela também.
- **Etapa 34**: `enriquecer::lote_valores` (valor comum ou `varios` por campo) e `lote_aplicar` (só artista/álbum/gênero/ano, reaproveita `aplicar`; falhas por música não param o resto). Comandos `batch_values`, `batch_edit`, `media_ids_range` (Shift+clique). `Biblioteca.tsx`: seleção (clique, Ctrl, Shift, Esc limpa; zera com busca/versão). `MenuMusica` com `selecao` > 1 mostra `MenuSelecao` (tocar a seguir, fila, playlist, "Editar N músicas…"). `src/EdicaoLote.tsx`; "Editar álbum…" na tela do álbum.
- **Etapa 33**: migração v12 `artist_alias(alias_norm PK, alias, canonical)`; `apelidos.rs` (`adicionar` resolve cadeias nos dois sentidos, `remover`, `sugestoes` = nomes iguais sem espaço/pontuação, a partir de `colecao::artistas`). A troca é feita em `artistas::separar_mapeado` com `artistas::mapa`; a função SQL `ARTISTAS` guarda o mapa em cache por `GERACAO` (atômica, incrementada por `apelidos_mudaram`, inclusive ao restaurar backup) e lê via `ctx.get_connection()`. Vale para coleção, estatísticas e histórico antigo. `colecao::artistas` agora agrupa em Rust (242 → 65 ms em debug). Comandos `aliases_list`, `alias_add`, `alias_remove`, `alias_suggestions`; `src/Apelidos.tsx` (botão "Apelidos…" em Artistas).
- **Etapa 32**: `backup.rs` — `exportar` com `VACUUM INTO` (consistente com o app aberto); `restaurar` valida (tem `media`, `user_version` ≤ `db::versao_atual()`), salva o banco atual em `app_data_dir()/skald-antes-de-restaurar-<unix>.db`, usa `Connection::restore` (feature `backup`) e `db::atualizar` (migrações + busca). Comando `backup_restore` esvazia a fila e a origem; a tela recarrega com aviso via `sessionStorage` `skald-aviso`. `cache_size`/`cache_clear` apagam `miniaturas/` e `mpris/` do cache. Seção Dados em Configurações.
- **Menu de música compartilhado**: `src/MenuMusica.tsx` (`MenuMusicaProvider` em volta do App, hook `useMenuMusica()` → `abrir(e, id, { tocar })`), usado na biblioteca, início, álbum, playlist, fila e histórico; o menu nativo do WebKit (Voltar/Recarregar) é bloqueado fora de campos de texto.
- **Enriquecimento de metadados** (antecipado da etapa 35): `enriquecer.rs` consulta o MusicBrainz (`ureq`, timeout 15 s, User-Agent genérico `Skald/0.1.0`, Lucene com título e cada artista dividido; candidatos por gravação × até 3 lançamentos, pontuação ajustada pela duração, gênero = tag mais votada) e baixa a capa `front-500` do Cover Art Archive (só UUID válido). `aplicar` valida e grava só os campos `Some` no banco (`COALESCE`, `search_text` refeito) e, se `no_arquivo`, nas tags com `lofty` (tag primária, capa frontal substituída; atualiza tamanho/mtime e zera o hash). Capa: no modo arquivo é embutida (e a guardada no Skáld é apagada); no modo só Skáld vai para `app_data_dir()/capas/<id>.jpg|png` (`details::salvar_capa`/`capa_salva`/`remover_capa_salva`, pasta definida no setup) e vale por cima da do arquivo em detalhes, MPRIS, retrospectivas e miniaturas (a data da capa salva entra na chave do cache). Removida junto com o registro; não entra no backup .db. Comandos `metadata_get`, `metadata_lookup`, `metadata_apply`. `src/Informacoes.tsx`: janela pelo menu da biblioteca ("Informações e sugestões…"), busca editável, lista de candidatos, campo a campo com marcação (vazios marcados ao escolher sugestão), edição manual, destino só Skáld ou arquivo. Ensaio real: `cargo test ensaio_rede -- --ignored --nocapture`.
- **Divisão de artistas** (antecipada da Fase 6): `artistas.rs` `separar` divide créditos em `/`, `;`, feat./ft./featuring/part./participação (sem diferenciar maiúsculas, também "(feat. X)"); "AC/DC" (todas as partes com até 3 letras) não divide; `&` e vírgula não dividem. Função SQLite `ARTISTAS(texto)` (feature `functions` do rusqlite, registrada em `db::open`) devolve JSON para `json_each` (nulo → `[null]`). Usada em `colecao::artistas`, `DO_ARTISTA` (ids/“Tocar tudo”), filtro de `albuns`, ranking de artistas e dominante (`stats::COM_ARTISTAS`) e `history::estatisticas`. `Faixa.artistas` dá um link por participante na tela do álbum. O texto gravado na música não muda.
- **Etapa 31**: feature `tray-icon` do tauri; `definir_bandeja` cria o ícone (id `principal`, menu Mostrar/Tocar-Pausar/Próxima/Anterior/Sair) com `catch_unwind` (sem libayatana-appindicator3 a crate entra em pânico; o `.deb` depende de `libayatana-appindicator3-1`). Configuração `bandeja` (padrão desligada): `on_window_event` esconde em vez de fechar. `sistema.rs`: autostart = `~/.config/autostart/skald.desktop` com `--minimizado` (com bandeja, abre escondido); notificação com `notify-rust` (D-Bus) e a miniatura (`miniaturas::arquivo`), só se `notificar` (padrão ligada) e a janela fora de foco; `NotificadaState` evita repetir. Comandos `set_tray`, `set_notifications`, `autostart_get`, `set_autostart`; seção Sistema em Configurações.
- **Correções visuais** (após teste do usuário): capas das coleções usam a classe `capa-colecao` (a `.capa-mini` do mini-player fixava 44 px de altura e quebrava a grade de álbuns); `history::encerrar` apaga sessões com menos de 1 s (clique duplo tocava duas vezes); plural correto no aviso do monitoramento; título "Fila" escondido no player completo (a aba já diz).
- **Etapa 30**: `temporizador.rs` (`Modo::Ate(Instant)` ou `FimDaFaixa`, `geracao` cancela contagens antigas; `atenuacao` desce o volume nos últimos 10 s). `Player::pausar` e `Player::atenuar` (fator `Interno::atenuacao` no `alvo`). Em `lib.rs`: `TemporizadorState`, `sleep_timer_set(minutos, fim_da_faixa)` (thread `contar_temporizador` pausa e restaura o volume), `sleep_timer_get`, evento `temporizador-mudou`; no modo fim da faixa `preparar_proxima` não emenda nada e `encerrar_por_temporizador` impede o `avancar` no fim. Persiste: configuração `temporizador` (`fim` ou instante final em ms Unix; `salvar_temporizador` em cada troca/fim, `retomar_temporizador` no setup se ainda não passou; `aplicar_temporizador` centraliza). `src/Temporizador.tsx`: botão ⏾ no rodapé (15/30/45/60/90 min, fim desta música, desligar) com o tempo restante.
- **Etapa 29**: `Fila::adicionar_varios` (ordem original; `a_seguir` põe o bloco antes da fila manual), `mover_manual(de, para)`, `limpar` (esvazia a manual e corta `ordem` depois da atual; a atual segue e "anterior" ainda volta). Comandos `queue_add_many`, `queue_add_album(chave)`, `queue_add_playlist(id, ordem)`, `queue_move`, `queue_clear` (zera a origem, para música nova não reencher). `Fila.tsx`: arrastar e soltar (HTML5 DnD) na fila manual, botão "Limpar fila"; álbum e playlist ganham "Tocar em seguida"/"Adicionar à fila". O contexto não é reordenável.
- **Etapa 28**: `src/coresCapa.ts` tira a cor predominante "viva" da miniatura da capa atual (caixas de cor pontuadas por quantidade e saturação), ajusta a luminosidade até contraste ≥ 3 com `--fundo` e grava `--acento`, `--acento-txt` (`var(--fundo)` ou `var(--forte)`, o que contrastar mais) e `--capa` no estilo do :root; `@property` em App.css anima a troca (0,8 s); fundo do player completo é um gradiente de `--capa`. Evento `tema-mudou` (de `aplicarTema`) recalcula o contraste. Configuração `cores_dinamicas` (padrão ligada), comando `set_dynamic_colors`, opção "Cores da capa" em Aparência.
- **Etapa 27**: `src/Inicio.tsx` é a vista padrão (`inicio`, item ⌂ na lateral): saudação, playlists tocadas recentemente (migração v11 `playlist.played_at`, gravado em `play_playlist`; `Resumo.tocada_em`), tocadas recentemente e álbuns adicionados recentemente (comando `home_data`: `colecao::albuns_recentes` por `added_at`, `tocadas_recentes` pelo histórico). `CartaoAlbum` exportado de `Colecao.tsx`.
- **Etapa 26**: `colecao.rs` (artistas por `artist` com `SEMACENTO`; álbum = nome normalizado + `album_artist` ou pasta, agrupado em Rust numa leitura só, pois a versão SQL com collation levava 2 s; `faixas_do_album`, `ids_do_artista`). `miniaturas.rs`: capa reduzida a 256 px (crate `image`), cache em `app_cache_dir()/miniaturas/<id>-<mtime>.jpg` (`.sem` = sem capa); `details::capa_bruta`. Comandos `artists_list`, `albums_list(por_artista, artista)`, `album_tracks`, `play_album`, `play_artist`, `cover_thumb`. Interface: `src/Colecao.tsx` (Artistas virtualizada, grade de álbuns virtualizada por linhas, ArtistaView, AlbumView), `src/Capa.tsx` (miniatura preguiçosa com IntersectionObserver e cache em memória); vistas `artistas`/`artista`/`albuns`/`album` (com `de` para voltar).
- **Etapa 25**: `Lateral.tsx` recolhível (botão «/», só ícones; playlists mostram a inicial; título vira dica; estado em localStorage `skald-lateral-recolhida`; "Nova playlist" reabre a barra).
- **Etapa 24**: `docs/temas.css` importado em `main.tsx`; `src/tema.ts` (famílias de uma variante: brasa, noite, papel, vidro; demais têm `-claro`/`-escuro`; `resolverTema`, `aplicarTema` com cópia em localStorage para o primeiro desenho, segue `prefers-color-scheme` no modo sistema). Configurações `tema` (padrão brasa) e `modo_tema` (claro/escuro/sistema, padrão sistema), comando `set_appearance`; seção Aparência em `Configuracoes.tsx`. `App.css` sem cores fixas (controles nativos, lateral com `--lateral*`); `retroCanvas.ts` lê as variáveis do tema ao gerar o PNG.
- **Etapa 23**: migração v10 põe `category` (padrão `music`) em `watched_folder`; `import_folder` registra a pasta antes de importar e `gravar` grava em `media.category` a categoria da pasta monitorada mais interna que contém o arquivo (senão `music`). `src/Categorias.tsx`: barra acima da biblioteca (Música ativa; Podcasts, Audiolivros e Outros áudios desabilitados, "Em breve").
- **Etapa 22**: comando `import_files(paths)` (botão "Importar arquivos", seleção múltipla com filtro de áudio): `import::readmitir` tira os caminhos de `excluded_path` (importar de propósito traz de volta), depois `importar_arquivos` + hashes + `reconciliar_movidos` + `incluir_novas`. Arquivos soltos não viram pasta monitorada (ausência só é notada ao tocar).
- **Etapa 21**: `library::excluir` apaga só o registro (histórico fica com `media_id` nulo; itens de playlist saem por cascata) e guarda o caminho em `excluded_path` (migração v9); `import::gravar` pula caminhos excluídos (monitoramento/importação não trazem de volta; reimportar explicitamente fica para a etapa 22). `Fila::remover_ids` tira da fila manual e do contexto mantendo a posição; se a atual saiu, `remove_from_library` avança. Menu de contexto da biblioteca: "Remover da biblioteca…" com confirmação no próprio menu.
- **Etapa 20**: `import::hash_arquivo` (BLAKE3 do arquivo inteiro); `preencher_hashes` calcula os pendentes (`file_hash` nulo, presentes) fora da trava do banco, em lotes, chamado em `import_folder` e em `biblioteca_mudou` (assim a biblioteca antiga é preenchida ao abrir); `gravar` zera o hash se o `mtime` mudou no mesmo caminho. `reconciliar_movidos` também une ausente + novo com o mesmo hash (par único) e copia hash/tamanho do novo. Duplicados: `import::duplicados` (mesmo hash, presentes), migração v8 `duplicate_ignored`, comandos `duplicates_list`/`duplicate_ignore`, `ImportResumo.duplicados` (grupos que ganharam membro novo) no aviso; painel `src/Duplicados.tsx` no cabeçalho (só aparece com grupos; botão "Ignorar aviso"). Nada é apagado.
- **Etapa 19.3**: `retrospective_get` (async) preenche `Item.capa` dos top álbuns/artistas (capa de uma música); `src/retroCanvas.ts` desenha a composição num canvas de 1080 px e `retrospective_save_png` grava via `stats::salvar_png` em `audio_dir()/Skáld/Retrospectivas/<período>/retrospectiva-<período>.png` (pasta de música do sistema, aqui `~/Music`).
- **Etapa 19.2**: `stats::retrospectiva` (top 5/10/5, gênero, "mais repetida" = mais escutas num mesmo dia, "dominante" = maior fatia do tempo) e `retrospectivas_disponiveis(mes_atual)` (só meses/anos encerrados com histórico); comandos `retrospective_list`/`retrospective_get` (recusa período em andamento); `src/Retrospectivas.tsx` (composição única em fundo escuro fixo).
- **Etapa 19.1**: `stats.rs` (`painel(periodo)`: ""/AAAA/AAAA-MM em horário local; rankings de artistas/músicas/álbuns/gêneros por `counted`, agrupados com `SEMACENTO`, álbum por álbum + `COALESCE(m.album_artist, h.artist)`; evolução por mês ou dia; `meses`). Comandos `stats_period`/`stats_months`; seção Máquina do Tempo em `src/Estatisticas.tsx` (barras em CSS).
- **Etapa 18**: `letras.rs` (`.lrc` de mesmo nome ao lado, extensão sem diferenciar maiúsculas, UTF-8/Latin-1; senão tag USLT/LYRICS/©lyr; tag em formato LRC vira sincronizada; parser com vários tempos, `[offset:]` e marcas por palavra). Comando `get_lyrics`; `src/Letra.tsx` na aba "Letra" do player completo (linha atual em destaque, clique busca).
- **Busca no LRCLIB** (`buscaletras.rs`, reaproveita `enriquecer::agente`): `/api/get` com título, artista, álbum e duração; senão `/api/search` com duração a até 3 s, sincronizada antes. Comandos `lyrics_lookup` (só ao clicar) e `lyrics_save(id, texto, no_arquivo)`: só Skáld em `app_data_dir()/letras/<id>.lrc` (`letras::definir_pasta`) ou `.lrc` ao lado da música (só sincronizada, `create_new`, nunca substitui). Ordem em `carregar`: `.lrc` ao lado, guardada no Skáld, tag. Apagada com o registro; não entra no backup .db. Na aba Letra: botão "Buscar letra" quando não há letra ou ela é sem tempos, com prévia e escolha do destino. Lote: aba "Letras" em Completar informações (`src/CompletarLetras.tsx`): comando `lyrics_missing` (presentes com título e artista, sem `.lrc`/salva/tag; `letras::tem_letra`, ~2 s para 814 em debug), busca uma a cada 300 ms e guarda direto no destino escolhido antes (só Skáld ou `.lrc`; sem tempos sempre no Skáld), opção "só sincronizadas", Parar.
- **Etapa 17**: `eq.rs` (presets desligado/grave/agudo/bateria/voz/instrumental em 10 bandas, `pre_atenuacao` compensa o maior reforço); cada deck tem `equalizer-10bands` como `audio-filter` (opcional); `Player::definir_equalizador` ajusta os dois decks na hora e `pre_eq` entra em `Interno::alvo`. Configuração `equalizador`, comando `set_equalizer`. Personalizado (ajuste manual) fica para o futuro.
- **Etapa 16**: `ganho.rs` lê ReplayGain (e R128 do Opus, +5 dB) com `lofty` e calcula o fator (pico limita aumento; sem pico não aumenta). `tocar_id`/`preparar_proxima` passam o `Ganho`; no player o volume de cada deck é `Interno::alvo(i)` = volume × fator (gapless troca o ganho no `StreamStart`). Configuração `normalizacao` (desligada/faixa/album, padrão faixa), comando `set_normalization`. Arquivos sem tags: `rganalise.rs` analisa em segundo plano (`uridecodebin ! audioconvert ! audioresample ! rganalysis ! fakesink`, ~1 s por música, 300 ms entre arquivos, parado com normalização desligada) e grava em `replaygain_analise` (migração v13; `tem_tag = 1` marca arquivos com tags; refaz se o `file_mtime` mudar). `ganho_de` (lib.rs) usa as tags e, sem elas, `rganalise::do_banco` (álbum = média de energia ponderada pela duração, só com o álbum todo analisado). Comando `replaygain_status`; progresso em Configurações. Arquivos não são alterados.
- **Etapa 15**: `player.rs` tem dois decks (`playbin`); `conduzir_cruzamento` pede a próxima a `crossfade_ms` do fim: se `Proxima.mesmo_album` (`library::mesmo_album`: álbum normalizado + mesmo `album_artist` ou pasta) guarda para o gapless, senão toca no outro deck com rampa de potência constante guiada pela posição da nova. Nunca mudar estado de `playbin` segurando `Interno`. Migração v7 `setting` (chave/valor), `settings.rs` (crossfade padrão 5 s, máx. 12 s), comandos `get_settings`/`set_crossfade`, tela `src/Configuracoes.tsx`.
- **Etapa 14**: `Player::new` recebe `pedir_proxima`, chamada no `about-to-finish` do playbin (carrega a próxima URI sem parar); `StreamStart` com `pendente` gera aviso `transicao`. Em `lib.rs`, `preparar_proxima` avança uma cópia da fila (`GaplessState` guarda antes/depois) e `concluir_transicao` aplica a cópia se a fila não mudou, fecha a sessão anterior como concluída e abre a nova. Teste `player::troca_sem_pausa` gera WAVs com `gst-launch-1.0`.
- **Etapa 13**: `history::estatisticas` (tempo total, reproduções com `counted`, artistas distintos agrupados por `SEMACENTO`, todo o histórico de `category = 'music'`); comando `stats_summary`; tela `src/Estatisticas.tsx` (vista `estatisticas` na lateral, atualiza com `historico-mudou`). Apelidos de artistas (11.3) ainda não feitos.
- **Etapa 12**: `mpris.rs` (crate `souvlaki` com `use_zbus`, nome `org.mpris.MediaPlayer2.skald`). Comandos do sistema chamam `avancar`/`voltar`/`alternar_pausa`/`buscar`; `Mpris::atualizar` recebe os avisos do player e só publica mudanças (faixa, estado, salto > 1,5 s); metadados e capa (cache `mpris/capa-<id>.ext`) numa thread. Sem D-Bus o app segue sem MPRIS: `iniciar_mpris` tenta numa thread a cada 15 s (até 40 vezes), `MprisStatus` guarda o motivo, comando `mpris_status` e evento `mpris-mudou` mostram o estado em Configurações → Sistema. Testar: `busctl --user call org.mpris.MediaPlayer2.skald /org/mpris/MediaPlayer2 org.mpris.MediaPlayer2.Player PlayPause`.
- **Etapa 10**: migração v6 cria `watched_folder`; `import_folder` grava a pasta e a vigia. `watch.rs`: `Vigia` (notify, espera 2 s para agrupar) importa arquivos/pastas novos e marca sumidos (`import::marcar_sumidos`); `Vigia::retomar` sincroniza ao abrir. Reaparecer desmarca `missing` sem tocar metadados. Evento `biblioteca-mudou` (resumo) → aviso na tela e `incluir_novas`. Comandos `watched_folders`/`unwatch_folder`; tela `src/Pastas.tsx`.
- **Arquivos renomeados/movidos**: `import::reconciliar_movidos` une ausente + novo (mesma duração ±1 s e: mesmo tamanho com mesmo título/artista ou título vindo do nome do arquivo; ou título/artista/álbum iguais; par único dos dois lados), passa playlists/curtidas/histórico para o registro antigo e troca ids na fila; roda em `import_folder` e em `biblioteca_mudou` (monitoramento e varredura inicial). Ensaio numa cópia do banco: `SKALD_DB_ENSAIO=/copia.db cargo test ensaio -- --nocapture`.
- **Etapa 11**: `queue.rs`: `ordem` (posições do contexto) vira Shuffle Bag com aleatório (atual protegida no início, novo ciclo sem repetir a última); `Repetir` desligado/fila/faixa (faixa só no fim natural: `proxima_automatica`). `incluir` põe música nova entre as restantes; `lib.rs` guarda `OrigemState` (biblioteca sem busca ou playlist) e chama `incluir_novas` após importar/`playlist_add`. Comandos `set_shuffle`/`set_repeat`; modos vêm em `get_queue`; botões em `Controles`. Modos persistem (configurações `aleatorio`/`repetir`, restaurados no setup).
- **Histórico (etapa 9)**: `history.rs`, `play_history` (migração v5).

### Próximo

- Fase 9: falta testar 69 e 70 no Android e no Windows (próxima release).
- Pendente no Android: restaurar backup (Configurações → Dados) recebe `content://` do seletor; ler com `tauri_plugin_fs` como em `playlist_set_cover`.
- Fase 11, sincronização pelo Drive (estado em 2026-11-17):
  - Linux: login (63) e envio de músicas (64) testados; a 1ª sincronização completa (815 músicas, ~2,7 GB) foi interrompida algumas vezes e continua de onde parou. Falta testar a 65 (dados) entre PC e celular.
  - Android: APK com 63 a 65 instalado no S24+; falta testar login (com o `SyncService`), baixar músicas para Música/Skáld/Drive e a 65.
  - Windows (Ded faz no dia seguinte): criar os segredos `SKALD_GOOGLE_CLIENT_ID` e `SKALD_GOOGLE_CLIENT_SECRET` no repositório (`gh secret set`, lendo o JSON local) e passar os dois como `env` no passo de build de `.github/workflows/windows.yml`; para compilar na máquina Windows, copiar o JSON para `%USERPROFILE%\Android\chaves\google-oauth-desktop.json` ou definir as variáveis. Depois testar 63 a 65 lá.
  - Publicar o app no Google (sair de "Testando", login hoje expira em 7 dias): política de privacidade + página no GitHub Pages, preencher Branding, sem logotipo.
  - Opcional: substituir no Drive a versão antiga da música quando as tags são editadas no arquivo (hoje as duas ficam).
  - Cópia do banco do PC antes da migração v14: `~/.local/share/com.skald.app/skald-antes-etapa65.db`.
- Futuro (Android): motor fora do Tauri, para o Android Auto e o widget tocarem com o app fechado (ver etapa 62).
- Fase 10 (podcasts), começando pela etapa 54.
- Depois: audiolivros e itens fora de escopo.
