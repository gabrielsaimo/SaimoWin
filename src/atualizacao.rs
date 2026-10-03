//! Procura versão nova no GitHub e abre o download no navegador.
//!
//! Igual ao Mac: o app não baixa nem troca nada por dentro. Checa na abertura e
//! de hora em hora, e cai para a página /releases/latest quando a API recusa por
//! limite de consultas (60 por hora por IP, sem conta).

use std::sync::Mutex;

const REPO: &str = "gabrielsaimo/SaimoPlayer";
/// O que procurar no release. Era um ZIP com os arquivos soltos até a 1.7.5,
/// e passou a ser o instalador: quem baixava o ZIP e clicava no executável de
/// dentro do compactador ficava sem vídeo, porque a DLL não ia junto.
pub const ARQUIVO: &str = "SaimoTV-Instalador.msi";

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

/// Só abre downloads HTTPS do repositório oficial, sem interpretador de comandos.
fn link_oficial(link: &str) -> bool {
    link.starts_with("https://github.com/gabrielsaimo/SaimoPlayer/releases/")
        && !link.chars().any(|c| c.is_control() || c.is_whitespace() || c == '\\')
}

pub fn abrir_no_navegador(link: &str) {
    if !link_oficial(link) { return; }
    #[cfg(windows)]
    {
        use std::ffi::c_void;
        #[link(name = "shell32")]
        extern "system" {
            fn ShellExecuteW(hwnd: *mut c_void, operation: *const u16,
                file: *const u16, parameters: *const u16, directory: *const u16,
                show: i32) -> *mut c_void;
        }
        let operation: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
        let url: Vec<u16> = link.encode_utf16().chain(Some(0)).collect();
        let result = unsafe { ShellExecuteW(std::ptr::null_mut(), operation.as_ptr(),
            url.as_ptr(), std::ptr::null(), std::ptr::null(), 1) };
        if result as isize <= 32 {
            eprintln!("Não foi possível abrir o navegador (Windows: {}).", result as isize);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("open").arg(link).spawn();
    }
}

#[cfg(test)]
mod testes_links {
    use super::link_oficial;
    #[test]
    fn restringe_downloads_ao_repositorio_oficial() {
        assert!(link_oficial("https://github.com/gabrielsaimo/SaimoPlayer/releases/download/v1.7.7/SaimoTV-Instalador.msi"));
        for url in ["file:///C:/bad.exe", "https://github.com.evil.test/gabrielsaimo/SaimoPlayer/releases/",
                    "https://github.com/gabrielsaimo/SaimoPlayer/releases/\ncalc.exe",
                    "https://github.com/other/repo/releases/latest"] {
            assert!(!link_oficial(url));
        }
    }
}
