//! Onde começa e acaba a abertura, a recapitulação e os créditos.
//!
//! Os tempos vêm do TheIntroDB (theintrodb.org), um banco aberto em que quem
//! assiste marca esses trechos. A consulta é pelo id do TMDB que o arquivo de
//! fichas já traz — sem adivinhação: ou o trecho foi marcado para aquele
//! episódio, ou o botão não aparece. Mesma fonte da TV Box, do celular e do site.

use std::collections::HashMap;
use std::sync::Mutex;

const BASE: &str = "https://api.theintrodb.org/v3/media";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tipo {
    Abertura,
    Recapitulacao,
    Creditos,
    Previa,
}

/// Um trecho em segundos. `fim` vazio quer dizer "até o fim do vídeo".
#[derive(Clone, Debug)]
pub struct Trecho {
    pub tipo: Tipo,
    pub inicio: f64,
    pub fim: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct Marcas {
    pub trechos: Vec<Trecho>,
}

impl Marcas {
    pub fn creditos(&self) -> Option<f64> {
        self.trechos.iter().find(|t| t.tipo == Tipo::Creditos).map(|t| t.inicio)
    }

    /// O trecho pulável em que a posição está agora, se houver.
    pub fn em(&self, posicao: f64, duracao: f64) -> Option<&Trecho> {
        self.trechos.iter().find(|t| {
            t.tipo != Tipo::Creditos && posicao >= t.inicio && posicao < t.fim.unwrap_or(duracao) - 1.0
        })
    }
}

impl Tipo {
    pub fn rotulo(self) -> &'static str {
        match self {
            Tipo::Abertura => "Pular abertura",
            Tipo::Recapitulacao => "Pular recapitulação",
            Tipo::Creditos => "Pular créditos",
            Tipo::Previa => "Pular prévia",
        }
    }
}

static GUARDADAS: Mutex<Option<HashMap<String, Marcas>>> = Mutex::new(None);

/// Consulta a rede: chamar fora da thread do desenho.
pub fn buscar(tmdb: u32, temporada: u32, episodio: u32) -> Option<Marcas> {
    if tmdb == 0 {
        return None;
    }
    let chave = format!("{tmdb}|{temporada}|{episodio}");
    if let Some(m) = GUARDADAS.lock().ok()?.get_or_insert_with(HashMap::new).get(&chave) {
        return Some(m.clone());
    }
    let mut url = format!("{BASE}?tmdb_id={tmdb}");
    if temporada > 0 {
        url.push_str(&format!("&season={temporada}&episode={episodio}"));
    }
    let marcas = Marcas { trechos: ler(&crate::rede::json(&url)?) };
    if let Ok(mut g) = GUARDADAS.lock() {
        g.get_or_insert_with(HashMap::new).insert(chave, marcas.clone());
    }
    Some(marcas)
}

pub fn ler(json: &serde_json::Value) -> Vec<Trecho> {
    let mut out = Vec::new();
    for (campo, tipo) in [
        ("intro", Tipo::Abertura),
        ("recap", Tipo::Recapitulacao),
        ("credits", Tipo::Creditos),
        ("preview", Tipo::Previa),
    ] {
        let Some(lista) = json.get(campo).and_then(|v| v.as_array()) else { continue };
        for item in lista {
            let inicio = item.get("start_ms").and_then(|v| v.as_f64()).unwrap_or(0.0) / 1000.0;
            let fim = item.get("end_ms").and_then(|v| v.as_f64()).map(|v| v / 1000.0);
            // Trecho de menos de três segundos é marcação errada.
            if fim.map(|f| f - inicio < 3.0).unwrap_or(false) {
                continue;
            }
            out.push(Trecho { tipo, inicio, fim });
        }
    }
    out
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_trechos_e_ignora_marcacao_curta() {
        let json = serde_json::json!({
            "intro": [{"start_ms": 12000, "end_ms": 71000}],
            "recap": [{"start_ms": null, "end_ms": 1000}],
            "credits": [{"start_ms": 2500000, "end_ms": null}]
        });
        let m = Marcas { trechos: ler(&json) };
        assert_eq!(m.trechos.len(), 2);
        assert_eq!(m.creditos(), Some(2500.0));
        assert_eq!(m.em(30.0, 2700.0).map(|t| t.tipo), Some(Tipo::Abertura));
        assert!(m.em(70.5, 2700.0).is_none());
        assert!(m.em(2600.0, 2700.0).is_none());
    }
}
