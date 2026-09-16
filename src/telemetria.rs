//! O que o Saimo Monitor fica sabendo deste PC.
//!
//! O mesmo de sempre: que o app abriu (versão, modelo e sistema), o que está
//! tocando de tempos em tempos, e quando uma fonte falha ou um canal cai. O
//! aparelho é um UUID sorteado na primeira abertura, guardado ao lado do cache.
//! Sem rede o monitor fica sem o dado e o app continua igual.

use std::sync::Mutex;
use std::time::Instant;

const BASE: &str = "https://saimo-monitor.gabrielsaimo68.workers.dev/v1";
const PLATAFORMA: &str = "windows";
/// Zapeando, cada canal que passa não vira batida: só quem ficou.
const MINIMO_PARA_CONTAR_S: u64 = 20;

struct Tocando {
    kind: String,
    titulo: String,
    host: Option<String>,
}

struct Estado {
    id: String,
    tocando: Option<Tocando>,
    contando_desde: Instant,
    ultima_batida: Instant,
    batida_s: u64,
    erros: u32,
}

static ESTADO: Mutex<Option<Estado>> = Mutex::new(None);

/// UUID v4 sem depender de mais uma biblioteca: tempo, endereço de pilha e
/// contador bastam para um número que só identifica esta instalação.
fn sorteia_id() -> String {
    let agora = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pilha = &agora as *const _ as usize as u128;
    let mut semente = agora ^ (pilha << 64) ^ (std::process::id() as u128) << 32;
    let mut digitos = String::new();
    for i in 0..32 {
        semente = semente.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let nibble = ((semente >> 64) & 0xF) as u8;
        let nibble = match i {
            12 => 4,                    // versão 4
            16 => (nibble & 0x3) | 0x8, // variante
            _ => nibble,
        };
        digitos.push(char::from_digit(nibble as u32, 16).unwrap_or('0'));
        if matches!(i, 7 | 11 | 15 | 19) {
            digitos.push('-');
        }
    }
    digitos
}

fn id_do_aparelho() -> String {
    let arquivo = crate::catalogo::pasta().join("aparelho.txt");
    if let Ok(guardado) = std::fs::read_to_string(&arquivo) {
        let guardado = guardado.trim().to_string();
        if guardado.len() >= 32 {
            return guardado;
        }
    }
    let novo = sorteia_id();
    let _ = std::fs::write(&arquivo, &novo);
    novo
}

fn modelo() -> String {
    // Sem consultar hardware: o que interessa ao painel é separar PC de TV Box.
    std::env::var("PROCESSOR_ARCHITECTURE")
        .map(|arq| format!("PC Windows ({arq})"))
        .unwrap_or_else(|_| "PC Windows".into())
}

fn sistema() -> String {
    std::env::var("OS").unwrap_or_else(|_| "Windows".into())
}

fn host(url: &str) -> Option<String> {
    let sem_esquema = url.split("://").nth(1)?;
    let host = sem_esquema.split('/').next()?.split('@').last()?;
    Some(host.split(':').next()?.trim_start_matches("www.").to_string())
}

fn enviar(rota: &str, mut corpo: serde_json::Value) {
    let estado = ESTADO.lock().unwrap();
    let Some(estado) = estado.as_ref() else { return };
    corpo["deviceId"] = estado.id.clone().into();
    corpo["platform"] = PLATAFORMA.into();
    corpo["version"] = crate::VERSAO.into();
    crate::rede::postar(format!("{BASE}/{rota}"), corpo.to_string());
}

pub fn iniciar() {
    // Teste da interface fora do Windows não deve virar aparelho no painel.
    if std::env::var("SAIMO_SEM_TELEMETRIA").is_ok() {
        return;
    }
    {
        let mut estado = ESTADO.lock().unwrap();
        if estado.is_some() {
            return;
        }
        *estado = Some(Estado {
            id: id_do_aparelho(),
            tocando: None,
            contando_desde: Instant::now(),
            ultima_batida: Instant::now(),
            batida_s: 300,
            erros: 0,
        });
    }
    // O monitor responde de quanto em quanto tempo quer a batida; sem ler a
    // resposta, um ajuste no painel não chegaria neste app.
    let corpo = {
        let estado = ESTADO.lock().unwrap();
        let Some(estado) = estado.as_ref() else { return };
        serde_json::json!({
            "deviceId": estado.id,
            "platform": PLATAFORMA,
            "version": crate::VERSAO,
            "model": modelo(),
            "os": sistema(),
            "deviceType": "computador",
        })
    };
    std::thread::spawn(move || {
        let Some(resposta) = crate::rede::postar_e_ler(format!("{BASE}/hello"), corpo.to_string()) else { return };
        let Some(segundos) = resposta.get("heartbeatSeconds").and_then(|v| v.as_u64()) else { return };
        if (60..=3600).contains(&segundos) {
            if let Some(estado) = ESTADO.lock().unwrap().as_mut() {
                estado.batida_s = segundos;
            }
        }
    });
}

