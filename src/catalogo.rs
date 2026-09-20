//! A mesma lista dos outros aplicativos, baixada do repositório publicado.
//!
//! Formato de `catalogo.txt`: uma chave por linha, `canal:` abre um canal,
//! `fonte:` acrescenta uma fonte, e referer/agente/chave pertencem à fonte
//! imediatamente acima. `restritos.txt` tem os canais que só aparecem depois do
//! código e vem no mesmo formato.

use std::path::PathBuf;

pub const BASE: &str = "https://raw.githubusercontent.com/gabrielsaimo/SaimoPlayer/main/";

/// Ordem das seções, igual à do Mac, do TV Box e do site.
pub const ORDEM: [&str; 10] = [
    "TV Aberta", "Filmes e Séries", "Esportes", "Notícias", "Infantil",
    "Documentários", "Pluto TV", "24 Horas", "Variedades", "Adulto",
];

#[derive(Clone, Debug)]
pub struct Fonte {
    pub url: String,
    pub qualidade: Option<String>,
    pub referer: Option<String>,
    pub agente: Option<String>,
    /// Par KID:chave do ClearKey. O mpv não monta licença: a fonte é descartada.
    pub chave: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Canal {
    pub nome: String,
    pub logo: Option<String>,
    pub categoria: Option<String>,
    pub fontes: Vec<Fonte>,
}

impl Canal {
    pub fn secao(&self) -> &str {
        match self.categoria.as_deref() {
            Some(c) if !c.is_empty() => c,
            _ => secao_pelo_nome(&self.nome),
        }
    }
}

/// Sem `categoria:` declarada, a seção sai do nome — mesma regra dos outros.
fn secao_pelo_nome(nome: &str) -> &'static str {
    let n = nome.to_lowercase();
    let tem = |lista: &[&str]| lista.iter().any(|p| n.contains(p));
    if tem(&["sexy hot", "playboy", "adulto", "venus", "hustler", "private", "sex"]) {
        "Adulto"
    } else if tem(&["pluto"]) {
        "Pluto TV"
    } else if tem(&["espn", "sportv", "premiere", "combate", "band sports", "cazé", "caze", "nsports", "xsports"]) {
        "Esportes"
    } else if tem(&["news", "globonews", "cnn", "record news", "jovem pan", "terra viva"]) {
        "Notícias"
    } else if tem(&["cartoon", "nick", "discovery kids", "gloob", "infantil", "kids", "boomerang"]) {
        "Infantil"
    } else if tem(&["discovery", "history", "animal planet", "natgeo", "national geographic", "investigação"]) {
        "Documentários"
    } else if tem(&["hbo", "telecine", "megapix", "paramount", "tnt", "space", "universal", "amc", "cinemax", "star"]) {
        "Filmes e Séries"
    } else if tem(&["globo", "sbt", "record", "band", "rede tv", "redetv", "cultura", "gazeta"]) {
        "TV Aberta"
    } else {
        "Variedades"
    }
}

pub fn parse(texto: &str) -> Vec<Canal> {
    let mut canais: Vec<Canal> = Vec::new();
    for linha in texto.lines() {
        let linha = linha.trim();
        if linha.is_empty() || linha.starts_with('#') {
            continue;
        }
        let Some((campo, valor)) = linha.split_once(':') else { continue };
        let campo = campo.trim().to_lowercase();
        let valor = valor.trim();
        if valor.is_empty() {
            continue;
        }
        match campo.as_str() {
            "canal" => canais.push(Canal {
                nome: valor.to_string(),
                logo: None,
                categoria: None,
                fontes: Vec::new(),
            }),
            "logo" => {
                if let Some(c) = canais.last_mut() {
                    c.logo = Some(valor.to_string());
                }
            }
            "categoria" => {
                if let Some(c) = canais.last_mut() {
                    c.categoria = Some(valor.to_string());
                }
            }
            "fonte" => {
                if let Some(c) = canais.last_mut() {
                    c.fontes.push(Fonte {
                        url: valor.to_string(),
                        qualidade: None,
                        referer: None,
                        agente: None,
                        chave: None,
                    });
                }
            }
            "referer" | "agente" | "chave" | "qualidade" => {
                if let Some(f) = canais.last_mut().and_then(|c| c.fontes.last_mut()) {
                    match campo.as_str() {
                        "referer" => f.referer = Some(valor.to_string()),
                        "agente" => f.agente = Some(valor.to_string()),
                        "qualidade" => f.qualidade = Some(valor.to_string()),
                        _ => f.chave = Some(valor.to_string()),
                    }
                }
            }
            _ => {}
        }
    }
    for canal in &mut canais {
        // ClearKey é DASH com licença, que o mpv não monta: a fonte não tocaria
        // e só atrasaria a troca para a próxima.
        canal.fontes.retain(|f| f.chave.is_none());
    }
    canais.retain(|c| !c.fontes.is_empty());
    canais
}

/// Ordena por seção e nome, como nos outros aplicativos, com os favoritos na
/// frente para não precisar procurar.
pub fn ordenar(canais: &mut Vec<Canal>, favoritos: &[String]) {
    canais.sort_by(|a, b| {
        let fav = |c: &Canal| !favoritos.iter().any(|f| f == &c.nome);
        let pos = |c: &Canal| ORDEM.iter().position(|o| *o == c.secao()).unwrap_or(ORDEM.len());
        fav(a)
            .cmp(&fav(b))
            .then(pos(a).cmp(&pos(b)))
            .then_with(|| chave_de_ordem(&a.nome).cmp(&chave_de_ordem(&b.nome)))
    });
}

/// "Ó" e "o" lado a lado: sem tirar o acento, a lista fica fora de ordem.
pub fn chave_de_ordem(texto: &str) -> String {
    texto
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'ê' | 'è' => 'e',
            'í' | 'ì' | 'î' => 'i',
            'ó' | 'ô' | 'õ' | 'ò' => 'o',
            'ú' | 'ù' | 'û' => 'u',
            'ç' => 'c',
            outro => outro,
        })
        .collect()
}

pub fn pasta() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    let pasta = base.join("SaimoTV");
    let _ = std::fs::create_dir_all(&pasta);
    pasta
}

/// Lê do disco o que foi baixado da última vez: a lista aparece na hora, mesmo
/// sem rede, e a rede só a substitui quando chega.
pub fn em_cache(arquivo: &str) -> Vec<Canal> {
    std::fs::read_to_string(pasta().join(arquivo))
        .map(|t| parse(&t))
        .unwrap_or_default()
}

pub fn baixar(arquivo: &str) -> Option<Vec<Canal>> {
    let url = format!("{BASE}{arquivo}");
    let texto = crate::rede::texto(&url)?;
    let canais = parse(&texto);
    if canais.is_empty() {
        return None;
    }
    let _ = std::fs::write(pasta().join(arquivo), &texto);
    Some(canais)
}
