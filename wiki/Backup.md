# Backup

Em **Configurações → Dados**.

## O que entra no backup

O backup é uma cópia do banco do Skáld: biblioteca, **playlists, curtidas, histórico**, apelidos, configurações e podcasts assinados. **As músicas não entram** (elas já estão nas suas pastas).

Capas e letras salvas "só no Skáld" também não entram no arquivo de backup.

## Fazer backup

**Backup → Exportar…** e escolha onde salvar. Dá para fazer com o Skáld tocando. Guarde o arquivo fora do aparelho (Drive, pendrive, outro computador).

## Restaurar

**Backup → Restaurar…** e escolha o arquivo. O Skáld:

1. confere se o arquivo é mesmo um backup do Skáld;
2. guarda uma cópia do banco atual antes de trocar (`skald-antes-de-restaurar-<data>.db`, na pasta de dados);
3. restaura e recarrega a tela.

Um backup de uma versão antiga é atualizado sozinho. Um backup de uma versão **mais nova** que a instalada é recusado: atualize o Skáld primeiro.

## Cache de capas

As miniaturas das capas ficam num cache para a tela abrir rápido. **Limpar** libera o espaço; elas são refeitas quando precisar.
