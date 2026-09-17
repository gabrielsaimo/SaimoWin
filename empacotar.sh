#!/bin/bash
# Monta o pacote do Windows a partir do Mac: executável + a biblioteca do mpv.
#
#   ./empacotar.sh            usa a DLL já baixada em mpv/
#   ./empacotar.sh --baixar   busca a DLL nova do mpv antes
set -euo pipefail
cd "$(dirname "$0")"

VERSAO=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
DLL="mpv/libmpv-2.dll"

if [ "${1:-}" = "--baixar" ] || [ ! -f "$DLL" ]; then
  mkdir -p mpv
  echo "==> baixando a biblioteca do mpv"
  url=$(curl -s https://api.github.com/repos/shinchiro/mpv-winbuild-cmake/releases/latest \
    | python3 -c "import json,sys;d=json.load(sys.stdin);print([a['browser_download_url'] for a in d['assets'] if a['name'].startswith('mpv-dev-x86_64-2')][0])")
  curl -sL -o mpv/dev.7z "$url"
  7z x -y -ompv/extraido mpv/dev.7z >/dev/null
  cp mpv/extraido/libmpv-2.dll "$DLL"
  rm -rf mpv/extraido mpv/dev.7z
fi

echo "==> compilando para Windows"
cargo build --release --target x86_64-pc-windows-gnu -q

rm -rf dist/SaimoTV
mkdir -p dist/SaimoTV
cp target/x86_64-pc-windows-gnu/release/saimo-tv.exe "dist/SaimoTV/Saimo TV.exe"
cp "$DLL" dist/SaimoTV/
cat > dist/SaimoTV/LEIAME.txt <<TXT
Saimo TV para Windows $VERSAO

Como usar
1. Descompacte esta pasta onde quiser (por exemplo, na Área de Trabalho).
2. Abra "Saimo TV.exe". Os dois arquivos precisam ficar juntos na mesma pasta.

O que tem
  Canais ao vivo com guia de programação, e o acervo de filmes e séries com
  capa, favoritos e "continuar de onde parou".

Comandos
  Todo botão da barra mostra o atalho ao passar o mouse, e a tecla H (ou F1)
  abre a lista completa dentro do próprio app.

O aplicativo avisa quando sai versão nova e abre o download no navegador.
TXT

( cd dist && rm -f SaimoTV-Windows.zip && zip -qr SaimoTV-Windows.zip SaimoTV )
echo "pronto: dist/SaimoTV-Windows.zip ($(du -h dist/SaimoTV-Windows.zip | cut -f1))"
