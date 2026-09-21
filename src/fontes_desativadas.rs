//! Servidores desligados à mão, no painel do monitor.
//!
//! Quando um provedor cai, cai inteiro: não é a fonte 3 de um canal que
//! morreu, é o servidor que parou de responder para todo mundo. Editar o
//! catálogo publicado a cada queda é lento e some com o link, que depois
//! precisa voltar. Desligar o servidor no painel some com ele de todo canal e
//! de todo filme, em todos os aplicativos, e religar devolve tudo.
//!
//! A lista é baixada sem chave nenhuma: são nomes de servidor, que o catálogo
//! publicado já mostra, e exigir segredo significaria embutir um segredo num
//! programa que qualquer um baixa.
//!
//! Sem rede a lista fica vazia e nada é escondido — o erro certo a cometer: um
//! canal a mais na tela é melhor que a lista inteira sumindo porque o monitor
//! não respondeu.

use std::collections::HashSet;
use std::sync::RwLock;
use std::time::{Duration, Instant};

const ENDERECO: &str = "https://saimo-monitor.gabrielsaimo68.workers.dev/v1/fontes";
/// Desligar um servidor tem que valer em minutos, que é o tempo que alguém
/// aguenta um canal quebrado.
const VALIDADE: Duration = Duration::from_secs(120);

static HOSTS: RwLock<Option<HashSet<String>>> = RwLock::new(None);
static LIDO_EM: RwLock<Option<Instant>> = RwLock::new(None);

fn hosts() -> HashSet<String> {
    HOSTS.read().ok().and_then(|h| h.clone()).unwrap_or_default()
}

/// Busca a lista quando ela envelheceu. Devolve verdadeiro quando mudou, que é
/// quando quem chamou precisa remontar a lista de canais.
pub fn atualizar() -> bool {
    if let Ok(marca) = LIDO_EM.read() {
        if marca.map(|m| m.elapsed() < VALIDADE).unwrap_or(false) {
            return false;
        }
    }
    let Some(corpo) = crate::rede::json(ENDERECO) else { return false };
    let Some(lista) = corpo.get("desativados").and_then(|d| d.as_array()) else { return false };

    if let Ok(mut marca) = LIDO_EM.write() {
        *marca = Some(Instant::now());
    }
    let novos: HashSet<String> = lista
        .iter()
        .filter_map(|v| v.as_str())
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let mudou = hosts() != novos;
    if mudou {
        if novos.is_empty() {
            eprintln!("nenhum servidor desligado");
        } else {
            let mut nomes: Vec<&str> = novos.iter().map(|s| s.as_str()).collect();
            nomes.sort_unstable();
            eprintln!("servidores desligados: {}", nomes.join(", "));
        }
        if let Ok(mut guarda) = HOSTS.write() {
            *guarda = Some(novos);
        }
    }
    mudou
}

/// O servidor de um endereço, em minúsculas: o pedaço entre "//" e a primeira
/// barra, sem usuário nem porta.
fn servidor(url: &str) -> Option<String> {
    let resto = url.split("://").nth(1).unwrap_or(url);
    let autoridade = resto.split(['/', '?', '#']).next()?;
    let sem_usuario = autoridade.rsplit('@').next()?;
    let so_host = sem_usuario.split(':').next()?;
    if so_host.is_empty() { None } else { Some(so_host.to_lowercase()) }
}

pub fn desligado(url: &str) -> bool {
    let hosts = hosts();
    if hosts.is_empty() {
        return false;
    }
    servidor(url).map(|h| hosts.contains(&h)).unwrap_or(false)
}

/// Os endereços de um filme ou episódio sem os que estão desligados.
pub fn peneirar(urls: Vec<String>) -> Vec<String> {
    if hosts().is_empty() {
        return urls;
    }
    urls.into_iter().filter(|u| !desligado(u)).collect()
}

/// O catálogo sem o que está desligado.
///
/// Canal que fica sem nenhuma fonte sai da lista: ele não abriria mesmo, e
/// deixá-lo ali só rende clique frustrado. Volta sozinho quando o servidor for
/// religado.
pub fn peneirar_canais(canais: Vec<crate::catalogo::Canal>) -> Vec<crate::catalogo::Canal> {
    if hosts().is_empty() {
        return canais;
    }
    canais
        .into_iter()
        .filter_map(|mut canal| {
            canal.fontes.retain(|f| !desligado(&f.url));
            if canal.fontes.is_empty() { None } else { Some(canal) }
        })
        .collect()
}

#[cfg(test)]
mod testes {
    use super::servidor;

    #[test]
    fn tira_o_servidor_do_endereco() {
        assert_eq!(servidor("https://proxy.nr3.com/a/b.m3u8").as_deref(), Some("proxy.nr3.com"));
        assert_eq!(servidor("http://Proxy.NR3.com:8080/x").as_deref(), Some("proxy.nr3.com"));
        assert_eq!(servidor("https://ana:senha@casa.tv/p").as_deref(), Some("casa.tv"));
        assert_eq!(servidor("nada"), Some("nada".into()));
    }
}
