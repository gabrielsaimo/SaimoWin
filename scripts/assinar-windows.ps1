param(
    [Parameter(Mandatory=$true)][string]$Arquivo,
    [Parameter(Mandatory=$true)][string]$Thumbprint
)
$ErrorActionPreference = 'Stop'
$certificate = Get-Item "Cert:\CurrentUser\My\$Thumbprint"
if (-not $certificate.HasPrivateKey -or $certificate.NotAfter -lt (Get-Date)) {
    throw 'Certificado inválido, expirado ou sem acesso à chave privada.'
}
if (-not ($certificate.EnhancedKeyUsageList | Where-Object { $_.ObjectId -eq '1.3.6.1.5.5.7.3.3' })) {
    throw 'É necessário um certificado específico de assinatura de código.'
}
$signature = Set-AuthenticodeSignature -FilePath $Arquivo -Certificate $certificate -HashAlgorithm SHA256 -TimestampServer 'http://timestamp.digicert.com'
if ($signature.Status -ne 'Valid') { throw "Assinatura não validada: $($signature.StatusMessage)" }
$verification = Get-AuthenticodeSignature -FilePath $Arquivo
if ($verification.Status -ne 'Valid') { throw 'Verificação da assinatura falhou.' }
Write-Output "Assinatura verificada: $Arquivo"
