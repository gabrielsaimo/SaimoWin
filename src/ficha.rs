//! A ficha de um título: sinopse, duração, classificação, gêneros e elenco.
//!
//! O catálogo publicado traz nome e endereço; o arquivo de fichas acrescenta
//! o id do TMDB, o pôster e os gêneros. É o bastante para desenhar a grade, e
//! pouco demais para quem parou num cartaz e quer saber do que se trata antes
//! de gastar dois minutos abrindo o filme.
//!
//! O resto só existe no endereço do próprio título no TMDB, e é um pedido por
//! título aberto — não por título listado. Com o id vindo da ficha não há
//! busca nem desempate: pergunta-se direto pelo número certo.
//!
//! Os mesmos campos, com os mesmos nomes, são o que o celular, o Mac, a TV Box
//! e o site mostram.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

const CHAVE: &str = "15d2ea6d0dc1d476efbca3eba2b9bbfb";
const BASE: &str = "https://api.themoviedb.org/3";
const IMAGENS: &str = "https://image.tmdb.org/t/p/";

#[derive(Clone, Debug, Default)]
pub struct Pessoa {
    pub id: u32,
    pub nome: String,
    pub papel: String,
    pub foto: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Ficha {
    pub titulo: String,
    pub sinopse: String,
    pub frase: String,
    /// Minutos: do filme, ou de um episódio da série.
    pub duracao: Option<u32>,
    pub classificacao: Option<String>,
    pub nota: f64,
    pub ano: String,
    pub generos: Vec<String>,
    /// Quem dirigiu o filme, ou quem criou a série.
    pub assinatura: String,
    pub roteiro: String,
    pub produtora: String,
    pub elenco: Vec<Pessoa>,
    pub capa: Option<String>,
}

impl Ficha {
    pub fn vazia(&self) -> bool {
        self.sinopse.is_empty()
            && self.elenco.is_empty()
            && self.generos.is_empty()
            && self.duracao.is_none()
    }
}

fn guardadas() -> &'static Mutex<HashMap<String, Ficha>> {
    static MAPA: OnceLock<Mutex<HashMap<String, Ficha>>> = OnceLock::new();
    MAPA.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A ficha já guardada, se alguma thread já a trouxe.
pub fn conhecida(titulo: &str, serie: bool) -> Option<Ficha> {
    guardadas().lock().ok()?.get(&chave(titulo, serie)).cloned()
}

fn chave(titulo: &str, serie: bool) -> String {
    format!("{}{titulo}", if serie { "s:" } else { "f:" })
}

/// Busca a ficha. Chamada de uma thread: fala com a rede.
pub fn baixar(titulo: &str, serie: bool, id: u32) -> Option<Ficha> {
    if let Some(pronta) = conhecida(titulo, serie) {
        return Some(pronta);
    }
    let tipo = if serie { "tv" } else { "movie" };
    let extras = if serie { "credits,content_ratings" } else { "credits,release_dates" };
    let json = crate::rede::json(&format!(
        "{BASE}/{tipo}/{id}?api_key={CHAVE}&language=pt-BR&append_to_response={extras}"
    ))?;

    let texto = |campo: &str| json[campo].as_str().unwrap_or("").to_string();
    let data = texto(if serie { "first_air_date" } else { "release_date" });

    let creditos = &json["credits"];
    let mut direcao: Vec<String> = Vec::new();
    let mut roteiro: Vec<String> = Vec::new();
    if let Some(equipe) = creditos["crew"].as_array() {
        for pessoa in equipe {
            let nome = pessoa["name"].as_str().unwrap_or("").to_string();
            match pessoa["job"].as_str().unwrap_or("") {
                "Director" => direcao.push(nome),
                "Screenplay" | "Writer" | "Story" => roteiro.push(nome),
                _ => {}
            }
        }
    }
    // Série não tem diretor único: quem a assina é quem a criou.
    let criadores: Vec<String> = json["created_by"]
        .as_array()
        .map(|lista| {
            lista.iter().filter_map(|p| p["name"].as_str().map(str::to_string)).collect()
        })
        .unwrap_or_default();
    let assinatura = if direcao.is_empty() { criadores } else { direcao };
    roteiro.dedup();

    let elenco = creditos["cast"]
        .as_array()
        .map(|lista| {
            lista
                .iter()
                .take(20)
                .map(|p| Pessoa {
                    id: p["id"].as_u64().unwrap_or(0) as u32,
                    nome: p["name"].as_str().unwrap_or("").to_string(),
                    papel: p["character"].as_str().unwrap_or("").to_string(),
                    foto: p["profile_path"]
                        .as_str()
                        .map(|caminho| format!("{IMAGENS}w185{caminho}")),
                })
                .collect()
        })
        .unwrap_or_default();

    let duracao = if serie {
        json["episode_run_time"].as_array().and_then(|l| l.first()).and_then(|v| v.as_u64())
    } else {
        json["runtime"].as_u64()
    }
    .filter(|m| *m > 0)
    .map(|m| m as u32);

    let ficha = Ficha {
        titulo: texto(if serie { "name" } else { "title" }),
        sinopse: texto("overview"),
        frase: texto("tagline"),
        duracao,
        classificacao: classificacao_br(&json, serie),
        nota: json["vote_average"].as_f64().unwrap_or(0.0),
        ano: data.chars().take(4).collect(),
        generos: json["genres"]
            .as_array()
            .map(|lista| {
                lista.iter().filter_map(|g| g["name"].as_str().map(str::to_string)).collect()
            })
            .unwrap_or_default(),
        assinatura: assinatura.into_iter().take(2).collect::<Vec<_>>().join(", "),
        roteiro: roteiro.into_iter().take(2).collect::<Vec<_>>().join(", "),
        produtora: json["production_companies"][0]["name"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        elenco,
        capa: json["poster_path"].as_str().map(|c| format!("{IMAGENS}w342{c}")),
    };

    if let Ok(mut mapa) = guardadas().lock() {
        mapa.insert(chave(titulo, serie), ficha.clone());
    }
    Some(ficha)
}

/// Os trabalhos de um ator, como o TMDB os devolve: id e se é série.
///
/// Só a ida à rede mora aqui. O cruzamento com o acervo é local e acontece na
/// thread do desenho, onde estão o índice e as fichas — mandar a rede junto
/// congelaria a janela pelo tempo do pedido.
pub fn creditos_de(ator: u32) -> Vec<(u32, bool)> {
    let Some(json) = crate::rede::json(&format!(
        "{BASE}/person/{ator}/combined_credits?api_key={CHAVE}&language=pt-BR"
    )) else {
        return Vec::new();
    };

    let mut trabalhos: Vec<&serde_json::Value> = Vec::new();
    for campo in ["cast", "crew"] {
        if let Some(lista) = json[campo].as_array() {
            trabalhos.extend(lista.iter());
        }
    }
    // Ordem de popularidade: o que a pessoa é mais conhecida por fazer vem
    // primeiro, e não a ordem em que o TMDB devolveu.
    trabalhos.sort_by(|a, b| {
        b["popularity"]
            .as_f64()
            .unwrap_or(0.0)
            .partial_cmp(&a["popularity"].as_f64().unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut saida = Vec::new();
    let mut vistos: HashSet<(u32, bool)> = HashSet::new();
    for trabalho in trabalhos {
        let Some(id) = trabalho["id"].as_u64() else { continue };
        let serie = trabalho["media_type"].as_str() == Some("tv");
        if vistos.insert((id as u32, serie)) {
            saida.push((id as u32, serie));
        }
    }
    saida
}

/// O que um ator fez **e que existe neste acervo**.
///
/// A filmografia inteira do TMDB não serve de dentro do aplicativo: listar
/// oitenta títulos dos quais setenta não abrem é uma lista que frustra. O
/// cruzamento é pelo id do TMDB, que o arquivo de fichas já traz para cada
/// título daqui — nome igual não engana, refilmagem não vira o original.
pub fn no_acervo(
    creditos: &[(u32, bool)],
    generos: &crate::generos::Generos,
    acervo: &[crate::vod::Achado],
) -> Vec<crate::vod::Achado> {
    let mut por_nome: HashMap<String, &crate::vod::Achado> = HashMap::new();
    for achado in acervo {
        por_nome.insert(chave(&achado.titulo, achado.serie), achado);
    }

    let mut vistos: HashSet<String> = HashSet::new();
    let mut saida = Vec::new();
    for (id, serie) in creditos {
        let Some(titulo) = generos.titulo_de(*id, *serie) else { continue };
        let achado = por_nome
            .get(&chave(titulo, *serie))
            .or_else(|| por_nome.get(&chave(&crate::generos::sem_ano(titulo), *serie)));
        let Some(achado) = achado else { continue };
        if !vistos.insert(chave(&achado.titulo, achado.serie)) {
            continue;
        }
        saida.push((*achado).clone());
    }
    saida
}

fn classificacao_br(json: &serde_json::Value, serie: bool) -> Option<String> {
    let listas = if serie {
        json["content_ratings"]["results"].as_array()?
    } else {
        json["release_dates"]["results"].as_array()?
    };
    let br = listas.iter().find(|item| item["iso_3166_1"].as_str() == Some("BR"))?;
    if serie {
        return br["rating"].as_str().filter(|n| !n.is_empty()).map(str::to_string);
    }
    br["release_dates"]
        .as_array()?
        .iter()
        .filter_map(|d| d["certification"].as_str())
        .find(|n| !n.is_empty())
        .map(str::to_string)
}
