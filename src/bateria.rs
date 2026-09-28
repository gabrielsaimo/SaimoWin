//! Bateria contra a rede de verdade: a lista publicada, o acervo e o TheIntroDB.
//!
//! Não roda no `cargo test` comum (depende da internet e leva um minuto):
//!
//! ```text
//! cargo test --release bateria -- --ignored --nocapture
//! ```
//!
//! Serve para separar "o app está quebrado" de "a fonte está fora": se a
//! bateria acha a fonte viva e o programa diz que o canal caiu, o erro é nosso.

#![cfg(test)]

use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Como a fonte respondeu.
#[derive(Debug, Clone, PartialEq)]
enum Resposta {
    Viva(&'static str),
    Morta(String),
}

fn sondar(url: &str, referer: Option<&str>, agente: Option<&str>) -> Resposta {
    let cliente: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(8)))
        .timeout_recv_response(Some(Duration::from_secs(10)))
        .timeout_recv_body(Some(Duration::from_secs(10)))
        .user_agent(agente.unwrap_or(crate::AGENTE))
        .build()
        .into();
    let mut pedido = cliente.get(url).header("Accept", "*/*").header("Range", "bytes=0-4095");
    if let Some(r) = referer {
        pedido = pedido.header("Referer", r);
    }
    let resposta = match pedido.call() {
        Ok(r) => r,
        Err(ureq::Error::StatusCode(codigo)) => return Resposta::Morta(format!("HTTP {codigo}")),
        Err(e) => return Resposta::Morta(e.to_string()),
    };
    let mut inicio = Vec::new();
    let _ = resposta.into_body().into_reader().take(4096).read_to_end(&mut inicio);
    let texto = String::from_utf8_lossy(&inicio);
    if texto.contains("#EXTM3U") {
        Resposta::Viva("hls")
    } else if texto.contains("<MPD") {
        Resposta::Viva("dash")
    } else if inicio.first() == Some(&0x47) {
        Resposta::Viva("ts")
    } else if inicio.len() > 8 && &inicio[4..8] == b"ftyp" {
        Resposta::Viva("mp4")
    } else if inicio.is_empty() {
        Resposta::Morta("resposta vazia".into())
    } else if texto.trim_start().starts_with('<') {
        Resposta::Morta("página HTML no lugar do vídeo".into())
    } else {
        // Binário que não reconhecemos (mkv, fMP4 sem ftyp no começo…).
        Resposta::Viva("binário")
    }
}

/// Roda `tarefa` em `paralelo` threads sobre os itens, na ordem de chegada.
fn em_paralelo<T: Send + Clone + 'static, R: Send + 'static>(
    itens: Vec<T>,
    paralelo: usize,
    tarefa: impl Fn(T) -> R + Send + Sync + 'static,
) -> Vec<R> {
    let fila = Arc::new(Mutex::new(itens.into_iter().enumerate().collect::<Vec<_>>()));
    let saida = Arc::new(Mutex::new(Vec::new()));
    let tarefa = Arc::new(tarefa);
    let threads: Vec<_> = (0..paralelo)
        .map(|_| {
            let (fila, saida, tarefa) = (fila.clone(), saida.clone(), tarefa.clone());
            std::thread::spawn(move || loop {
                let Some((i, item)) = fila.lock().unwrap().pop() else { return };
                let r = tarefa(item);
                saida.lock().unwrap().push((i, r));
            })
        })
        .collect();
    for t in threads {
        let _ = t.join();
    }
    let mut saida = Arc::try_unwrap(saida).ok().unwrap().into_inner().unwrap();
    saida.sort_by_key(|(i, _)| *i);
    saida.into_iter().map(|(_, r)| r).collect()
}

#[test]
#[ignore]
fn bateria_canais_publicados() {
    let canais = crate::catalogo::baixar("catalogo.txt").expect("catalogo.txt não baixou");
    assert!(canais.len() > 50, "lista publicada veio pequena demais: {}", canais.len());
    let total = canais.len();

    let resultados = em_paralelo(canais, 24, |canal| {
        let mut notas = Vec::new();
        // Até três fontes por canal: é o que a pessoa espera antes de desistir.
        for fonte in canal.fontes.iter().take(3) {
            let r = sondar(&fonte.url, fonte.referer.as_deref(), fonte.agente.as_deref());
            let viva = matches!(r, Resposta::Viva(_));
            notas.push(r);
            if viva {
                break;
            }
        }
        (canal.nome, notas)
    });

    let mortos: Vec<_> = resultados
        .iter()
        .filter(|(_, n)| !n.iter().any(|r| matches!(r, Resposta::Viva(_))))
        .collect();
    let vivos = total - mortos.len();
    println!("\n== canais: {vivos} de {total} com fonte viva ({:.0}%)", vivos as f64 * 100.0 / total as f64);
    for (nome, notas) in &mortos {
        println!("   fora: {nome} — {notas:?}");
    }
    // Referência, não perfeição: fontes caem o tempo todo. Abaixo disso é
    // sinal de problema no app (cabeçalho, TLS) e não nas fontes.
    assert!(vivos * 100 / total >= 50, "menos da metade dos canais respondeu");
}

#[test]
#[ignore]
fn bateria_acervo_abre() {
    let filmes = crate::vod::filmes("A");
    assert!(!filmes.is_empty(), "letra A do acervo veio vazia");
    let amostra: Vec<_> = filmes
        .into_iter()
        .filter(|f| !f.reservado)
        .step_by(97)
        .take(25)
        .filter_map(|f| f.versoes.first().and_then(|(_, urls)| urls.first().cloned()).map(|u| (f.titulo, u)))
        .collect();
    let total = amostra.len();
    let resultados = em_paralelo(amostra, 12, |(titulo, url)| (titulo, sondar(&url, None, None)));
    let vivos = resultados.iter().filter(|(_, r)| matches!(r, Resposta::Viva(_))).count();
    println!("\n== filmes (amostra da letra A): {vivos} de {total} abrem");
    for (titulo, r) in resultados.iter().filter(|(_, r)| !matches!(r, Resposta::Viva(_))) {
        println!("   fora: {titulo} — {r:?}");
    }
    assert!(vivos * 100 / total.max(1) >= 50, "menos da metade da amostra abriu");
}

#[test]
#[ignore]
fn bateria_theintrodb() {
    // Breaking Bad, T1 E2: abertura marcada pela comunidade.
    let marcas = crate::pulos::buscar(1396, 1, 2).expect("TheIntroDB não respondeu");
    println!("\n== TheIntroDB: {:?}", marcas.trechos);
    assert!(marcas.trechos.iter().any(|t| t.tipo == crate::pulos::Tipo::Abertura));
}
