//! Chamadas HTTP curtas. Tudo que fala com a rede passa por aqui.

use std::io::Read;
use std::time::Duration;

/// Teto do que se lê de uma vez para a memória: os arquivos do acervo passam
/// de alguns megabytes, e o padrão do ureq (10 MB) cortava os maiores.
const LIMITE_TEXTO: u64 = 256 * 1024 * 1024;

fn agente() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(45)))
        // O corpo pode ser um feed de guia de dezenas de megabytes: o prazo é
        // para o todo, com folga, e não por pedaço como no ureq 2.
        .timeout_recv_body(Some(Duration::from_secs(300)))
        .user_agent(crate::AGENTE)
        .build()
        .into()
}

pub fn texto(url: &str) -> Option<String> {
    agente().get(url).call().ok()?.into_body().with_config().limit(LIMITE_TEXTO).read_to_string().ok()
}

pub fn bytes(url: &str, limite: usize) -> Option<Vec<u8>> {
    let resposta = agente().get(url).call().ok()?;
    let mut dados = Vec::new();
    resposta.into_body().into_reader().take(limite as u64).read_to_end(&mut dados).ok()?;
    Some(dados)
}

/// O corpo da resposta conforme ele chega, já descomprimido: os feeds do guia
/// têm dezenas de megabytes e não cabem bem na memória de uma vez.
pub fn fluxo(url: &str) -> Option<Box<dyn Read + Send + Sync>> {
    Some(Box::new(agente().get(url).call().ok()?.into_body().into_reader()))
}

pub fn json(url: &str) -> Option<serde_json::Value> {
    agente().get(url).call().ok()?.into_body().with_config().limit(LIMITE_TEXTO).read_json().ok()
}

/// Para onde um endereço redireciona, sem baixar o conteúdo.
pub fn destino(url: &str) -> Option<String> {
    let agente: ureq::Agent = ureq::Agent::config_builder()
        .max_redirects(0)
        // 302 é a resposta esperada aqui, não um erro.
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(15)))
        .user_agent(crate::AGENTE)
        .build()
        .into();
    let resposta = agente.get(url).call().ok()?;
    resposta.headers().get("location")?.to_str().ok().map(str::to_string)
}

/// Envia e lê a resposta. Só para o "olá": o resto é fogo e esquece.
pub fn postar_e_ler(url: String, corpo: String) -> Option<serde_json::Value> {
    agente()
        .post(&url)
        .header("content-type", "application/json")
        .send(&corpo)
        .ok()?
        .into_body()
        .read_json()
        .ok()
}

/// Envia e esquece: nada que o monitor receba pode segurar a tela.
pub fn postar(url: String, corpo: String) {
    std::thread::spawn(move || {
        let _ = agente().post(&url).header("content-type", "application/json").send(&corpo);
    });
}
