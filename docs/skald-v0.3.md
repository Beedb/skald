# SKÁLD

Documento de visão e planejamento

Versão 0.3 — Novembro de 2026

---

## 1. Visão geral

O Skáld é um reprodutor de áudio desktop, inicialmente desenvolvido para Linux, com foco em arquivos locais.

A proposta é oferecer uma experiência de reprodução e organização semelhante à praticidade de serviços como Spotify e YouTube Music, mas com identidade própria e prioridade para o controle do usuário sobre sua biblioteca.

O reprodutor deve ter uma interface agradável, intuitiva e bem trabalhada, na qual o player seja protagonista, e não apenas uma ferramenta escondida dentro de uma biblioteca.

### Princípios do projeto

- Prioridade para arquivos locais.
- Funcionamento offline.
- Preservação da biblioteca e do histórico.
- Interface intuitiva e personalizável.
- Reprodução contínua e confortável.
- Organização flexível de músicas.
- Ausência de dependência obrigatória de serviços externos.

## 2. Tecnologias

| Componente | Tecnologia |
|---|---|
| Sistema inicial | Linux |
| Distribuição | `.deb` |
| Interface | React + TypeScript |
| Aplicativo desktop | Tauri 2 |
| Backend | Rust |
| Motor de áudio | GStreamer |
| Banco de dados | SQLite |

## 3. Identidade visual

### 3.1. Temas

Identidade monocromática padrão, com duas variantes: tema claro e tema escuro. Haverá possibilidade de outras paletas de cores.

### 3.2. Cores dinâmicas

Extrair as cores predominantes da capa do álbum atual para adaptar: acentos, planos de fundo, gradientes, barra de progresso e elementos de destaque.

- Transição gradual quando a música mudar.
- Leitura e contraste sempre preservados.

### 3.3. Tipografia

Fonte neutra e convencional, sem temática visual específica e sem OpenDyslexic como padrão.

### 3.4. Ícones

Inicialmente genéricos. Futuramente, identidade própria em pixel art 16×16, incluindo o logotipo.

## 4. Estrutura da interface

### 4.1. Navegação

Barra lateral permanente e recolhível.

- Expandida: ícones e nomes.
- Recolhida: somente ícones.

### 4.2. Tela inicial

- Playlists reproduzidas recentemente.
- Prévia da biblioteca.
- Mini-player fixo.
- Barra lateral de navegação.

### 4.3. Biblioteca

Visualizações: playlists, artistas, álbuns e músicas.

Haverá uma barra para alternar entre categorias de conteúdo (música, podcasts, audiolivros, outros). **Na versão inicial, apenas "Música" fica ativa** (ver seção 10). A biblioteca terá um botão para criar playlists.

### 4.4. Player completo

O mini-player fica em um canto da interface. Ao clicar nele, a tela principal é substituída pelo player completo, com:

- Capa do álbum.
- Título, artista e álbum.
- Controles de reprodução.
- Barra de progresso.
- Fila de reprodução.
- Informações adicionais.

A capa mantém sempre o mesmo tamanho e posicionamento. Ao retornar à biblioteca, a navegação preserva a posição anterior e a reprodução continua normalmente.

## 5. Biblioteca e importação

### Formatos suportados

MP3, FLAC, OGG/Opus, M4A/AAC e WAV.

### Formas de adicionar músicas

- **Importação manual:** selecionar arquivos ou pastas pelo aplicativo.
- **Monitoramento de pastas:** o Skáld detecta novos arquivos de áudio nos diretórios monitorados e os incorpora à biblioteca.

### Arquivos ausentes

Quando um arquivo desaparecer de uma pasta monitorada:

- Manter o registro da música na biblioteca.
- Marcar como não encontrado.
- Preservar playlists e histórico.
- Não apagar dados associados automaticamente.

Se uma música ausente estiver na fila, o Skáld a ignora e continua com a próxima faixa disponível.

### Arquivos duplicados

Detectar por hash do arquivo. O Skáld **avisa sem apagar nada**, e o usuário escolhe manter ou ignorar.

### Arquivos movidos

Reconhecer por hash + metadados, atualizar o caminho e manter histórico e playlists.

### Versões da mesma gravação

Ao vivo, remaster e similares são tratadas como **faixas separadas**. Sem agrupamento automático.

### Edição em lote

Disponível para artista, álbum, gênero e ano.

### Excluir da biblioteca

Remove apenas o registro. **O arquivo no disco nunca é apagado.** O histórico é preservado.

## 6. Metadados e identificação

O Skáld consulta os metadados dos arquivos locais: título, artista, álbum, capa, gênero, ano, número da faixa e outras informações disponíveis.

### Pesquisa

Considera metadados, não apenas o título.

- Pesquisar "Anirap" encontra músicas desse artista.
- Pesquisar "ani" encontra músicas cujo título ou artista contenha o trecho.

### Arquivos renomeados

Tentar reconhecer pelos metadados, evitando tratar como música nova.

### Informações ausentes

Exibir "Desconhecido" em artista ou álbum não identificado.

### Enriquecimento de metadados

