//! O gênero de cada título: Ação, Terror, Animação, Comédia.
//!
//! O catálogo não tem gênero — as listas de origem trazem nome e endereço,
//! nada mais. Perguntar ao TMDB por trinta e quatro mil títulos, no aparelho,
//! a cada abertura, é uma tela que nunca abre.
//!
//! A pergunta é feita uma vez no repositório (`gerar_generos.py`) e chega aqui
//! pronta, no mesmo arquivo que o Mac, a TV Box e o site leem.
//!
//! Sem o arquivo, a régua de gêneros não aparece: oferecer um filtro que
//! devolve vazio é pior que não oferecer.

use std::collections::{BTreeSet, HashMap};

/// O que o gerador publicou, já lido.
#[derive(Default)]
pub struct Generos {
    /// "f|Nome" ou "s|Nome" -> os gêneros dele.
    mapa: HashMap<String, Vec<String>>,
    /// Todos os que aparecem no acervo, em ordem.
    pub todos: Vec<String>,
}

impl Generos {
    pub fn vazio(&self) -> bool {
        self.mapa.is_empty()
    }

    pub fn de(&self, titulo: &str, serie: bool) -> &[String] {
        let chave = format!("{}|{titulo}", if serie { "s" } else { "f" });
        self.mapa.get(&chave).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn tem(&self, titulo: &str, serie: bool, genero: &str) -> bool {
        genero.is_empty() || self.de(titulo, serie).iter().any(|g| g == genero)
    }
}

/// Busca a lista. Chamada de uma thread: fala com a rede.
pub fn baixar() -> Generos {
    let Some(texto) = crate::vod::arquivo_publico("generos.txt") else {
        return Generos::default();
    };
    ler(&texto)
}

fn ler(texto: &str) -> Generos {
    let mut mapa = HashMap::new();
    let mut vistos = BTreeSet::new();
    for linha in texto.lines() {
        if linha.starts_with('#') {
            continue;
        }
        let campos: Vec<&str> = linha.split('\t').collect();
        if campos.len() < 3 || campos[2].trim().is_empty() {
            continue;
        }
        let lista: Vec<String> = campos[2]
            .split(',')
            .map(str::trim)
            .filter(|g| !g.is_empty())
            .map(str::to_string)
            .collect();
        if lista.is_empty() {
            continue;
        }
        for g in &lista {
            vistos.insert(g.clone());
        }
        mapa.insert(format!("{}|{}", campos[0], campos[1]), lista);
    }
    Generos { mapa, todos: vistos.into_iter().collect() }
}

/// O título sem o ano final, que é como a lista de gêneros o guarda.
pub fn sem_ano(titulo: &str) -> String {
    let cortado = titulo.trim_end();
    if cortado.ends_with(')') && cortado.len() > 6 {
        let inicio = cortado.len() - 6;
        let cauda = &cortado[inicio..];
        if cauda.starts_with('(') && cauda[1..5].chars().all(|c| c.is_ascii_digit()) {
            return cortado[..inicio].trim_end().to_string();
        }
    }
    cortado.to_string()
}
