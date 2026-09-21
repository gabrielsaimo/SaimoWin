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
    /// "f|Nome" ou "s|Nome" -> o endereço do pôster, quando o TMDB conhece.
    capas: HashMap<String, String>,
    /// Todos os que aparecem no acervo, em ordem.
    pub todos: Vec<String>,
}

impl Generos {
    pub fn vazio(&self) -> bool {
        self.mapa.is_empty()
    }

    /// Os gêneros de um título.
    ///
    /// A chave é o nome como o acervo o escreve — e o acervo escreve o ano
    /// dentro do nome do filme, mas guarda o da série num campo à parte. Quem
    /// chama nem sempre sabe de qual dos dois veio, então procura-se o nome
    /// como ele chegou e, não achando, sem o ano.
    pub fn de(&self, titulo: &str, serie: bool) -> &[String] {
        let marca = if serie { "s" } else { "f" };
        if let Some(lista) = self.mapa.get(&format!("{marca}|{titulo}")) {
            return lista;
        }
        self.mapa
            .get(&format!("{marca}|{}", sem_ano(titulo)))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn tem(&self, titulo: &str, serie: bool, genero: &str) -> bool {
        genero.is_empty() || self.de(titulo, serie).iter().any(|g| g == genero)
    }

    /// O pôster de um título, pelo id que o gerador já resolveu.
    ///
    /// Antes a capa era procurada pelo nome no TMDB, a cada abertura: lento, e
    /// errado quando dois filmes se chamam igual. Quem não tem ficha fica sem
    /// capa, e a tela põe uma marca no lugar.
    pub fn capa(&self, titulo: &str, serie: bool) -> Option<&str> {
        let marca = if serie { "s" } else { "f" };
        self.capas
            .get(&format!("{marca}|{titulo}"))
            .or_else(|| self.capas.get(&format!("{marca}|{}", sem_ano(titulo))))
            .map(String::as_str)
    }
}

/// Busca a lista. Chamada de uma thread: fala com a rede.
pub fn baixar() -> Generos {
    let Some(texto) = crate::vod::arquivo_publico("fichas.txt") else {
        return Generos::default();
    };
    ler(&texto)
}

fn ler(texto: &str) -> Generos {
    let mut mapa = HashMap::new();
    let mut capas = HashMap::new();
    let mut vistos = BTreeSet::new();
    let mut base = String::new();
    // tipo \t título \t id do TMDB \t pôster \t gêneros
    for linha in texto.lines() {
        if let Some(resto) = linha.strip_prefix("capa:") {
            base = resto.trim().to_string();
            continue;
        }
        if linha.starts_with('#') {
            continue;
        }
        let campos: Vec<&str> = linha.split('\t').collect();
        if campos.len() < 5 {
            continue;
        }
        let chave = format!("{}|{}", campos[0], campos[1]);
        if !campos[3].trim().is_empty() {
            capas.insert(chave.clone(), format!("{base}{}", campos[3]));
        }
        let lista: Vec<String> = campos[4]
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
        mapa.insert(chave, lista);
    }
    Generos { mapa, capas, todos: vistos.into_iter().collect() }
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
