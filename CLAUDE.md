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
- [ ] 13. Estatísticas (painel e critério de 60%)
- [ ] 14. Gapless
- [ ] 15. Crossfade
- [ ] 16. ReplayGain
- [ ] 17. Equalizador (presets)
- [ ] 18. Letras (.lrc e tag)
- [ ] 19. Máquina do Tempo e retrospectivas

## Estado atual

**MVP concluído; etapas 10 a 12 feitas.** Etapa 12: `mpris.rs` (crate `souvlaki` com `use_zbus`, nome `org.mpris.MediaPlayer2.skald`). Comandos do sistema chamam `avancar`/`voltar`/`alternar_pausa`/`buscar`; `Mpris::atualizar` recebe os avisos do player e só publica mudanças (faixa, estado, salto > 1,5 s); metadados e capa (cache `mpris/capa-<id>.ext`) numa thread. Sem D-Bus o app segue sem MPRIS. Testar: `busctl --user call org.mpris.MediaPlayer2.skald /org/mpris/MediaPlayer2 org.mpris.MediaPlayer2.Player PlayPause`. Etapa 10: migração v6 cria `watched_folder`; `import_folder` grava a pasta e a vigia. `watch.rs`: `Vigia` (notify, espera 2 s para agrupar) importa arquivos/pastas novos e marca sumidos (`import::marcar_sumidos`); `Vigia::retomar` sincroniza ao abrir. Reaparecer desmarca `missing` sem tocar metadados. Evento `biblioteca-mudou` (resumo) → aviso na tela e `incluir_novas`. Comandos `watched_folders`/`unwatch_folder`; tela `src/Pastas.tsx`. Arquivo movido ainda vira ausente + novo (falta preencher `file_hash`). Etapa 11: `queue.rs`: `ordem` (posições do contexto) vira Shuffle Bag com aleatório (atual protegida no início, novo ciclo sem repetir a última); `Repetir` desligado/fila/faixa (faixa só no fim natural: `proxima_automatica`). `incluir` põe música nova entre as restantes; `lib.rs` guarda `OrigemState` (biblioteca sem busca ou playlist) e chama `incluir_novas` após importar/`playlist_add`. Comandos `set_shuffle`/`set_repeat`; modos vêm em `get_queue`; botões em `Controles`. Modos não persistem entre execuções. Histórico (etapa 9): `history.rs`, `play_history` (migração v5). Próximo: etapa 13 (estatísticas, dados já existem).