O usuário costuma ter metadados preenchidos, mas o app pode sugerir informações em campos incompletos.

- **MusicBrainz:** artistas, álbuns e gravações.
- **Cover Art Archive:** capas de álbuns.

Regras:

- Integrações gratuitas e simples, sem contas ou serviços pagos.
- Não sobrescrever metadados silenciosamente.
- Permitir revisar sugestões e editar manualmente.
- Permitir escolher entre salvar nos arquivos ou só no banco do Skáld.
- Continuar funcionando offline.

## 7. Playlists

Coleções de **referências** às músicas da biblioteca. Uma música pode estar em várias playlists sem duplicar o arquivo.

Exemplo: "Invasão — Rodrigo Zin" existe uma vez na biblioteca; as playlists "HipHop" e "Batata Doce" apontam para o mesmo registro.

### Funcionamento

- Criar playlist e definir nome.
- Escolher capa (opcional). Sem capa escolhida, usa a capa da primeira música.
- Adicionar músicas existentes na biblioteca.

### Alterações

- Alterar título, artista ou capa atualiza todas as playlists.
- Remover uma música de uma playlist não a remove da biblioteca nem de outras playlists.
- **Primeira música removida:** se a capa era automática, passa a usar a da nova primeira música. Se a capa foi escolhida pelo usuário, não muda.

### Organização

- Ordem personalizada.
- Ordem alfabética por título.
- Ordem alfabética por artista.

### Favoritos

Botão de curtir em cada música. As curtidas formam uma playlist fixa chamada "Curtidas".

### Exportação

Exportar playlist em M3U (primeiro recurso de importação/exportação a entrar).

## 8. Reprodução de áudio

### 8.1. Controles

Reproduzir, pausar, avançar, voltar, volume, navegação pela duração e teclas de mídia do teclado.

### 8.2. Fila

- Reproduzir agora.
- Adicionar ao final da fila.
- Reproduzir em seguida.
- Remover música.
- Reordenar por arrastar e soltar.
- Limpar fila.

Ao adicionar um álbum ou playlist, preservar a ordem original.

### 8.3. Modo repetir

Três modos: desligado, fila inteira e faixa.

### 8.4. Reprodução aleatória (Shuffle Bag)

1. Embaralhar as músicas elegíveis.
2. Cada música toca uma vez por ciclo.
3. Músicas já tocadas não retornam ao sorteio antes do fim do ciclo.
4. A primeira música fica protegida contra repetição naquele ciclo.
5. Ao terminar, inicia novo ciclo com outra ordem.
6. Embaralhar manualmente também gera nova sequência.

Regras de interação:

- **Fila manual × aleatório:** músicas adicionadas com "Reproduzir em seguida" ou "Adicionar à fila" sempre tocam antes do Shuffle Bag.
- **Música nova durante o ciclo:** entra no ciclo atual, em posição aleatória entre as restantes.

### 8.5. Transições

- **Gapless Playback:** próxima música sem pausa perceptível.
- **Crossfade:** sobreposição gradual entre o fim de uma música e o início da seguinte, com duração configurável.
- **Regra:** crossfade desligado entre faixas consecutivas do mesmo álbum (para preservar o gapless) e ligado nos demais casos.

### 8.6. Normalização de volume

ReplayGain ou equivalente, por faixa ou por álbum. Se o arquivo não tiver os valores, o app pode analisar o áudio e calculá-los. O usuário ativa, desativa e escolhe o modo.

### 8.7. Equalizador

Presets: grave, agudo, bateria, voz, instrumental e personalizado. O ajuste manual de frequências fica para uma versão futura.

### 8.8. Saída de áudio

Usar a saída padrão do sistema (PipeWire/PulseAudio). Seletor de dispositivo fica para depois.

### 8.9. Temporizador

Encerrar a reprodução automaticamente após um período definido.

### 8.10. Encerramento do aplicativo

Por padrão, fechar a janela encerra completamente o Skáld. Opção nas configurações para enviar à bandeja.

## 9. Letras

- **Sincronizadas:** arquivos `.lrc`.
- **Estáticas:** tag embutida no arquivo.

A disponibilidade depende das informações existentes nos arquivos.

## 10. Categorias de conteúdo e terreno para o futuro

Categorias previstas: **Música**, **Podcasts**, **Audiolivros** e **Outros áudios**.

**Nesta fase, só Música é implementada.** Podcasts e audiolivros ficam para uma atualização futura, e suas funcionalidades específicas serão definidas depois. Por ora, apenas deixamos o terreno pronto:

- A tabela de mídia no SQLite tem o campo `category` (`music`, `podcast`, `audiobook`, `other`), com valor padrão `music`.
- Existe um campo opcional `resume_position` (posição de retomada em segundos), nulo para música, para uso futuro em podcasts e audiolivros.
- Cada pasta monitorada guarda a categoria associada (padrão: música).
- A barra de categorias da biblioteca já existe na interface, mas só "Música" fica habilitada.
- Estatísticas e histórico já registram a categoria de cada reprodução, para não misturar tipos de áudio no futuro.

Nada além disso é construído agora.

## 11. Histórico e estatísticas

