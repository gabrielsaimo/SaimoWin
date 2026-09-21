#!/bin/bash
# Monta o instalador do Windows a partir do Mac: executável, a biblioteca do
# mpv e um instalador MSI que põe tudo no lugar.
#
# Antes isto entregava um ZIP com os dois arquivos soltos, e quem clicava no
# .exe de dentro da janela do compactador ganhava um programa sem vídeo: o
# Windows copia só o executável para o Temp e deixa a DLL para trás.
#
#   ./empacotar.sh            usa a DLL já baixada em mpv/
#   ./empacotar.sh --baixar   busca a DLL nova do mpv antes
#
# Precisa do wixl (brew install msitools).
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

O que tem
  Canais ao vivo com guia de programação, e o acervo de filmes e séries com
  capa, favoritos e "continuar de onde parou".

Comandos
  Todo botão da barra mostra o atalho ao passar o mouse, e a tecla H (ou F1)
  abre a lista completa dentro do próprio app.

Onde ficou instalado
  %LOCALAPPDATA%\\Programs\\Saimo TV — só para este usuário, sem precisar de
  senha de administrador. Para remover: Configurações > Aplicativos, ou o
  atalho "Desinstalar o Saimo TV" no menu Iniciar.

Seus favoritos e o ponto onde cada filme parou ficam em %APPDATA%\\SaimoTV, e
a desinstalação pergunta antes de apagá-los.

O aplicativo avisa quando sai versão nova e abre o download no navegador.
TXT

echo "==> montando o instalador"
command -v wixl >/dev/null || { echo "falta o wixl: brew install msitools"; exit 1; }
rm -f dist/SaimoTV-Instalador.msi
wixl -a x64 --extdir instalador --ext ui -D Versao="$VERSAO" \
     -o dist/SaimoTV-Instalador.msi instalador.wxs

# O wixl não escreve a tabela de caracteres do banco, e sem ela o Windows lê os
# acentos com a tabela da máquina — "instalação" vira "instalaÃ§Ã£o" em quem
# não estiver no 1252. O texto já sai gravado em 1252; isto só o diz.
printf '\r\n\r\n1252\t_ForceCodepage\r\n' > dist/_ForceCodepage.idt
msibuild dist/SaimoTV-Instalador.msi -i dist/_ForceCodepage.idt
rm -f dist/_ForceCodepage.idt

[ -f dist/SaimoTV-Instalador.msi ] || { echo "o instalador não foi gerado"; exit 1; }
echo "pronto: dist/SaimoTV-Instalador.msi ($(du -h dist/SaimoTV-Instalador.msi | cut -f1))"
