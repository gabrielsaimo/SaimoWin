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

for arg in "$@"; do
  case "$arg" in
    --baixar) python3 scripts/verificar_mpv.py --baixar ;;
    --assinado) [ -n "${SAIMO_ASSINAR:-}" ] || { echo 'Configure SAIMO_ASSINAR com o assinador autorizado antes de gerar uma distribuição assinada.'; exit 1; } ;;
    *) echo "Opção desconhecida: $arg"; exit 1 ;;
  esac
done
python3 scripts/verificar_mpv.py

echo "==> compilando para Windows"
cargo build --release --target x86_64-pc-windows-gnu -q

rm -rf dist/SaimoTV
mkdir -p dist/SaimoTV
cp target/x86_64-pc-windows-gnu/release/saimo-tv.exe "dist/SaimoTV/Saimo TV.exe"
cp "$DLL" dist/SaimoTV/
if [ -n "${SAIMO_ASSINAR:-}" ]; then
  "$SAIMO_ASSINAR" "dist/SaimoTV/Saimo TV.exe"
fi
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
if [ -n "${SAIMO_ASSINAR:-}" ]; then
  "$SAIMO_ASSINAR" "dist/SaimoTV-Instalador.msi"
else
  echo 'AVISO: pacote de teste SEM assinatura de editor; não foi confirmado como falso positivo.'
fi
shasum -a 256 'dist/SaimoTV/Saimo TV.exe' dist/SaimoTV/libmpv-2.dll dist/SaimoTV-Instalador.msi > dist/SHA256SUMS.txt

[ -f dist/SaimoTV-Instalador.msi ] || { echo "o instalador não foi gerado"; exit 1; }
echo "pronto: dist/SaimoTV-Instalador.msi ($(du -h dist/SaimoTV-Instalador.msi | cut -f1))"
