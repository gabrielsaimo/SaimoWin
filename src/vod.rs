//! Filmes e séries: o mesmo acervo publicado que o Mac e o celular leem.
//!
//! O catálogo de origem tem 30 MB, então ele vem fatiado por letra (e as séries
//! em pedaços dentro da letra): nenhum download passa de uns 100 KB, e a tela
//! também navega por letra. Cada item guarda "base:resto" em vez do endereço
//! inteiro — as bases vêm no índice.

use std::sync::Mutex;

const BASE: &str = "https://raw.githubusercontent.com/gabrielsaimo/SaimoPlayer/main/vod/";

#[derive(Clone, Debug, Default)]
pub struct Gaveta {
    pub letra: String,
    pub filmes: usize,
    pub series: usize,
}

#[derive(Clone, Debug)]
pub struct Filme {
    pub titulo: String,
    /// Só aparece depois do código (seção Extras).
    pub reservado: bool,
    /// "dub", "leg"… na ordem em que o catálogo publica.
    pub versoes: Vec<(String, Vec<String>)>,
}

#[derive(Clone, Debug)]
pub struct Serie {
    pub titulo: String,
    pub ano: String,
    pedaco: usize,
    pub episodios: usize,
}

#[derive(Clone, Debug)]
pub struct Episodio {
    pub temporada: u32,
    pub numero: u32,
    pub versao: String,
    pub urls: Vec<String>,
}

static BASES: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn pasta() -> std::path::PathBuf {
    let pasta = crate::catalogo::pasta().join("vod");
    let _ = std::fs::create_dir_all(&pasta);
    pasta
}

/// O índice sempre da rede quando ela responde: cada filme guarda o número da
/// base, então um índice velho com as bases noutra ordem manda todo mundo para
/// o servidor errado. São menos de um kilobyte.
pub fn indice() -> Vec<Gaveta> {
    let texto = crate::rede::texto(&format!("{BASE}indice.txt")).or_else(|| {
        std::fs::read_to_string(pasta().join("indice.txt")).ok()
    });
    let Some(texto) = texto else { return Vec::new() };

    let mut bases = Vec::new();
    let mut gavetas = Vec::new();
    for linha in texto.lines() {
        if let Some(resto) = linha.strip_prefix("base:") {
            if let Some((_numero, endereco)) = resto.trim().split_once(' ') {
                bases.push(endereco.trim().to_string());
            }
            continue;
        }
        let campos: Vec<&str> = linha.split('\t').collect();
        if campos.len() >= 3 && !campos[0].is_empty() {
            gavetas.push(Gaveta {
                letra: campos[0].to_string(),
                filmes: campos[1].parse().unwrap_or(0),
                series: campos[2].parse().unwrap_or(0),
            });
        }
    }
    if !bases.is_empty() {
        // Bases novas invalidam as fatias guardadas: elas apontam por número.
        let marca = pasta().join("bases.txt");
        let atual = bases.join("\n");
        if std::fs::read_to_string(&marca).map(|a| a != atual).unwrap_or(false) {
            limpar_fatias();
        }
        let _ = std::fs::write(&marca, &atual);
        let _ = std::fs::write(pasta().join("indice.txt"), &texto);
        *BASES.lock().unwrap() = bases;
    }
    gavetas
}

fn limpar_fatias() {
    let Ok(arquivos) = std::fs::read_dir(pasta()) else { return };
    for arquivo in arquivos.flatten() {
        let nome = arquivo.file_name();
        let nome = nome.to_string_lossy();
        if nome != "bases.txt" && nome != "indice.txt" {
            let _ = std::fs::remove_file(arquivo.path());
        }
    }
}

/// "0:19927" vira o endereço inteiro. Sem a base conhecida, a fonte é
/// descartada: "0:19927" sozinho o player aceitaria como URL e só falharia na
/// hora de tocar.
fn montar(valor: &str) -> Option<String> {
    if valor.starts_with("http") {
        return Some(valor.to_string());
    }
    let (numero, resto) = valor.split_once(':')?;
    let bases = BASES.lock().unwrap();
    let base = bases.get(numero.trim().parse::<usize>().ok()?)?;
    Some(if resto.contains('.') { format!("{base}{resto}") } else { format!("{base}{resto}.mp4") })
}

