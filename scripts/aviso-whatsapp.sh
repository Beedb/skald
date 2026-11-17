#!/usr/bin/env bash
# Abre o WhatsApp com a nota de atualização pronta (escolha os contatos e envie).
# Uso: scripts/aviso-whatsapp.sh [vX.Y.Z]   (sem tag: a release mais recente)
# Só imprime, sem abrir: SO_TEXTO=1 scripts/aviso-whatsapp.sh
set -euo pipefail
tag="${1:-$(gh release view --json tagName -q .tagName)}"
versao="${tag#v}"
# Itens da lista das notas (linhas "- ..."), sem a linha de inscrição, até 8.
novidades=$(gh release view "$tag" --json body -q .body \
  | tr -d '\r' | grep -E '^\s*[-*] ' | grep -v 'buttondown.com' | head -n 8 \
  | sed -E 's/^\s*[-*] +/• /; s/\*\*//g; s/`//g; s/\[([^]]*)\]\([^)]*\)/\1/g' || true)
texto="🎵 Saiu o Skáld ${versao}!"
[ -n "$novidades" ] && texto+=$'\n\n'"O que mudou:"$'\n'"${novidades}"
texto+=$'\n\n'"Baixe aqui (Windows, Linux e Android): https://beedb.github.io/skald/#baixar"
if [ "${SO_TEXTO:-}" = 1 ]; then printf '%s\n' "$texto"; exit 0; fi
url="https://wa.me/?text=$(jq -rn --arg t "$texto" '$t|@uri')"
printf '%s\n\n%s\n' "$texto" "$url"
xdg-open "$url" >/dev/null 2>&1 || true
