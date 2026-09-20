//! Capa de um filme ou série, procurada pelo nome no TMDB.
//!
//! As listas de origem não trazem imagem nenhuma, e são trinta mil títulos: a
//! busca acontece só para o que está na tela. O algoritmo de pontuação é o
//! mesmo do TV Box — título exato, título contido, ano e popularidade —, porque
//! "A 13ª Emenda" casava com qualquer coisa quando se pegava o primeiro
//! resultado.
//!
//! O endereço achado fica guardado em disco: a capa não muda, e sem isso cada
//! abertura refaria trinta buscas.

use std::collections::HashMap;
use std::sync::Mutex;

const CHAVE: &str = "15d2ea6d0dc1d476efbca3eba2b9bbfb";
const BASE: &str = "https://api.themoviedb.org/3";
const PONTUACAO_MINIMA: i32 = 10;

struct Resultado {
    titulo: String,
    original: String,
    data: String,
    poster: String,
    votos: i64,
}

static MEMORIA: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
/// Três buscas ao mesmo tempo: rolar a lista depressa não pode virar cinquenta
/// pedidos ao TMDB.
static EM_VOO: Mutex<usize> = Mutex::new(0);
/// O que já foi pedido nesta abertura, achado ou não. A tela redesenha muitas
/// vezes por segundo e pediria a mesma capa em todas elas.
static PEDIDAS: Mutex<Option<std::collections::HashSet<String>>> = Mutex::new(None);

fn arquivo() -> std::path::PathBuf {
    // v2: a primeira versão gravava "não achei" também quando a busca falhava
    // por rede, e o engano ficava para sempre.
    crate::catalogo::pasta().join("capas-v2.json")
}

fn memoria() -> std::sync::MutexGuard<'static, Option<HashMap<String, String>>> {
    let mut guarda = MEMORIA.lock().unwrap();
    if guarda.is_none() {
        let lido = std::fs::read_to_string(arquivo())
            .ok()
            .and_then(|t| serde_json::from_str::<HashMap<String, String>>(&t).ok())
            .unwrap_or_default();
        *guarda = Some(lido);
    }
    guarda
}

fn gravar() {
    let guarda = memoria();
    if let Some(mapa) = guarda.as_ref() {
        if let Ok(texto) = serde_json::to_string(mapa) {
            let _ = std::fs::write(arquivo(), texto);
        }
    }
}

fn chave(titulo: &str, serie: bool) -> String {
    format!("{}{titulo}", if serie { "s:" } else { "f:" })
}

/// O que já se sabe: o endereço da capa, ou nada. `Some("")` é "já procurei e
/// não existe" — guardado também, senão a busca infrutífera se repete.
pub fn conhecida(titulo: &str, serie: bool) -> Option<String> {
    memoria().as_ref()?.get(&chave(titulo, serie)).cloned()
}

/// Guarda uma capa que já veio pronta, sem procurar ninguém.
///
/// As fileiras da tela inicial trazem o endereço do pôster junto, resolvido no
/// repositório. Anotando aqui, o desenho acha a capa no mesmo lugar de sempre e
/// nenhuma busca sai para o TMDB — que é o que deixa a tela abrir na hora.
pub fn anotar(titulo: &str, serie: bool, url: &str) {
    if url.is_empty() {
        return;
    }
    let mut guarda = memoria();
    let mapa = guarda.get_or_insert_with(Default::default);
    mapa.entry(chave(titulo, serie)).or_insert_with(|| url.to_string());
}

