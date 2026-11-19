#!/usr/bin/env python3
"""Gera as páginas de ajuda do site (site/ajuda/*.html) a partir da wiki (wiki/*.md).

A mesma pasta wiki/ é publicada na Wiki do GitHub (.github/workflows/wiki.yml).
Links entre páginas seguem o formato da wiki ("Nome-da-Pagina" ou "Nome#ancora") e
viram "nome-da-pagina.html". Uso: python3 scripts/wiki-site.py  (precisa de `markdown`).
"""
import html
import re
import unicodedata
from pathlib import Path

import markdown
from markdown.extensions.toc import TocExtension

RAIZ = Path(__file__).resolve().parent.parent
WIKI = RAIZ / "wiki"
SAIDA = RAIZ / "site" / "ajuda"


def ancora(texto: str, sep: str) -> str:
    """Igual ao GitHub: minúsculas, sem pontuação, espaços viram hífens; acentos ficam."""
    texto = unicodedata.normalize("NFC", texto).strip().lower()
    texto = re.sub(r"[^\w\- ]", "", texto)
    return texto.replace(" ", "-")


def arquivo(pagina: str) -> str:
    return "index.html" if pagina == "Home" else pagina.lower() + ".html"


def links(md: str) -> str:
    # [texto](Pagina) e [texto](Pagina#ancora), sem esquema nem barra.
    def troca(m):
        alvo, resto = m.group(2), m.group(3) or ""
        return f"[{m.group(1)}]({arquivo(alvo)}{resto})"

    return re.sub(r"\[([^\]]+)\]\(([A-Z][\w\-]*)(#[^)]+)?\)", troca, md)


def converter(md: str) -> str:
    return markdown.markdown(
        links(md),
        extensions=["tables", "fenced_code", TocExtension(slugify=ancora, toc_depth="2-3")],
    )


MODELO = """<!doctype html>
<html lang="pt-BR">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{titulo} · Ajuda do Skáld</title>
  <meta name="description" content="{descricao}">
  <link rel="icon" href="../icone.png">
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=Cinzel:wght@600&family=Lora:wght@400;600&display=swap" rel="stylesheet">
  <link rel="stylesheet" href="../estilo.css?v=7">
</head>
<body>
<div class="ajuda">
  <nav class="ajuda-menu" aria-label="Páginas da ajuda">
    <a class="ajuda-marca" href="../"><img src="../icone.png" alt=""> Skáld</a>
    <details open>
      <summary>Páginas</summary>
      {menu}
    </details>
  </nav>
  <main class="ajuda-texto">
    {corpo}
    <p class="ajuda-rodape"><a href="https://github.com/Beedb/skald/wiki/{pagina}">Ver na Wiki do GitHub</a> · <a href="https://github.com/Beedb/skald/issues">Relatar um problema</a></p>
  </main>
</div>
</body>
</html>
"""


def main():
    SAIDA.mkdir(parents=True, exist_ok=True)
    menu_html = converter((WIKI / "_Sidebar.md").read_text(encoding="utf-8"))
    paginas = sorted(p for p in WIKI.glob("*.md") if not p.name.startswith("_"))
    for p in paginas:
        md = p.read_text(encoding="utf-8")
        titulo = re.search(r"^# (.+)$", md, re.M).group(1).strip()
        primeiro = next(
            (l for l in md.splitlines() if l and not l.startswith(("#", "|", "-", ">", "`"))), titulo
        )
        descricao = re.sub(r"[*_`\[\]]|\([^)]*\)", "", primeiro)[:160]
        atual = arquivo(p.stem)
        menu = menu_html.replace(f'href="{atual}"', f'href="{atual}" aria-current="page"')
        (SAIDA / atual).write_text(
            MODELO.format(
                titulo=html.escape(titulo),
                descricao=html.escape(descricao, quote=True),
                menu=menu,
                corpo=converter(md),
                pagina=p.stem,
            ),
            encoding="utf-8",
        )
    print(f"{len(paginas)} páginas em {SAIDA.relative_to(RAIZ)}")


if __name__ == "__main__":
    main()
