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
- [ ] Fila de reprodução
- [ ] Mini-player e player completo
- [ ] Busca por metadados
- [ ] Playlists
- [ ] Histórico de reprodução

Depois do MVP: monitoramento de pastas, shuffle, repetir, estatísticas, MPRIS, gapless/crossfade, ReplayGain, equalizador, letras, retrospectivas.

## Estado atual

Etapa 4 concluída: `src-tauri/src/player.rs` usa um `playbin` só de áudio; comandos `play_media`, `toggle_pause`, `set_volume` (escala cúbica) e `seek`; uma thread lê o barramento e emite o evento `player-estado` (posição a cada 250 ms, fim de faixa, erros). Tocar arquivo inexistente marca `missing = 1`. Front: duplo clique na lista toca; `src/PlayerBar.tsx` tem play/pausa, progresso e volume. Teste: `SKALD_TEST_DIR=/pasta cargo test -- --test-threads=1` (pasta com `a.mp3` e `sub/quebrado.ogg`). Próximo: fila de reprodução.