/// Procura a capa numa thread e chama `pronto` com o endereço achado.
pub fn procurar(titulo: String, serie: bool, pronto: impl FnOnce(String) + Send + 'static) {
    {
        if memoria().as_ref().unwrap().contains_key(&chave(&titulo, serie)) {
            return;
        }
        let mut pedidas = PEDIDAS.lock().unwrap();
        let pedidas = pedidas.get_or_insert_with(Default::default);
        if !pedidas.insert(chave(&titulo, serie)) {
            return;
        }
    }
    std::thread::spawn(move || {
        loop {
            {
                let mut em_voo = EM_VOO.lock().unwrap();
                if *em_voo < 3 {
                    *em_voo += 1;
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(120));
        }
        let achado = melhor_poster(&titulo, serie);
        *EM_VOO.lock().unwrap() -= 1;
        // `None` é "a busca não respondeu": não vira "não existe capa", senão o
        // engano de uma rede ruim ficaria gravado para sempre.
        let Some(achado) = achado else {
            PEDIDAS.lock().unwrap().as_mut().unwrap().remove(&chave(&titulo, serie));
            return;
        };
        {
            let mut guarda = memoria();
            guarda.as_mut().unwrap().insert(chave(&titulo, serie), achado.clone());
        }
        gravar();
        if !achado.is_empty() {
            pronto(achado);
        }
    });
}

/// `None` quando o TMDB não respondeu; `Some(vazio)` quando respondeu e não
/// achou nada — são coisas diferentes na hora de guardar o resultado.
fn buscar_uma_vez(termo: &str, tv: bool, idioma: &str) -> Option<Vec<Resultado>> {
    let rota = if tv { "search/tv" } else { "search/movie" };
    let url = format!("{BASE}/{rota}?query={}&api_key={CHAVE}&language={idioma}", url_encode(termo));
    let dados = crate::rede::json(&url)?;
    let lista = dados.get("results")?.as_array()?;
    Some(lista
        .iter()
        .map(|item| {
            let texto = |campo: &str| item.get(campo).and_then(|v| v.as_str()).unwrap_or("").to_string();
            Resultado {
                titulo: texto(if tv { "name" } else { "title" }),
                original: texto(if tv { "original_name" } else { "original_title" }),
                data: texto(if tv { "first_air_date" } else { "release_date" }),
                poster: texto("poster_path"),
                votos: item.get("vote_count").and_then(|v| v.as_i64()).unwrap_or(0),
            }
        })
        .collect())
}

/// pt-BR primeiro; sem resultado, en-US.
fn buscar(termo: &str, tv: bool) -> Option<Vec<Resultado>> {
    let pt = buscar_uma_vez(termo, tv, "pt-BR")?;
    if !pt.is_empty() {
        return Some(pt);
    }
    buscar_uma_vez(termo, tv, "en-US")
}

/// `None` quando nem deu para procurar; `Some("")` quando procurou e não achou.
fn melhor_poster(nome: &str, serie: bool) -> Option<String> {
    let ano = ano_do_titulo(nome);
    let variantes = variantes(nome);
    let mut melhor: Option<String> = None;
    let mut pontos_do_melhor = 0;
    let mut respondeu = false;

    for tipo in [serie, !serie] {
        for variante in &variantes {
            let Some(resultados) = buscar(variante, tipo) else { continue };
            respondeu = true;
            let unico = resultados.len() == 1;
            for resultado in resultados.iter().take(5) {
                let pontos = pontuar(nome, resultado, ano, unico);
                if pontos > pontos_do_melhor && !resultado.poster.is_empty() {
                    pontos_do_melhor = pontos;
                    melhor = Some(resultado.poster.clone());
                }
            }
            if pontos_do_melhor >= 90 {
                break;
            }
        }
        if pontos_do_melhor >= PONTUACAO_MINIMA {
            break;
        }
    }

    if !respondeu {
        return None;
    }
    if pontos_do_melhor < PONTUACAO_MINIMA {
        return Some(String::new());
    }
    // O TMDB devolve só o caminho ("/abc.jpg"). w342 basta para a lista.
    Some(melhor.map(|caminho| format!("https://image.tmdb.org/t/p/w342{caminho}")).unwrap_or_default())
}

fn pontuar(nome: &str, resultado: &Resultado, ano: Option<i32>, unico: bool) -> i32 {
    let local = achatar(&limpar(nome));
    let titulo = achatar(&resultado.titulo);
    let original = achatar(&resultado.original);

    let mut pontos = if local == titulo || local == original {
        100
    } else if titulo.starts_with(&local) || local.starts_with(&titulo) {
        70
    } else if original.starts_with(&local) || local.starts_with(&original) {
        65
    } else if titulo.contains(&local) || local.contains(&titulo) {
        50
    } else if original.contains(&local) || local.contains(&original) {
        45
    } else if unico {
        // A busca do TMDB casa por título traduzido que nem `title` nem
        // `original_title` revelam de volta: um resultado só já é sinal.
        12
    } else {
        0
    };

    if resultado.votos > 1000 {
        pontos += 15;
    } else if resultado.votos > 100 {
        pontos += 8;
    }

    if let (Some(ano), true) = (ano, resultado.data.len() >= 4) {
        if let Ok(ano_tmdb) = resultado.data[..4].parse::<i32>() {
            let diferenca = (ano - ano_tmdb).abs();
            pontos += match diferenca {
                0 => 25,
                1 => 10,
                d if d > 2 => -25,
                _ => 0,
            };
        }
    }
    pontos
}

/// Só letras, números e espaço, sem acento — para comparar nomes.
fn achatar(texto: &str) -> String {
    let base = crate::catalogo::chave_de_ordem(texto);
    let limpo: String = base
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect();
    limpo.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Tira colchetes, qualidade e dublagem, mas mantém o ano: é ele que separa a
/// capa de uma refilmagem da outra.
pub fn limpar(titulo: &str) -> String {
    let mut texto = String::new();
    let mut nivel = 0;
    for c in titulo.chars() {
        match c {
            '[' => nivel += 1,
            ']' => nivel = (nivel as i32 - 1).max(0) as usize,
            _ if nivel == 0 => texto.push(c),
            _ => {}
        }
    }
    const RUIDO: [&str; 14] = [
        "4k", "uhd", "fhd", "hd", "sd", "h265", "hevc", "hdr", "dv", "dual", "remux", "legendado",
        "dublado", "dub",
    ];
    let palavras: Vec<&str> = texto
        .split_whitespace()
        .filter(|p| !RUIDO.contains(&p.to_lowercase().trim_matches(|c: char| !c.is_alphanumeric())))
        .collect();
    palavras.join(" ").trim().to_string()
}

fn ano_do_titulo(titulo: &str) -> Option<i32> {
    let bytes: Vec<char> = titulo.chars().collect();
    for janela in bytes.windows(4) {
        let texto: String = janela.iter().collect();
        if texto.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(ano) = texto.parse::<i32>() {
                if (1900..2100).contains(&ano) {
                    return Some(ano);
                }
            }
        }
    }
    None
}

/// Título limpo, sem artigo inicial, sem subtítulo depois de ":" ou "-", e sem
/// algarismo romano no fim.
fn variantes(nome: &str) -> Vec<String> {
    let limpo = limpar(nome);
    let mut saida = vec![limpo.clone()];
    let mut juntar = |texto: String| {
        if texto.len() > 1 && !saida.contains(&texto) {
            saida.push(texto);
        }
    };

    let primeira = limpo.split_whitespace().next().unwrap_or("").to_lowercase();
    if ["o", "a", "os", "as", "um", "uma", "the", "an"].contains(&primeira.as_str()) {
        juntar(limpo.splitn(2, ' ').nth(1).unwrap_or("").trim().to_string());
    }
    for corte in [": ", " - "] {
        if let Some(pos) = limpo.find(corte) {
            juntar(limpo[..pos].trim().to_string());
        }
    }
    let sem_acento = crate::catalogo::chave_de_ordem(&limpo);
    if sem_acento != limpo.to_lowercase() {
        juntar(sem_acento);
    }
    for romano in ["II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"] {
        if let Some(resto) = limpo.strip_suffix(&format!(" {romano}")) {
            juntar(resto.trim().to_string());
        }
    }
    saida
}

fn url_encode(texto: &str) -> String {
    let mut saida = String::new();
    for byte in texto.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                saida.push(*byte as char)
            }
            b' ' => saida.push('+'),
            outro => saida.push_str(&format!("%{outro:02X}")),
        }
    }
    saida
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    #[ignore = "usa a rede"]
    fn acha_capa_de_filme_conhecido() {
        for (titulo, serie) in [("Matrix", false), ("A 5ª Onda", false), ("Breaking Bad", true)] {
            let achado = melhor_poster(titulo, serie);
            println!("{titulo}: {achado:?}");
            assert!(achado.expect("o TMDB respondeu").starts_with("https://image.tmdb.org/"));
        }
    }
}