Histórico armazenado localmente no SQLite: música reproduzida, horário de início, tempo efetivamente reproduzido, conclusão e dados para estatísticas. O histórico é preservado mesmo quando um arquivo é removido da biblioteca.

### 11.1. Critério de música escutada

Mais de 60% da duração, com **mínimo de 20 segundos** ouvidos (para não distorcer faixas muito curtas).

- **Interrompida e retomada:** soma o tempo efetivamente ouvido dentro da mesma sessão da faixa.
- **Parcial várias vezes:** soma os trechos; conta como escutada ao passar de 60%, uma vez por reprodução.

### 11.2. Painel permanente

Três indicadores:

1. Tempo total de reprodução.
2. Quantidade de músicas escutadas.
3. Quantidade de artistas distintos escutados.

Acumulam desde o primeiro uso, nunca são zerados automaticamente, consideram o histórico (não só a biblioteca atual) e não são afetados pelas retrospectivas.

### 11.3. Artistas com grafias diferentes

Tabela de apelidos (aliases) editável manualmente pelo usuário, que une grafias alternativas na contagem.

### 11.4. Máquina do Tempo

Área dedicada exclusivamente às estatísticas. Não cria playlists nostálgicas nem coleções automáticas.

- Artistas, músicas, álbuns e gêneros mais escutados.
- Tempo de reprodução.
- Evolução ao longo de meses e anos.

### 11.5. Retrospectiva mensal

Disponibilizada no primeiro dia do mês seguinte (em 1º de dezembro, a de novembro).

- Top 5 artistas.
- Top 10 músicas.
- Top 5 álbuns.
- Gênero predominante.
- Tempo total de reprodução.
- Música mais repetida.
- Artista dominante.

### 11.6. Retrospectiva anual

Disponibilizada em 1º de janeiro, com a estrutura da mensal adaptada ao ano anterior.

### 11.7. Apresentação e armazenamento

- Uma única composição visual por retrospectiva (sem stories ou cartões separados).
- Fundo escuro padrão do Skáld, blocos legíveis, capas de álbuns e imagens de artistas.
- Exportação em PNG.
- Pasta própria: `~/Músicas/Skáld/Retrospectivas/AAAA-MM/`.
- O layout definitivo será decidido no final do projeto, testando com dados reais.

## 12. Configurações

Área centralizada.

- **Aparência:** tema claro/escuro, paletas adicionais.
- **Reprodução:** tipo de transição, duração do crossfade, gapless, aleatório, normalização, temporizador, modo repetir.
- **Biblioteca:** pastas monitoradas, locais de importação, gerenciamento.
- **Áudio:** equalizador e ajustes de som.
- **Comportamento:** ação ao fechar a janela, integração com controles de mídia, iniciar com o sistema (desligado por padrão).
- **Dados:** exportar/importar backup do banco (`.db`) por botão; limpar cache de capas.

## 13. Integração com o sistema

- Teclas multimídia.
- Controles de reprodução do sistema.
- MPRIS.
- Notificações simples de reprodução.

---

## 14. Decisões tomadas (antes pendentes)

Os itens da antiga seção 14 foram resolvidos e incorporados às seções acima. Resumo:

| Tema | Decisão |
|---|---|
| Formatos | MP3, FLAC, OGG/Opus, M4A/AAC, WAV |
| Duplicados | Hash + aviso, sem apagar |
| Arquivo movido | Hash + metadados, atualiza caminho |
| Versões da gravação | Faixas separadas |
| Edição em lote | Artista, álbum, gênero, ano |
| Excluir da biblioteca | Só o registro; arquivo intacto |
| Favoritos | Sim, playlist fixa "Curtidas" |
| Primeira música removida | Capa automática atualiza; escolhida não muda |
| Repetir | Desligado, fila, faixa |
| Fila × aleatório | Fila manual tem prioridade |
| Música nova no ciclo | Entra em posição aleatória |
| Crossfade | Desligado no mesmo álbum |
| Letras | `.lrc` e tag embutida |
| Saída de áudio | Padrão do sistema |
| Equalizador | Só presets por enquanto |
| Música curta | Mínimo de 20 s + 60% |
| Interrompida/retomada | Soma na mesma sessão |
| Grafias de artistas | Tabela de apelidos manual |
| Pasta de retrospectivas | `~/Músicas/Skáld/Retrospectivas/AAAA-MM/` |
| Bandeja | Opção nas configurações |
| Iniciar com o sistema | Desligado por padrão |
| Backup | Exportar/importar `.db` |
| Cache | Capas em cache + botão de limpar |
| Podcasts e audiolivros | Atualização futura; só terreno pronto (seção 10) |

### Adiado para depois do MVP

- Pastas ou agrupamentos de playlists.
- Playlists inteligentes.
- Importação de playlists (a exportação M3U entra antes).
- Seletor de dispositivo de saída.
- Equalizador com ajuste manual de frequências.
- Atalhos de teclado e notificações personalizáveis.
- Layout definitivo das retrospectivas.

---

**Status:** planejamento funcional fechado. Próximo passo: iniciar o MVP (importar pasta → listar → tocar → fila).
