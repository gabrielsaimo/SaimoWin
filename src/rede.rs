//! Chamadas HTTP curtas. Tudo que fala com a rede passa por aqui.

use std::io::Read;
use std::time::Duration;

fn agente() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(45))
        .user_agent(crate::AGENTE)
        .build()
}

pub fn texto(url: &str) -> Option<String> {
    agente().get(url).call().ok()?.into_string().ok()
}

pub fn bytes(url: &str, limite: usize) -> Option<Vec<u8>> {
    let resposta = agente().get(url).call().ok()?;
    let mut dados = Vec::new();
    resposta.into_reader().take(limite as u64).read_to_end(&mut dados).ok()?;
    Some(dados)
}

pub fn json(url: &str) -> Option<serde_json::Value> {
    agente().get(url).call().ok()?.into_json().ok()
}

/// Para onde um endereço redireciona, sem baixar o conteúdo.
pub fn destino(url: &str) -> Option<String> {
    let resposta = ureq::builder()
        .redirects(0)
        .timeout_connect(Duration::from_secs(15))
        .user_agent(crate::AGENTE)
        .build()
        .get(url)
        .call()
        .ok()?;
    resposta.header("location").map(str::to_string)
}

/// Envia e lê a resposta. Só para o "olá": o resto é fogo e esquece.
pub fn postar_e_ler(url: String, corpo: String) -> Option<serde_json::Value> {
    agente()
        .post(&url)
        .set("content-type", "application/json")
        .send_string(&corpo)
        .ok()?
        .into_json()
        .ok()
}

/// Envia e esquece: nada que o monitor receba pode segurar a tela.
pub fn postar(url: String, corpo: String) {
    std::thread::spawn(move || {
        let _ = agente().post(&url).set("content-type", "application/json").send_string(&corpo);
    });
}
