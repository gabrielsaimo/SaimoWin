//! As fileiras da tela inicial do acervo, prontas para desenhar.
//!
//! O acervo tem trinta e quatro mil filmes e nenhuma data de entrada, então não
//! há como o programa descobrir sozinho o que é novidade — e perguntar a capa de
//! cada título ao TMDB, a cada abertura, seria uma tela que demora para
//! aparecer. A conta é feita no repositório (`gerar_destaques.py`) e chega aqui
//! pronta: seis fileiras, cento e vinte títulos, sete quilobytes, com o caminho
//! do pôster junto.
//!
//! O formato de cada item é o mesmo de um resultado de busca — tipo, título,
//! letra, ano —, então abrir um destaque passa pelo caminho que já abre um
//! título procurado. É o mesmo arquivo que a TV Box e o Mac leem.

use std::time::{Duration, SystemTime};

const ENDERECO: &str =
    "https://raw.githubusercontent.com/gabrielsaimo/SaimoPlayer/main/vod/destaques.txt";
/// A lista muda quando o gerador roda; um dia em disco abre instantâneo sem
/// ficar semanas desatualizado.
const VALIDADE: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Debug)]
pub struct Item {
    pub titulo: String,
    /// 'f' filme, 's' série, 'a' anime, 'd' dorama.
    pub tipo: char,
    pub letra: String,
    pub ano: String,
    /// Endereço inteiro da capa, ou vazio quando o gerador não achou uma.
    pub capa: String,
}

impl Item {
    pub fn serie(&self) -> bool {
        self.tipo != 'f'
    }

    pub fn da_colecao(&self) -> bool {
        self.tipo == 'a' || self.tipo == 'd'
    }

    pub fn colecao(&self) -> &'static str {
        if self.tipo == 'a' { "animes" } else { "doramas" }
    }
}

#[derive(Clone, Debug)]
pub struct Fila {
    pub titulo: String,
    pub itens: Vec<Item>,
}

fn arquivo() -> std::path::PathBuf {
    crate::catalogo::pasta().join("destaques.txt")
}

/// Busca as fileiras. Chamada de uma thread: fala com a rede.
pub fn filas() -> Vec<Fila> {
    let Some(texto) = texto() else { return Vec::new() };
    ler(&texto)
}

fn texto() -> Option<String> {
    let local = arquivo();
    let fresco = std::fs::metadata(&local)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|quando| SystemTime::now().duration_since(quando).ok())
        .map(|idade| idade < VALIDADE)
        .unwrap_or(false);
    if fresco {
        if let Ok(guardado) = std::fs::read_to_string(&local) {
            if !guardado.trim().is_empty() {
                return Some(guardado);
            }
        }
    }

    if let Some(baixado) = crate::rede::texto(ENDERECO) {
        if !baixado.trim().is_empty() {
            let _ = std::fs::write(&local, &baixado);
            return Some(baixado);
        }
    }
    // Rede fora: o que está em disco, mesmo vencido, é melhor que nada.
    std::fs::read_to_string(&local).ok()
}

fn ler(texto: &str) -> Vec<Fila> {
    let mut filas: Vec<Fila> = Vec::new();
    let mut titulo: Option<String> = None;
    let mut itens: Vec<Item> = Vec::new();
    let mut base = String::new();

    for linha in texto.lines() {
        if linha.trim().is_empty() || linha.starts_with('#') {
            continue;
        }
        if let Some(resto) = linha.strip_prefix("capa:") {
            base = resto.trim().to_string();
            continue;
        }
        if let Some(nome) = linha.strip_prefix("fila\t") {
            if let Some(anterior) = titulo.take() {
                if !itens.is_empty() {
                    filas.push(Fila { titulo: anterior, itens: std::mem::take(&mut itens) });
                }
            }
            itens.clear();
            titulo = Some(nome.trim().to_string());
            continue;
        }
        let campos: Vec<&str> = linha.split('\t').collect();
        if campos.len() < 3 {
            continue;
        }
        let poster = campos.get(4).copied().unwrap_or("");
        itens.push(Item {
            titulo: campos[1].to_string(),
            tipo: campos[0].chars().next().unwrap_or('f'),
            letra: campos[2].to_string(),
            ano: campos.get(3).copied().unwrap_or("").to_string(),
            capa: if poster.is_empty() { String::new() } else { format!("{base}{poster}") },
        });
    }
    if let Some(nome) = titulo {
        if !itens.is_empty() {
            filas.push(Fila { titulo: nome, itens });
        }
    }
    filas
}
