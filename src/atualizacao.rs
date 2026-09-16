//! Procura versão nova no GitHub e abre o download no navegador.
//!
//! Igual ao Mac: o app não baixa nem troca nada por dentro. Checa na abertura e
//! de hora em hora, e cai para a página /releases/latest quando a API recusa por
//! limite de consultas (60 por hora por IP, sem conta).

use std::sync::Mutex;

const REPO: &str = "gabrielsaimo/SaimoPlayer";
pub const ARQUIVO: &str = "SaimoTV-Windows.zip";

#[derive(Clone, Debug)]
pub struct Versao {
    pub numero: String,
    pub notas: String,
    pub link: String,
}

static ADIADA: Mutex<Option<String>> = Mutex::new(None);

/// "v1.2.3" e "1.2" viram "1.2.3" e "1.2"; qualquer outra coisa, nada.
fn numero_da_tag(tag: &str) -> Option<String> {
    let limpo = tag.trim_start_matches(['v', 'V']);
    let partes: Vec<&str> = limpo.split('.').collect();
    if !(2..=3).contains(&partes.len()) || !partes.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    Some(limpo.to_string())
}

/// Compara 1.10.0 com 1.9.3 por parte, e não como texto.
pub fn mais_nova(candidata: &str, atual: &str) -> bool {
    let numeros = |v: &str| -> Vec<u32> { v.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    let (a, b) = (numeros(candidata), numeros(atual));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

pub fn procurar() -> Option<Versao> {
    let achada = pela_api().or_else(pelo_site)?;
    if !mais_nova(&achada.numero, crate::VERSAO) {
        return None;
    }
    if ADIADA.lock().unwrap().as_deref() == Some(achada.numero.as_str()) {
        return None;
    }
    Some(achada)
}

pub fn adiar(versao: &Versao) {
    *ADIADA.lock().unwrap() = Some(versao.numero.clone());
}

fn pela_api() -> Option<Versao> {
    let dados = crate::rede::json(&format!("https://api.github.com/repos/{REPO}/releases/latest"))?;
    let numero = numero_da_tag(dados.get("tag_name")?.as_str()?)?;
    let link = dados
        .get("assets")?
        .as_array()?
        .iter()
        .find(|a| a.get("name").and_then(|n| n.as_str()) == Some(ARQUIVO))
        .and_then(|a| a.get("browser_download_url")?.as_str())
        .map(str::to_string)?;
    Some(Versao {
        numero,
        notas: dados.get("body").and_then(|b| b.as_str()).unwrap_or("").to_string(),
        link,
    })
}

/// Reserva sem limite de consultas: /releases/latest redireciona para a tag da
/// versão mais nova, e o arquivo mora num endereço fixo dela.
fn pelo_site() -> Option<Versao> {
    let tag = crate::rede::destino(&format!("https://github.com/{REPO}/releases/latest"))?;
    let tag = tag.rsplit('/').next()?.to_string();
    let numero = numero_da_tag(&tag)?;
    Some(Versao {
        numero,
        notas: String::new(),
        link: format!("https://github.com/{REPO}/releases/download/{tag}/{ARQUIVO}"),
    })
}

/// Abre no navegador padrão. `start` é o comando do próprio Windows.
pub fn abrir_no_navegador(link: &str) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd").args(["/C", "start", "", link]).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("open").arg(link).spawn();
    }
}