fn gaveta_do_arquivo(letra: &str) -> String {
    if letra == "#" { "%23".to_string() } else { letra.to_string() }
}

/// O arquivo, do disco quando já foi baixado: o acervo muda de vez em quando e
/// nunca no meio de uma navegação.
/// O mesmo arquivo publicado que o resto do catálogo usa, para quem está fora
/// deste módulo — a lista de gêneros, por exemplo.
pub fn arquivo_publico(nome: &str) -> Option<String> {
    arquivo(nome)
}

fn arquivo(nome: &str) -> Option<String> {
    let local = pasta().join(nome.replace("%23", "hash"));
    if let Ok(texto) = std::fs::read_to_string(&local) {
        if !texto.is_empty() {
            return Some(texto);
        }
    }
    let texto = crate::rede::texto(&format!("{BASE}{nome}"))?;
    if let Some(pai) = local.parent() {
        let _ = std::fs::create_dir_all(pai);
    }
    let _ = std::fs::write(&local, &texto);
    Some(texto)
}

pub fn filmes(letra: &str) -> Vec<Filme> {
    ler_filmes(letra, "filmes")
}

/// Os títulos que só aparecem depois do código, no mesmo formato dos filmes.
pub fn reservados(letra: &str) -> Vec<Filme> {
    ler_filmes(letra, "reservado")
}

fn ler_filmes(letra: &str, prefixo: &str) -> Vec<Filme> {
    let Some(texto) = arquivo(&format!("{prefixo}-{}.txt", gaveta_do_arquivo(letra))) else {
        return Vec::new();
    };
    texto
        .lines()
        .filter_map(|linha| {
            let campos: Vec<&str> = linha.split('\t').collect();
            if campos.len() < 2 || campos[0].is_empty() {
                return None;
            }
            let versoes: Vec<(String, Vec<String>)> = campos[1..]
                .iter()
                .filter_map(|parte| {
                    let (versao, lista) = parte.split_once('=')?;
                    let urls: Vec<String> = lista.split(',').filter_map(montar).collect();
                    if urls.is_empty() {
                        None
                    } else {
                        Some((versao.to_string(), urls))
                    }
                })
                .collect();
            if versoes.is_empty() {
                return None;
            }
            Some(Filme { titulo: campos[0].to_string(), reservado: prefixo == "reservado", versoes })
        })
        .collect()
}

pub fn series(letra: &str) -> Vec<Serie> {
    let Some(texto) = arquivo(&format!("series-{}.txt", gaveta_do_arquivo(letra))) else {
        return Vec::new();
    };
    texto
        .lines()
        .filter_map(|linha| {
            let campos: Vec<&str> = linha.split('\t').collect();
            if campos.len() < 4 || campos[0].is_empty() {
                return None;
            }
            Some(Serie {
                titulo: campos[0].to_string(),
                ano: campos[1].to_string(),
                pedaco: campos[2].parse().unwrap_or(0),
                episodios: campos[3].parse().unwrap_or(0),
            })
        })
        .collect()
}

/// Animes e doramas já vêm com todos os episódios no mesmo arquivo.
pub fn colecao(tipo: &str) -> Vec<(Serie, Vec<Episodio>)> {
    if tipo != "animes" && tipo != "doramas" {
        return Vec::new();
    }
    let nome = format!("redeflix/links-{tipo}.txt");
    let local = pasta().join(&nome);
    let texto = crate::rede::texto(&format!("{BASE}{nome}")).map(|texto| {
        if let Some(pai) = local.parent() { let _ = std::fs::create_dir_all(pai); }
        let _ = std::fs::write(&local, &texto);
        texto
    }).or_else(|| std::fs::read_to_string(&local).ok());
    let Some(texto) = texto else {
        return Vec::new();
    };
    let mut saida = Vec::new();
    let mut identidade: Option<(String, String)> = None;
    let mut episodios = Vec::new();
    let concluir = |identidade: &mut Option<(String, String)>, episodios: &mut Vec<Episodio>,
                    saida: &mut Vec<(Serie, Vec<Episodio>)>| {
        let Some((titulo, ano)) = identidade.take() else { return };
        if episodios.is_empty() { return; }
        let lista = std::mem::take(episodios);
        saida.push((Serie { titulo, ano, pedaco: 0, episodios: lista.len() }, lista));
    };
    for linha in texto.lines() {
        if let Some(cabecalho) = linha.strip_prefix('@') {
            concluir(&mut identidade, &mut episodios, &mut saida);
            let campos: Vec<&str> = cabecalho.split('\t').collect();
            identidade = campos.first().filter(|titulo| !titulo.is_empty()).map(|titulo| {
                ((*titulo).to_string(), campos.get(1).copied().unwrap_or("").to_string())
            });
            continue;
        }
        if identidade.is_none() { continue; }
        let campos: Vec<&str> = linha.split('\t').collect();
        if campos.len() < 4 { continue; }
        let urls: Vec<String> = campos[3].split(',').filter_map(montar).collect();
        if urls.is_empty() { continue; }
        episodios.push(Episodio {
            temporada: campos[0].parse().unwrap_or(0),
            numero: campos[1].parse().unwrap_or(0),
            versao: campos[2].to_string(), urls,
        });
    }
    concluir(&mut identidade, &mut episodios, &mut saida);
    saida
}

