# Assinatura e integridade

O alerta ESET ainda precisa de análise do fornecedor. Uma recompilação ou mudança de hash não confirma falso positivo.

## Biblioteca de vídeo

`mpv.lock.json` fixa a versão, URL, hash do arquivo oficial publicado no GitHub e hash da DLL extraída. `empacotar.sh` verifica a DLL em toda montagem e para se houver divergência. `--baixar` baixa somente essa versão e verifica ambos os hashes antes de substituir a DLL; uma cópia anterior fica em `auditoria/`.

## Assinatura oficial pendente

Não há certificado de assinatura de código configurado. Um certificado autoassinado ou de assinatura de documentos não substitui um certificado de editor confiável.

Depois de obter o certificado ou serviço de assinatura, configure `SAIMO_ASSINAR` como caminho de um executável que recebe um arquivo, assina e verifica sua assinatura, retornando erro se qualquer passo falhar. A montagem chama esse executável primeiro para o EXE, antes de incluí-lo no MSI, e depois para o MSI final. Use `./empacotar.sh --assinado` para exigir essa configuração.

Em Windows, `scripts/assinar-windows.ps1 -Arquivo <arquivo> -Thumbprint <identificador>` oferece a operação para certificados de assinatura de código acessíveis pelo repositório pessoal do Windows. Para token físico ou assinatura em nuvem, siga a integração do fornecedor. Uma ponte remota deve devolver o arquivo assinado ao mesmo caminho antes de retornar; não basta assinar só uma cópia remota.

Não altere o EXE depois de assinado e não altere o MSI depois de assinado. O script de assinatura é uma ferramenta de desenvolvimento: não é distribuído nem executado pelo instalador.

`dist/SHA256SUMS.txt` registra os hashes da montagem. Assinatura identifica o editor, mas não garante a retirada do alerta de antivírus.

## ESET

O arquivo original suspeito foi preservado separadamente em `auditoria/eset-original/`. Envie esse arquivo, não apenas a recompilação. Informe a detecção, a versão, o hash, a origem e que se trata de uma suspeita de falso positivo ainda não confirmada. O ZIP de amostra usa a senha `infected`, conforme orientação da ESET.
