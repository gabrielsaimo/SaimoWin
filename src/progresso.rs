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

pub fn onde_parou(titulo: &str) -> Option<Marca> {
    let guarda = marcas();
    let (posicao, duracao) = guarda.as_ref()?.get(titulo).copied()?;
    Some(Marca { posicao, duracao })
}

/// Guarda a posição. Começo e fim apagam a marca em vez de gravá-la.
pub fn salvar(titulo: &str, posicao: f64, duracao: f64) {
    if titulo.is_empty() || duracao <= 0.0 || !posicao.is_finite() {
        return;
    }
    {
        let mut guarda = marcas();
        let mapa = guarda.as_mut().unwrap();
        if posicao < MINIMO || posicao > duracao - SOBRA {
            if mapa.remove(titulo).is_none() {
                return;
            }
        } else {
            mapa.insert(titulo.to_string(), (posicao, duracao));
        }
    }
    gravar();
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
    let restante = (marca.duracao - marca.posicao).max(0.0) as i64;
    let horas = restante / 3600;
    let minutos = (restante % 3600) / 60;
    if horas > 0 {
        format!("faltam {horas} h {minutos:02} min")
    } else {
        format!("faltam {minutos} min")
    }
}
