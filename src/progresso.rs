//! Onde cada filme e episódio parou.
//!
//! A chave é o título (e, na série, temporada e episódio), não o endereço: o
//! mesmo episódio vem de fontes diferentes, e trocar de fonte não pode fazer
//! recomeçar do zero. Fica gravado em disco de propósito — voltar ao ponto em
//! que se parou não é enfeite.

use std::collections::HashMap;
use std::sync::Mutex;

/// Menos de um minuto não é "onde parou", é ter aberto e desistido.
const MINIMO: f64 = 60.0;
/// A dois minutos do fim está visto: retomar ali só irrita.
const SOBRA: f64 = 120.0;

#[derive(Clone, Copy, Debug)]
pub struct Marca {
    pub posicao: f64,
    pub duracao: f64,
}

static MARCAS: Mutex<Option<HashMap<String, (f64, f64)>>> = Mutex::new(None);

fn arquivo() -> std::path::PathBuf {
    crate::catalogo::pasta().join("progresso.json")
}

fn marcas() -> std::sync::MutexGuard<'static, Option<HashMap<String, (f64, f64)>>> {
    let mut guarda = MARCAS.lock().unwrap();
    if guarda.is_none() {
        let lido = std::fs::read_to_string(arquivo())
            .ok()
            .and_then(|t| serde_json::from_str::<HashMap<String, (f64, f64)>>(&t).ok())
            .unwrap_or_default();
        *guarda = Some(lido);
    }
    guarda
}

fn gravar() {
    let guarda = marcas();
    if let Some(mapa) = guarda.as_ref() {
        if let Ok(texto) = serde_json::to_string(mapa) {
            let _ = std::fs::write(arquivo(), texto);
        }
    }
}

/// A fração já vista, de 0 a 1. Zero quando a fonte não diz a duração: aí não
/// há barra a desenhar.
pub fn fracao(marca: &Marca) -> f32 {
    if marca.duracao <= 0.0 {
        return 0.0;
    }
    (marca.posicao / marca.duracao).clamp(0.0, 1.0) as f32
}

pub fn onde_parou(titulo: &str) -> Option<Marca> {
    let guarda = marcas();
    let (posicao, duracao) = guarda.as_ref()?.get(titulo).copied()?;
    Some(Marca { posicao, duracao })
}

/// Guarda a posição. Começo e fim apagam a marca em vez de gravá-la.
///
/// Duração 0 é "a fonte não diz quanto dura" — acontece com o HLS do
/// EmbedPlayer. Exigir a duração fazia o progresso nunca ser gravado nesses
/// filmes; sem ela dá para guardar onde parou do mesmo jeito, só não dá para
/// dizer quanto falta.
pub fn salvar(titulo: &str, posicao: f64, duracao: f64) {
    if titulo.is_empty() || !posicao.is_finite() || duracao < 0.0 {
        return;
    }
    let no_fim = duracao > 0.0 && posicao > duracao - SOBRA;
    {
        let mut guarda = marcas();
        let mapa = guarda.as_mut().unwrap();
        if posicao < MINIMO || no_fim {
            if mapa.remove(titulo).is_none() {
                return;
            }
        } else {
            mapa.insert(titulo.to_string(), (posicao, duracao));
        }
    }
    gravar();
}

/// Onde parou o último episódio visto de uma série.
///
/// A chave do episódio é "Série · T1 E2"; a do cartão é só o nome da série.
/// Sem isto o cartão de série nunca mostrava barra nenhuma.
pub fn onde_parou_na_serie(titulo: &str) -> Option<(String, Marca)> {
    let prefixo = format!("{titulo} · ");
    let guarda = marcas();
    let mapa = guarda.as_ref()?;
    let mut achados: Vec<(&String, &(f64, f64))> = mapa
        .iter()
        .filter(|(chave, _)| chave.starts_with(&prefixo))
        .collect();
    achados.sort_by(|a, b| a.0.cmp(b.0));
    let (chave, (posicao, duracao)) = achados.pop()?;
    Some((chave.clone(), Marca { posicao: *posicao, duracao: *duracao }))
}

/// O que dá para continuar, do mais recente para o mais antigo.
pub fn pendentes() -> Vec<(String, Marca)> {
    let guarda = marcas();
    let Some(mapa) = guarda.as_ref() else { return Vec::new() };
    let mut lista: Vec<(String, Marca)> = mapa
        .iter()
        .map(|(titulo, (posicao, duracao))| {
            (titulo.clone(), Marca { posicao: *posicao, duracao: *duracao })
        })
        .collect();
    lista.sort_by(|a, b| a.0.cmp(&b.0));
    lista
}

pub fn esquecer(titulo: &str) {
    {
        let mut guarda = marcas();
        guarda.as_mut().unwrap().remove(titulo);
    }
    gravar();
}

/// "1 h 12 min" — o quanto falta.
pub fn falta(marca: &Marca) -> String {
    if marca.duracao <= 0.0 {
        // Sem duração, o que dá para dizer é onde parou.
        let visto = marca.posicao.max(0.0) as i64;
        let horas = visto / 3600;
        let minutos = (visto % 3600) / 60;
        return if horas > 0 {
            format!("parou em {horas} h {minutos:02} min")
        } else {
            format!("parou em {minutos} min")
        };
    }
    let restante = (marca.duracao - marca.posicao).max(0.0) as i64;
    let horas = restante / 3600;
    let minutos = (restante % 3600) / 60;
    if horas > 0 {
        format!("faltam {horas} h {minutos:02} min")
    } else {
        format!("faltam {minutos} min")
    }
}
