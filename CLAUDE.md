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

## Estética

- O visual usa as variáveis CSS de `docs/temas.css` (atributo `data-tema` no `:root`).
- Nunca escrever cor fixa no código; sempre `var(--...)`.
- Tema padrão: brasa.

## Fora de escopo por enquanto

Pastas de playlists, playlists inteligentes, importar M3U, seletor de saída, equalizador manual, atalhos personalizáveis, ícones pixel art, podcasts e audiolivros.

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

- [ ] 24. Temas (usar `docs/temas.css`) + opção claro/escuro/seguir sistema
- [ ] 25. Barra lateral recolhível
- [ ] 26. Visões de Artistas e Álbuns
- [ ] 27. Tela inicial
- [ ] 28. Cores dinâmicas da capa

## Fase 5 (uso diário)

- [ ] 29. Fila (arrastar, limpar, adicionar álbum/playlist)
- [ ] 30. Temporizador
- [ ] 31. Bandeja, iniciar com o sistema, notificação ao trocar de música
- [ ] 32. Backup do banco e limpar cache de capas

## Fase 6 (pesado)

- [ ] 33. Apelidos de artistas
- [ ] 34. Edição em lote
- [ ] 35. MusicBrainz e Cover Art Archive

## Estado atual

**MVP, Fase 2 (etapas 10 a 19) e Fase 3 (20 a 23) concluídos.** Etapa 23: migração v10 põe `category` (padrão `music`) em `watched_folder`; `import_folder` registra a pasta antes de importar e `gravar` grava em `media.category` a categoria da pasta monitorada mais interna que contém o arquivo (senão `music`). `src/Categorias.tsx`: barra acima da biblioteca (Música ativa; Podcasts, Audiolivros e Outros áudios desabilitados, "Em breve"). Etapa 22: comando `import_files(paths)` (botão "Importar arquivos", seleção múltipla com filtro de áudio): `import::readmitir` tira os caminhos de `excluded_path` (importar de propósito traz de volta), depois `importar_arquivos` + hashes + `reconciliar_movidos` + `incluir_novas`. Arquivos soltos não viram pasta monitorada (ausência só é notada ao tocar). Etapa 21: `library::excluir` apaga só o registro (histórico fica com `media_id` nulo; itens de playlist saem por cascata) e guarda o caminho em `excluded_path` (migração v9); `import::gravar` pula caminhos excluídos (monitoramento/importação não trazem de volta; reimportar explicitamente fica para a etapa 22). `Fila::remover_ids` tira da fila manual e do contexto mantendo a posição; se a atual saiu, `remove_from_library` avança. Menu de contexto da biblioteca: "Remover da biblioteca…" com confirmação no próprio menu. Etapa 20: `import::hash_arquivo` (BLAKE3 do arquivo inteiro); `preencher_hashes` calcula os pendentes (`file_hash` nulo, presentes) fora da trava do banco, em lotes, chamado em `import_folder` e em `biblioteca_mudou` (assim a biblioteca antiga é preenchida ao abrir); `gravar` zera o hash se o `mtime` mudou no mesmo caminho. `reconciliar_movidos` também une ausente + novo com o mesmo hash (par único) e copia hash/tamanho do novo. Duplicados: `import::duplicados` (mesmo hash, presentes), migração v8 `duplicate_ignored`, comandos `duplicates_list`/`duplicate_ignore`, `ImportResumo.duplicados` (grupos que ganharam membro novo) no aviso; painel `src/Duplicados.tsx` no cabeçalho (só aparece com grupos; botão "Ignorar aviso"). Nada é apagado. Etapa 19.3: `retrospective_get` (async) preenche `Item.capa` dos top álbuns/artistas (capa de uma música); `src/retroCanvas.ts` desenha a composição num canvas de 1080 px e `retrospective_save_png` grava via `stats::salvar_png` em `audio_dir()/Skáld/Retrospectivas/<período>/retrospectiva-<período>.png` (pasta de música do sistema, aqui `~/Music`). Etapa 19.2: `stats::retrospectiva` (top 5/10/5, gênero, "mais repetida" = mais escutas num mesmo dia, "dominante" = maior fatia do tempo) e `retrospectivas_disponiveis(mes_atual)` (só meses/anos encerrados com histórico); comandos `retrospective_list`/`retrospective_get` (recusa período em andamento); `src/Retrospectivas.tsx` (composição única em fundo escuro fixo). Etapa 19.1: `stats.rs` (`painel(periodo)`: ""/AAAA/AAAA-MM em horário local; rankings de artistas/músicas/álbuns/gêneros por `counted`, agrupados com `SEMACENTO`, álbum por álbum + `COALESCE(m.album_artist, h.artist)`; evolução por mês ou dia; `meses`). Comandos `stats_period`/`stats_months`; seção Máquina do Tempo em `src/Estatisticas.tsx` (barras em CSS). Etapa 18: `letras.rs` (`.lrc` de mesmo nome ao lado, extensão sem diferenciar maiúsculas, UTF-8/Latin-1; senão tag USLT/LYRICS/©lyr; tag em formato LRC vira sincronizada; parser com vários tempos, `[offset:]` e marcas por palavra). Comando `get_lyrics`; `src/Letra.tsx` na aba "Letra" do player completo (linha atual em destaque, clique busca). Etapa 17: `eq.rs` (presets desligado/grave/agudo/bateria/voz/instrumental em 10 bandas, `pre_atenuacao` compensa o maior reforço); cada deck tem `equalizer-10bands` como `audio-filter` (opcional); `Player::definir_equalizador` ajusta os dois decks na hora e `pre_eq` entra em `Interno::alvo`. Configuração `equalizador`, comando `set_equalizer`. Personalizado (ajuste manual) fica para o futuro. Etapa 16: `ganho.rs` lê ReplayGain (e R128 do Opus, +5 dB) com `lofty` e calcula o fator (pico limita aumento; sem pico não aumenta). `tocar_id`/`preparar_proxima` passam o `Ganho`; no player o volume de cada deck é `Interno::alvo(i)` = volume × fator (gapless troca o ganho no `StreamStart`). Configuração `normalizacao` (desligada/faixa/album, padrão faixa), comando `set_normalization`. Sem análise de áudio para arquivos sem tags. Etapa 15: `player.rs` tem dois decks (`playbin`); `conduzir_cruzamento` pede a próxima a `crossfade_ms` do fim: se `Proxima.mesmo_album` (`library::mesmo_album`: álbum normalizado + mesmo `album_artist` ou pasta) guarda para o gapless, senão toca no outro deck com rampa de potência constante guiada pela posição da nova. Nunca mudar estado de `playbin` segurando `Interno`. Migração v7 `setting` (chave/valor), `settings.rs` (crossfade padrão 5 s, máx. 12 s), comandos `get_settings`/`set_crossfade`, tela `src/Configuracoes.tsx`. Etapa 14: `Player::new` recebe `pedir_proxima`, chamada no `about-to-finish` do playbin (carrega a próxima URI sem parar); `StreamStart` com `pendente` gera aviso `transicao`. Em `lib.rs`, `preparar_proxima` avança uma cópia da fila (`GaplessState` guarda antes/depois) e `concluir_transicao` aplica a cópia se a fila não mudou, fecha a sessão anterior como concluída e abre a nova. Teste `player::troca_sem_pausa` gera WAVs com `gst-launch-1.0`. Etapa 13: `history::estatisticas` (tempo total, reproduções com `counted`, artistas distintos agrupados por `SEMACENTO`, todo o histórico de `category = 'music'`); comando `stats_summary`; tela `src/Estatisticas.tsx` (vista `estatisticas` na lateral, atualiza com `historico-mudou`). Apelidos de artistas (11.3) ainda não feitos. Etapa 12: `mpris.rs` (crate `souvlaki` com `use_zbus`, nome `org.mpris.MediaPlayer2.skald`). Comandos do sistema chamam `avancar`/`voltar`/`alternar_pausa`/`buscar`; `Mpris::atualizar` recebe os avisos do player e só publica mudanças (faixa, estado, salto > 1,5 s); metadados e capa (cache `mpris/capa-<id>.ext`) numa thread. Sem D-Bus o app segue sem MPRIS. Testar: `busctl --user call org.mpris.MediaPlayer2.skald /org/mpris/MediaPlayer2 org.mpris.MediaPlayer2.Player PlayPause`. Etapa 10: migração v6 cria `watched_folder`; `import_folder` grava a pasta e a vigia. `watch.rs`: `Vigia` (notify, espera 2 s para agrupar) importa arquivos/pastas novos e marca sumidos (`import::marcar_sumidos`); `Vigia::retomar` sincroniza ao abrir. Reaparecer desmarca `missing` sem tocar metadados. Evento `biblioteca-mudou` (resumo) → aviso na tela e `incluir_novas`. Comandos `watched_folders`/`unwatch_folder`; tela `src/Pastas.tsx`. Arquivos renomeados/movidos: `import::reconciliar_movidos` une ausente + novo (mesma duração ±1 s e: mesmo tamanho com mesmo título/artista ou título vindo do nome do arquivo; ou título/artista/álbum iguais; par único dos dois lados), passa playlists/curtidas/histórico para o registro antigo e troca ids na fila; roda em `import_folder` e em `biblioteca_mudou` (monitoramento e varredura inicial). Ensaio numa cópia do banco: `SKALD_DB_ENSAIO=/copia.db cargo test ensaio -- --nocapture`. Etapa 11: `queue.rs`: `ordem` (posições do contexto) vira Shuffle Bag com aleatório (atual protegida no início, novo ciclo sem repetir a última); `Repetir` desligado/fila/faixa (faixa só no fim natural: `proxima_automatica`). `incluir` põe música nova entre as restantes; `lib.rs` guarda `OrigemState` (biblioteca sem busca ou playlist) e chama `incluir_novas` após importar/`playlist_add`. Comandos `set_shuffle`/`set_repeat`; modos vêm em `get_queue`; botões em `Controles`. Modos não persistem entre execuções. Histórico (etapa 9): `history.rs`, `play_history` (migração v5). Próximo: etapa 24 (Fase 4: temas com `docs/temas.css`).