/// Chamada a cada quadro: manda a batida quando der a hora, e nada além disso.
pub fn no_relogio() {
    let passou = {
        let estado = ESTADO.lock().unwrap();
        match estado.as_ref() {
            Some(e) => e.ultima_batida.elapsed().as_secs() >= e.batida_s,
            None => return,
        }
    };
    if passou {
        bater();
    }
}

fn bater() {
    let corpo = {
        let mut estado = ESTADO.lock().unwrap();
        let Some(estado) = estado.as_mut() else { return };
        let segundos = if estado.tocando.is_some() { estado.contando_desde.elapsed().as_secs() } else { 0 };
        estado.contando_desde = Instant::now();
        estado.ultima_batida = Instant::now();
        let tocando = match &estado.tocando {
            Some(t) => serde_json::json!({ "kind": t.kind, "title": t.titulo, "host": t.host }),
            None => serde_json::Value::Null,
        };
        serde_json::json!({ "seconds": segundos, "playing": tocando })
    };
    enviar("beat", corpo);
}

fn evento(tipo: &str, kind: Option<&str>, titulo: Option<&str>, url: Option<&str>,
          fonte: Option<usize>, detalhe: Option<String>) {
    enviar("event", serde_json::json!({
        "type": tipo,
        "kind": kind,
        "title": titulo,
        "host": url.and_then(host),
        "source": fonte.map(|f| f as i64),
        "detail": detalhe,
    }));
}

/// `nova` é falso quando é só a próxima fonte do mesmo canal depois de uma
/// falha: aí não conta como mais uma abertura.
pub fn comecou(titulo: &str, url: &str, fonte: usize, nova: bool) {
    // O que fecha a conta do canal anterior é decidido com o cadeado na mão,
    // mas a batida só sai depois de soltá-lo: `bater` tranca o mesmo cadeado.
    let fechar_conta = {
        let estado = ESTADO.lock().unwrap();
        match estado.as_ref() {
            Some(e) => {
                e.tocando.as_ref().map(|t| t.titulo != titulo).unwrap_or(false)
                    && e.contando_desde.elapsed().as_secs() >= MINIMO_PARA_CONTAR_S
            }
            None => return,
        }
    };
    if fechar_conta {
        bater();
    }
    {
        let mut estado = ESTADO.lock().unwrap();
        let Some(estado) = estado.as_mut() else { return };
        if estado.tocando.as_ref().map(|t| t.titulo != titulo).unwrap_or(true) {
            estado.contando_desde = Instant::now();
        }
        estado.tocando = Some(Tocando {
            kind: "live".into(),
            titulo: titulo.to_string(),
            host: host(url),
        });
    }
    if nova {
        evento("play_start", Some("live"), Some(titulo), Some(url), Some(fonte), None);
    }
}

pub fn tocou(titulo: &str, url: &str, fonte: usize, ms: u128) {
    evento("play_ok", Some("live"), Some(titulo), Some(url), Some(fonte), Some(format!("{ms} ms")));
}

pub fn falhou(titulo: &str, url: &str, fonte: usize, detalhe: &str) {
    evento("source_fail", Some("live"), Some(titulo), Some(url), Some(fonte), Some(detalhe.into()));
}

pub fn caiu(titulo: &str, fontes: usize) {
    evento("channel_down", Some("live"), Some(titulo), None, None,
           Some(format!("nenhuma das {fontes} fonte(s) abriu")));
}

pub fn parou() {
    let fechar_conta = {
        let estado = ESTADO.lock().unwrap();
        match estado.as_ref() {
            Some(e) if e.tocando.is_some() => {
                e.contando_desde.elapsed().as_secs() >= MINIMO_PARA_CONTAR_S
            }
            _ => return,
        }
    };
    if fechar_conta {
        bater();
    }
    {
        let mut estado = ESTADO.lock().unwrap();
        if let Some(estado) = estado.as_mut() {
            estado.tocando = None;
        }
    }
    evento("play_stop", None, None, None, None, None);
}

/// Falha engolida: conta, mas no máximo algumas por abertura.
pub fn erro(detalhe: String) {
    {
        let mut estado = ESTADO.lock().unwrap();
        let Some(estado) = estado.as_mut() else { return };
        if estado.erros >= 5 {
            return;
        }
        estado.erros += 1;
    }
    evento("error", None, None, None, None, Some(detalhe));
}

pub fn fechando() {
    let tocando = ESTADO.lock().unwrap().as_ref().map(|e| e.tocando.is_some()).unwrap_or(false);
    bater();
    if tocando {
        evento("play_stop", None, None, None, None, None);
    }
}