/// Episódios de uma série: baixa só o pedaço em que ela está.
pub fn episodios(letra: &str, serie: &Serie) -> Vec<Episodio> {
    let nome = format!("series-{}-{}.txt", gaveta_do_arquivo(letra), serie.pedaco);
    let Some(texto) = arquivo(&nome) else { return Vec::new() };
    let mut saida = Vec::new();
    let mut dentro = false;
    for linha in texto.lines() {
        if let Some(identidade) = linha.strip_prefix('@') {
            if dentro {
                break;
            }
            let campos: Vec<&str> = identidade.split('\t').collect();
            dentro = campos.first().copied().unwrap_or("") == serie.titulo
                && campos.get(1).copied().unwrap_or("") == serie.ano;
            continue;
        }
        if !dentro {
            continue;
        }
        let campos: Vec<&str> = linha.split('\t').collect();
        if campos.len() < 4 {
            continue;
        }
        let urls: Vec<String> = campos[3].split(',').filter_map(montar).collect();
        if urls.is_empty() {
            continue;
        }
        saida.push(Episodio {
            temporada: campos[0].parse().unwrap_or(0),
            numero: campos[1].parse().unwrap_or(0),
            versao: campos[2].to_string(),
            urls,
        });
    }
    saida.sort_by_key(|e| (e.temporada, e.numero));
    saida
}

#[derive(Clone, Debug)]
pub struct Achado {
    pub titulo: String,
    pub serie: bool,
    pub letra: String,
    pub ano: String,
}

/// O acervo inteiro só com nome, tipo e letra (uns 800 KB para trinta mil
/// títulos): dá para procurar em tudo sem baixar tudo.
pub fn busca() -> Vec<Achado> {
    let Some(texto) = arquivo("busca.txt") else { return Vec::new() };
    texto
        .lines()
        .filter_map(|linha| {
            let campos: Vec<&str> = linha.split('\t').collect();
            if campos.len() < 3 || campos[0].is_empty() {
                return None;
            }
            Some(Achado {
                titulo: campos[0].to_string(),
                serie: campos[1] == "s",
                letra: campos[2].to_string(),
                ano: campos.get(3).copied().unwrap_or("").to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    #[ignore = "usa a rede"]
    fn le_o_acervo_publicado() {
        let gavetas = indice();
        println!("gavetas: {} | primeira: {:?}", gavetas.len(), gavetas.first());
        assert!(gavetas.len() > 20);

        let filmes = filmes("A");
        println!("filmes em A: {}", filmes.len());
        let exemplo = filmes.iter().find(|f| !f.versoes.is_empty()).expect("algum filme");
        println!("{} -> {:?}", exemplo.titulo, exemplo.versoes[0].1.first());
        assert!(exemplo.versoes[0].1[0].starts_with("http"));

        let series = series("A");
        let serie = series.first().expect("alguma série").clone();
        let episodios = episodios("A", &serie);
        println!("série {} ({} episódios no índice): {} lidos, 1º: {:?}",
                 serie.titulo, serie.episodios, episodios.len(),
                 episodios.first().map(|e| (e.temporada, e.numero, e.urls.first().cloned())));
        assert!(!episodios.is_empty());

        let tudo = busca();
        println!("acervo: {} títulos", tudo.len());
        assert!(tudo.len() > 10_000);
    }
}
