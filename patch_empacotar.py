import re

with open("empacotar.sh", "r") as f:
    text = f.read()

# Replace MSI logic with ZIP logic
text = text.replace(
    'echo "==> montando o instalador"\ncommand -v wixl >/dev/null || { echo "falta o wixl: brew install msitools"; exit 1; }\nrm -f dist/SaimoTV-Instalador.msi\nwixl -a x64 --extdir instalador --ext ui -D Versao="$VERSAO" \\\n     -o dist/SaimoTV-Instalador.msi instalador.wxs',
    'echo "==> montando o zip"\nrm -f dist/SaimoTV-Windows.zip\ncd dist && zip -r SaimoTV-Windows.zip SaimoTV && cd ..'
)

text = text.replace(
    'printf \'\\r\\n\\r\\n1252\\t_ForceCodepage\\r\\n\' > dist/_ForceCodepage.idt\nmsibuild dist/SaimoTV-Instalador.msi -i dist/_ForceCodepage.idt\nrm -f dist/_ForceCodepage.idt\nif [ -n "${SAIMO_ASSINAR:-}" ]; then\n  "$SAIMO_ASSINAR" "dist/SaimoTV-Instalador.msi"\nelse\n  echo \'AVISO: pacote de teste SEM assinatura de editor; não foi confirmado como falso positivo.\'\nfi\nshasum -a 256 \'dist/SaimoTV/Saimo TV.exe\' dist/SaimoTV/libmpv-2.dll dist/SaimoTV-Instalador.msi > dist/SHA256SUMS.txt',
    'if [ -n "${SAIMO_ASSINAR:-}" ]; then\n  echo \'AVISO: O zip em si não é assinado, os arquivos dentro dele foram.\'\nfi\nshasum -a 256 \'dist/SaimoTV/Saimo TV.exe\' dist/SaimoTV/libmpv-2.dll dist/SaimoTV-Windows.zip > dist/SHA256SUMS.txt'
)

text = text.replace(
    '[ -f dist/SaimoTV-Instalador.msi ] || { echo "o instalador não foi gerado"; exit 1; }\necho "pronto: dist/SaimoTV-Instalador.msi ($(du -h dist/SaimoTV-Instalador.msi | cut -f1))"',
    '[ -f dist/SaimoTV-Windows.zip ] || { echo "o zip não foi gerado"; exit 1; }\necho "pronto: dist/SaimoTV-Windows.zip ($(du -h dist/SaimoTV-Windows.zip | cut -f1))"'
)

with open("empacotar.sh", "w") as f:
    f.write(text)
