//! Guia de programação, com as mesmas fontes dos outros aplicativos.
//!
//! Três origens, nesta ordem de prioridade: o guia da própria Pluto TV (casado
//! pelo id da Pluto que está no link do canal) e dois feeds XMLTV, casados pelo
//! nome do canal. O que chega primeiro por canal fica; ninguém sobrescreve
//! ninguém.
//!
//! O trabalho acontece numa thread, com o resultado guardado em disco por seis
//! horas: abrir o aplicativo não espera guia nenhum.

use crate::catalogo::Canal;
use serde_json::json;
use std::collections::HashMap;
use std::io::Read;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const FEEDS: [&str; 2] = [
    "https://iptv-epg.org/files/epg-br.xml",
    "https://www.open-epg.com/files/brazil3.xml",
];
const PLUTO: &str = "https://raw.githubusercontent.com/matthuisman/i.mjh.nz/master/PlutoTV/br.xml";
const VALIDADE_S: i64 = 6 * 3600;
const JANELA_PASSADA_S: i64 = 6 * 3600;
const JANELA_FUTURA_S: i64 = 3 * 86400;

#[derive(Clone, Debug)]
pub struct Programa {
    pub titulo: String,
    pub descricao: String,
    pub categoria: String,
    /// Epoch em segundos.
    pub inicio: i64,
    pub fim: i64,
}

impl Programa {
    pub fn no_ar(&self, agora: i64) -> bool {
        self.inicio <= agora && self.fim > agora
    }

    /// De 0 a 1, o quanto já passou.
    pub fn andamento(&self, agora: i64) -> f32 {
        let total = (self.fim - self.inicio).max(1) as f32;
        (((agora - self.inicio) as f32) / total).clamp(0.0, 1.0)
    }

    pub fn horario(&self) -> String {
        format!("{} – {}", hora_local(self.inicio), hora_local(self.fim))
    }
}

/// Nomes que os feeds escrevem diferente do catálogo.
const APELIDOS: [(&str, &str); 16] = [
    ("adult swim", "trutv"),
    ("history", "history channel"),
    ("sony channel", "sony"),
    ("sportv 2", "sportv2"),
    ("sportv 3", "sportv3"),
    ("gnt", "gnt hd"),
    ("band", "band sp"),
    ("warner", "warner channel"),
    ("sbt", "sbt sp"),
    ("discovery id", "investigacao discovery"),
    ("amc", "amc brasil"),
    ("cnn brasil money", "cnn brasil money hd br"),
    ("universal premiere", "universal premiere hd br"),
    ("universal reality", "universal reality br"),
    ("tnt novelas", "tnt novelas br"),
    ("record sp", "recordtv sp"),
];

/// Canais que o meuguia.tv lista, com o código de cada um. É a fonte com o
/// horário mais confiável da TV aberta e dos canais grandes — os feeds XMLTV
/// erram a grade de vários deles.
const MEUGUIA: [(&str, &str); 67] = [
    ("a e", "MDO"),
    ("animal planet", "APL"),
    ("band", "BAN"),
    ("band news", "NEW"),
    ("band sports", "BSP"),
    ("cartoon network", "CAR"),
    ("cinemax", "MNX"),
    ("combate", "135"),
    ("discovery kids", "DIK"),
    ("discovery world", "DIW"),
    ("discovery channel", "DIS"),
    ("discovery home health", "HEA"),
    ("discovery science", "DSC"),
    ("discovery turbo", "DTU"),
    ("espn", "ESP"),
    ("espn 2", "ES2"),
    ("espn 3", "ES3"),
    ("espn 4", "ES4"),
    ("espn 5", "ES5"),
    ("gnt", "GNT"),
    ("globo rj", "GRD"),
    ("globo sp", "GRD"),
    ("globo", "GRD"),
    ("globonews", "GLN"),
    ("gloob", "GOB"),
    ("gloobinho", "GBI"),
    ("hbo", "HBO"),
    ("hbo2", "HB2"),
    ("hbo family", "HFA"),
    ("hbo plus", "HPL"),
    ("history", "HIS"),
    ("megapix", "MPX"),
    ("multishow", "MSH"),
    ("record", "REC"),
    ("record news", "RCN"),
    ("redetv", "RTV"),
    ("sbt", "SBT"),
    ("space", "SPA"),
    ("sportv", "SPO"),
    ("sportv 2", "SP2"),
    ("sportv 3", "SP3"),
    ("tnt", "TNT"),
    ("tnt series", "TBS"),
    ("tcm", "TCM"),
    ("tlc", "TRV"),
    ("telecine action", "TC2"),
    ("telecine cult", "TC5"),
    ("telecine fun", "TC6"),
    ("telecine pipoca", "TC4"),
    ("telecine premium", "TC1"),
    ("telecine touch", "TC3"),
    ("universal tv", "USA"),
    ("studio universal", "HAL"),
    ("warner", "WBT"),
    ("axn", "AXN"),
    ("canal brasil", "CBR"),
    ("canal off", "OFF"),
    ("e", "EET"),
    ("sony channel", "SET"),
    ("premiere clubes", "121"),
    ("viva", "VIV"),
    ("arte 1", "BQ5"),
    ("amc", "MGM"),
    ("tv brasil", "TED"),
    ("tv aparecida", "TAP"),
    ("tv cultura", "CUL"),
    ("tv gazeta", "GAZ"),
];

static GRADE: Mutex<Option<HashMap<String, Vec<Programa>>>> = Mutex::new(None);

pub fn agora() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Programação do canal, já ordenada.
pub fn grade(canal: &str) -> Vec<Programa> {
    GRADE.lock().unwrap().as_ref().and_then(|g| g.get(canal).cloned()).unwrap_or_default()
}

/// O que está no ar e o que vem depois.
pub fn agora_e_depois(canal: &str) -> Option<(Programa, Option<Programa>)> {
    let guia = GRADE.lock().unwrap();
    let lista = guia.as_ref()?.get(canal)?;
    let instante = agora();
    let posicao = lista.iter().position(|p| p.no_ar(instante))?;
    Some((lista[posicao].clone(), lista.get(posicao + 1).cloned()))
}

pub fn canais_com_guia() -> usize {
    GRADE.lock().unwrap().as_ref().map(|g| g.len()).unwrap_or(0)
}

/// Lê o guia guardado e, se estiver velho, monta um novo em segundo plano.
pub fn carregar(canais: Vec<Canal>, aviso: impl Fn() + Send + 'static) {
    if let Some((quando, grade)) = ler_do_disco() {
        *GRADE.lock().unwrap() = Some(grade);
        aviso();
        if agora() - quando < VALIDADE_S {
            return;
        }
    }
    std::thread::spawn(move || {
        let montada = montar(&canais);
        if montada.is_empty() {
            return;
        }
        gravar_no_disco(&montada);
        *GRADE.lock().unwrap() = Some(montada);
        aviso();
    });
}

fn montar(canais: &[Canal]) -> HashMap<String, Vec<Programa>> {
    let de = agora() - JANELA_PASSADA_S;
    let ate = agora() + JANELA_FUTURA_S;
    let mut grade: HashMap<String, Vec<Programa>> = HashMap::new();

    // A Pluto primeiro: é o guia certo dos canais dela, e o mais leve.
    let (ids_da_pluto, principais) = ids_da_pluto(canais);
    if !ids_da_pluto.is_empty() {
        for (canal, lista) in ler_feed(PLUTO, &ids_da_pluto, de, ate) {
            if principais.contains(&canal) {
                grade.insert(canal, lista);
            }
        }
    }

    // O meuguia manda na TV aberta e nos canais grandes: os feeds XMLTV erram
    // a grade de vários deles. São páginas pequenas, seis por vez.
    for (canal, lista) in do_meuguia(canais, de, ate) {
        grade.entry(canal).or_insert(lista);
    }

    // Os feeds XMLTV, casados pelo nome, só para quem ainda está sem guia.
    let por_nome = nomes_procurados(canais, &principais);
    for feed in FEEDS {
        let achado = ler_feed_por_nome(feed, &por_nome, de, ate);
        for (canal, lista) in achado {
            grade.entry(canal).or_insert(lista);
        }
    }

    // Onde a Pluto é só reserva, a grade dela entra na falta de outra.
    if !ids_da_pluto.is_empty() {
        for (canal, lista) in ler_feed(PLUTO, &ids_da_pluto, de, ate) {
            grade.entry(canal).or_insert(lista);
        }
    }
    grade
}

/// Páginas do meuguia.tv, seis de cada vez: trinta e três downloads ao mesmo
/// tempo tiram banda do vídeo e fazem o site devolver 429.
fn do_meuguia(canais: &[Canal], de: i64, ate: i64) -> HashMap<String, Vec<Programa>> {
    let alvos: Vec<(String, &str)> = canais
        .iter()
        .filter_map(|c| {
            let chave = normalizar(&c.nome);
            MEUGUIA
                .iter()
                .find(|(nome, _)| *nome == chave)
                .map(|(_, codigo)| (c.nome.clone(), *codigo))
        })
        .collect();
    if alvos.is_empty() {
        return HashMap::new();
    }

    let fila = std::sync::Mutex::new(alvos.into_iter());
    let saida: std::sync::Mutex<HashMap<String, Vec<Programa>>> = std::sync::Mutex::new(HashMap::new());
    std::thread::scope(|escopo| {
        for _ in 0..6 {
            escopo.spawn(|| loop {
                let Some((nome, codigo)) = fila.lock().unwrap().next() else { return };
                let url = format!("https://meuguia.tv/programacao/canal/{codigo}");
                let Some(html) = crate::rede::texto(&url) else { continue };
                let lista: Vec<Programa> = ler_meuguia(&html)
                    .into_iter()
                    .filter(|p| p.fim > de && p.inicio < ate)
                    .collect();
                if !lista.is_empty() {
                    saida.lock().unwrap().insert(nome, lista);
                }
            });
        }
    });
    saida.into_inner().unwrap()
}

/// A página do meuguia: cada `<li>` é um programa, com a hora numa `div` de
/// classe `time` e o nome no `<h2>`. O dia vem num cabeçalho "Quarta, 16/09",
/// e o fim de um programa é o começo do seguinte.
fn ler_meuguia(html: &str) -> Vec<Programa> {
    let (ano_atual, mes_atual, dia_atual) = hoje_em_sao_paulo();
    let (mut ano, mut mes, mut dia) = (ano_atual, mes_atual, dia_atual);
    let mut achados: Vec<(i64, String, String)> = Vec::new();

    for item in html.split("<li") {
        if item.contains("subheader") {
            // "Quarta, 16/09" -> dia 16, mês 9.
            if let Some(texto) = item.split_once('>').map(|(_, resto)| resto) {
                let cabecalho: String = texto.chars().take_while(|c| *c != '<').collect();
                if let Some((_, data)) = cabecalho.split_once(',') {
                    let campos: Vec<&str> = data.trim().split('/').collect();
                    if campos.len() == 2 {
                        if let (Ok(d), Ok(m)) = (campos[0].trim().parse::<i64>(), campos[1].trim().parse::<i64>()) {
                            dia = d;
                            mes = m;
                            // A lista não traz o ano e pode atravessar janeiro.
                            ano = if m < mes_atual { ano_atual + 1 } else { ano_atual };
                        }
                    }
                }
            }
            continue;
        }
        let Some(relogio) = entre(item, "lileft time'>", "<").or_else(|| entre(item, "lileft time\">", "<")) else {
            continue;
        };
        let hm: Vec<&str> = relogio.trim().split(':').collect();
        let (Ok(hora), Ok(minuto)) = (hm.first().unwrap_or(&"x").parse::<i64>(),
                                      hm.get(1).unwrap_or(&"x").parse::<i64>()) else { continue };
        let Some(titulo) = entre(item, "<h2>", "</h2>") else { continue };
        let titulo = sem_entidades(titulo.trim());
        if titulo.is_empty() {
            continue;
        }
        let genero = entre(item, "<h3>", "</h3>").map(|g| sem_entidades(g.trim())).unwrap_or_default();
        // O meuguia publica em horário de Brasília e não diz isso em lugar nenhum.
        achados.push((epoch(ano, mes, dia, hora, minuto, 0) + 3 * 3600, titulo, genero));
    }

    achados.sort_by_key(|(inicio, _, _)| *inicio);
    let inicios: Vec<i64> = achados.iter().map(|(i, _, _)| *i).collect();
    achados
        .into_iter()
        .enumerate()
        .map(|(posicao, (inicio, titulo, categoria))| Programa {
            titulo,
            descricao: String::new(),
            categoria,
            inicio,
            fim: inicios.get(posicao + 1).copied().unwrap_or(inicio + 3600),
        })
        .collect()
}

fn entre<'a>(texto: &'a str, abre: &str, fecha: &str) -> Option<&'a str> {
    let inicio = texto.find(abre)? + abre.len();
    let fim = texto[inicio..].find(fecha)? + inicio;
    Some(&texto[inicio..fim])
}

/// Data de hoje em Brasília, para o dia que o meuguia não escreve por extenso.
fn hoje_em_sao_paulo() -> (i64, i64, i64) {
    let dias = (agora() - 3 * 3600).div_euclid(86400);
    // Inverso da fórmula civil usada em `epoch`.
    let z = dias + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// id da Pluto -> canal, e quais canais têm a Pluto como fonte principal.
fn ids_da_pluto(canais: &[Canal]) -> (HashMap<String, String>, Vec<String>) {
    let mut ids = HashMap::new();
    let mut principais = Vec::new();
    for canal in canais {
        let links: Vec<&str> = canal.fontes.iter().map(|f| f.url.as_str()).collect();
        let do_link = links.iter().find_map(|l| id_da_pluto(l));
        let id = match do_link.clone().or_else(|| id_da_pluto(canal.logo.as_deref().unwrap_or(""))) {
            Some(id) => id,
            None => continue,
        };
        ids.entry(id).or_insert_with(|| canal.nome.clone());
        let primeira = links.first().and_then(|l| id_da_pluto(l)).is_some();
        if primeira || do_link.is_none() {
            principais.push(canal.nome.clone());
        }
    }
    (ids, principais)
}

/// "jmp2.uk/plu-<24 hex>" ou "images.pluto.tv/channels/<24 hex>/".
fn id_da_pluto(texto: &str) -> Option<String> {
    for marca in ["plu-", "images.pluto.tv/channels/"] {
        if let Some(pos) = texto.find(marca) {
            let resto: String = texto[pos + marca.len()..].chars().take(24).collect();
            if resto.len() == 24 && resto.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) {
                return Some(resto);
            }
        }
    }
    None
}

fn nomes_procurados(canais: &[Canal], fora: &[String]) -> Vec<String> {
    canais
        .iter()
        .filter(|c| !fora.contains(&c.nome))
        .map(|c| c.nome.clone())
        .collect()
}

/// Nome comparável: sem o "BR -" da frente, sem acento, sem pontuação e sem os
/// "HD"/"BR" do fim. É o mesmo tratamento do Mac e do TV Box, sem o qual o
/// feed ("BR - A&E") nunca casa com o catálogo ("A&E").
pub fn normalizar(texto: &str) -> String {
    let texto = sem_prefixo_de_pais(texto);
    let limpo: String = crate::catalogo::chave_de_ordem(texto)
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect();
    let mut partes: Vec<&str> = limpo.split_whitespace().collect();
    while partes.len() > 1 && matches!(partes[partes.len() - 1], "hd" | "sd" | "fhd" | "uhd" | "4k" | "br") {
        partes.pop();
    }
    partes.join(" ")
}

/// "BR - A&E" e "BR| A&E" viram "A&E"; o resto passa inteiro.
fn sem_prefixo_de_pais(texto: &str) -> &str {
    let bytes = texto.as_bytes();
    if bytes.len() < 4 || !bytes[0].is_ascii_alphabetic() || !bytes[1].is_ascii_alphabetic() {
        return texto;
    }
    let resto = texto[2..].trim_start();
    match resto.strip_prefix(['-', '|']) {
        Some(corpo) => corpo.trim_start(),
        None => texto,
    }
}

/// Um feed lido em fluxo, casando pelo id que o próprio feed publica.
fn ler_feed(url: &str, ids: &HashMap<String, String>, de: i64, ate: i64) -> HashMap<String, Vec<Programa>> {
    percorrer(url, de, ate, |_nomes| ids.clone())
}

/// Um feed lido em fluxo, casando pelo nome do canal.
fn ler_feed_por_nome(url: &str, procurados: &[String], de: i64, ate: i64) -> HashMap<String, Vec<Programa>> {
    percorrer(url, de, ate, |nomes| casar(nomes, procurados))
}

/// Casamento exato e por apelido primeiro, marcando o id como tomado; a regra
/// solta de prefixo só depois, e nunca sobre um id já tomado.
fn casar(nomes: &HashMap<String, Vec<String>>, procurados: &[String]) -> HashMap<String, String> {
    let mut saida = HashMap::new();
    let mut tomados: Vec<String> = Vec::new();
    let mut sobraram: Vec<(String, String)> = Vec::new();

    for nome in procurados {
        let chave = normalizar(nome);
        let ids = nomes.get(&chave).or_else(|| {
            APELIDOS
                .iter()
                .find(|(de, _)| *de == chave)
                .and_then(|(_, para)| nomes.get(*para))
        });
        match ids {
            Some(ids) if !ids.is_empty() => {
                for id in ids {
                    saida.insert(id.clone(), nome.clone());
                    tomados.push(id.clone());
                }
            }
            _ => sobraram.push((nome.clone(), chave)),
        }
    }

    for (nome, chave) in sobraram {
        // Prefixo só quebrando palavra: "viva" não pode casar com "vivax tv".
        let candidatos: Vec<&Vec<String>> = nomes
            .iter()
            .filter(|(k, _)| {
                format!("{k} ").starts_with(&format!("{chave} "))
                    || format!("{chave} ").starts_with(&format!("{k} "))
            })
            .map(|(_, v)| v)
            .collect();
        if candidatos.len() != 1 {
            continue;
        }
        let livres: Vec<String> =
            candidatos[0].iter().filter(|id| !tomados.contains(id)).cloned().collect();
        for id in livres {
            saida.insert(id.clone(), nome.clone());
            tomados.push(id);
        }
    }
    saida
}

/// Lê o XMLTV conforme ele chega, elemento por elemento.
///
/// O maior feed tem 16 MB descomprimidos; virar texto de uma vez só gastaria
/// dezenas de megabytes à toa. Todo `<channel>` vem antes do primeiro
/// `<programme>`, então uma passada basta: o mapa de canais é resolvido no
/// instante em que os programas começam.
fn percorrer(
    url: &str,
    de: i64,
    ate: i64,
    resolver: impl Fn(&HashMap<String, Vec<String>>) -> HashMap<String, String>,
) -> HashMap<String, Vec<Programa>> {
    let mut saida: HashMap<String, Vec<Programa>> = HashMap::new();
    let Some(resposta) = crate::rede::fluxo(url) else { return saida };
    let mut leitor = resposta;
    let mut buffer = String::new();
    let mut pedaco = vec![0u8; 256 * 1024];
    let mut nomes: HashMap<String, Vec<String>> = HashMap::new();
    let mut de_id: Option<HashMap<String, String>> = None;

    loop {
        let lidos = match leitor.read(&mut pedaco) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        buffer.push_str(&String::from_utf8_lossy(&pedaco[..lidos]));
        let mut consumido = 0;
        while let Some((inicio, fim, elemento)) = proximo_elemento(&buffer[consumido..]) {
            let absoluto = consumido + inicio;
            consumido = consumido + fim;
            let _ = absoluto;
            if elemento.starts_with("<channel") {
                if de_id.is_some() {
                    continue;
                }
                if let (Some(id), Some(nome)) = (atributo(&elemento, "id"), texto_de(&elemento, "display-name")) {
                    nomes.entry(normalizar(&nome)).or_default().push(id);
                }
                continue;
            }
            let mapa = match &de_id {
                Some(m) => m,
                None => {
                    de_id = Some(resolver(&nomes));
                    de_id.as_ref().unwrap()
                }
            };
            if mapa.is_empty() {
                continue;
            }
            if let Some((canal, programa)) = ler_programa(&elemento, mapa, de, ate) {
                saida.entry(canal).or_default().push(programa);
            }
        }
        if consumido > 0 {
            buffer.drain(..consumido);
        }
        // Lixo antes do primeiro elemento não pode crescer sem limite.
        if buffer.len() > 4 * 1024 * 1024 {
            buffer.clear();
        }
    }
    for lista in saida.values_mut() {
        lista.sort_by_key(|p| p.inicio);
    }
    saida
}

/// O próximo `<channel>` ou `<programme>` inteiro do buffer.
fn proximo_elemento(texto: &str) -> Option<(usize, usize, String)> {
    for (abre, fecha) in [("<channel", "</channel>"), ("<programme", "</programme>")] {
        if let Some(inicio) = texto.find(abre) {
            if let Some(fim) = texto[inicio..].find(fecha) {
                let fim = inicio + fim + fecha.len();
                // O outro tipo pode começar antes; vence quem aparece primeiro.
                let outro = if abre == "<channel" { texto.find("<programme") } else { texto.find("<channel") };
                if let Some(outro) = outro {
                    if outro < inicio {
                        continue;
                    }
                }
                return Some((inicio, fim, texto[inicio..fim].to_string()));
            }
        }
    }
    None
}

fn atributo(elemento: &str, nome: &str) -> Option<String> {
    let marca = format!("{nome}=\"");
    let cabeca = &elemento[..elemento.find('>').unwrap_or(elemento.len())];
    let inicio = cabeca.find(&marca)? + marca.len();
    let fim = cabeca[inicio..].find('"')? + inicio;
    Some(cabeca[inicio..fim].to_string())
}

fn texto_de(elemento: &str, tag: &str) -> Option<String> {
    let abre = format!("<{tag}");
    let fecha = format!("</{tag}>");
    let inicio = elemento.find(&abre)?;
    let corpo = elemento[inicio..].find('>')? + inicio + 1;
    let fim = elemento[corpo..].find(&fecha)? + corpo;
    let bruto = elemento[corpo..fim].trim();
    if bruto.is_empty() {
        None
    } else {
        Some(sem_entidades(bruto))
    }
}

fn sem_entidades(texto: &str) -> String {
    if !texto.contains('&') {
        return texto.to_string();
    }
    texto
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
}

fn ler_programa(
    elemento: &str,
    de_id: &HashMap<String, String>,
    de: i64,
    ate: i64,
) -> Option<(String, Programa)> {
    let canal = de_id.get(&atributo(elemento, "channel")?)?.clone();
    let inicio = data_xmltv(&atributo(elemento, "start")?)?;
    let fim = data_xmltv(&atributo(elemento, "stop")?)?;
    if fim <= de || inicio >= ate {
        return None;
    }
    let titulo = texto_de(elemento, "title")?;
    Some((
        canal,
        Programa {
            titulo,
            descricao: texto_de(elemento, "desc").unwrap_or_default(),
            categoria: texto_de(elemento, "category").unwrap_or_default(),
            inicio,
            fim,
        },
    ))
}

/// `20260809153000 -0300` vira epoch em segundos.
fn data_xmltv(texto: &str) -> Option<i64> {
    let digitos: String = texto.chars().take(14).collect();
    if digitos.len() != 14 || !digitos.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let numero = |a: usize, b: usize| digitos[a..b].parse::<i64>().ok();
    let (ano, mes, dia) = (numero(0, 4)?, numero(4, 6)?, numero(6, 8)?);
    let (hora, minuto, segundo) = (numero(8, 10)?, numero(10, 12)?, numero(12, 14)?);

    let mut fuso = 0i64;
    let cauda = texto[14..].trim();
    if cauda.len() >= 5 {
        let sinal = if cauda.starts_with('-') { -1 } else { 1 };
        if let (Ok(hh), Ok(mm)) = (cauda[1..3].parse::<i64>(), cauda[3..5].parse::<i64>()) {
            fuso = sinal * (hh * 3600 + mm * 60);
        }
    }
    Some(epoch(ano, mes, dia, hora, minuto, segundo) - fuso)
}

/// Dias desde 1970 pela fórmula civil — sem depender de biblioteca de datas.
fn epoch(ano: i64, mes: i64, dia: i64, hora: i64, minuto: i64, segundo: i64) -> i64 {
    let a = if mes <= 2 { ano - 1 } else { ano };
    let era = if a >= 0 { a } else { a - 399 } / 400;
    let ano_da_era = a - era * 400;
    let dia_do_ano = (153 * (mes + if mes > 2 { -3 } else { 9 }) + 2) / 5 + dia - 1;
    let dia_da_era = ano_da_era * 365 + ano_da_era / 4 - ano_da_era / 100 + dia_do_ano;
    let dias = era * 146097 + dia_da_era - 719468;
    dias * 86400 + hora * 3600 + minuto * 60 + segundo
}

/// Hora de Brasília (−3, sem horário de verão desde 2019).
pub fn hora_local(instante: i64) -> String {
    let segundos_do_dia = (instante - 3 * 3600).rem_euclid(86400);
    format!("{:02}:{:02}", segundos_do_dia / 3600, (segundos_do_dia % 3600) / 60)
}

// MARK: - Disco

fn arquivo() -> std::path::PathBuf {
    crate::catalogo::pasta().join("guia.json")
}

fn gravar_no_disco(grade: &HashMap<String, Vec<Programa>>) {
    let canais: serde_json::Map<String, serde_json::Value> = grade
        .iter()
        .map(|(canal, lista)| {
            let programas: Vec<serde_json::Value> = lista
                .iter()
                .map(|p| json!([p.titulo, p.inicio, p.fim, p.descricao, p.categoria]))
                .collect();
            (canal.clone(), serde_json::Value::Array(programas))
        })
        .collect();
    let _ = std::fs::write(arquivo(), json!({ "at": agora(), "canais": canais }).to_string());
}

fn ler_do_disco() -> Option<(i64, HashMap<String, Vec<Programa>>)> {
    let texto = std::fs::read_to_string(arquivo()).ok()?;
    let dados: serde_json::Value = serde_json::from_str(&texto).ok()?;
    let quando = dados.get("at")?.as_i64()?;
    let corte = agora() - JANELA_PASSADA_S;
    let mut grade = HashMap::new();
    for (canal, lista) in dados.get("canais")?.as_object()? {
        let programas: Vec<Programa> = lista
            .as_array()?
            .iter()
            .filter_map(|p| {
                let p = p.as_array()?;
                let fim = p.get(2)?.as_i64()?;
                if fim <= corte {
                    return None;
                }
                Some(Programa {
                    titulo: p.first()?.as_str()?.to_string(),
                    inicio: p.get(1)?.as_i64()?,
                    fim,
                    descricao: p.get(3).and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    categoria: p.get(4).and_then(|v| v.as_str()).unwrap_or("").to_string(),
                })
            })
            .collect();
        if !programas.is_empty() {
            grade.insert(canal.clone(), programas);
        }
    }
    if grade.is_empty() {
        None
    } else {
        Some((quando, grade))
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Monta o guia com o catálogo publicado de verdade: é a única forma de
    /// saber se o casamento de nomes e a leitura em fluxo continuam de pé.
    #[test]
    #[ignore = "usa a rede"]
    fn monta_o_guia_do_catalogo_publicado() {
        let canais = crate::catalogo::baixar("catalogo.txt").expect("catálogo");
        let grade = montar(&canais);
        let programas: usize = grade.values().map(|l| l.len()).sum();
        println!("canais com guia: {} de {} | programas: {programas}", grade.len(), canais.len());
        let instante = agora();
        for nome in ["Band", "Globo", "Pluto TV Cine Sucessos", "A Feiticeira"] {
            let atual = grade
                .get(nome)
                .and_then(|l| l.iter().find(|p| p.no_ar(instante)))
                .map(|p| format!("{} ({})", p.titulo, p.horario()));
            println!("{nome}: {}", atual.unwrap_or_else(|| "sem guia".into()));
        }
        assert!(grade.len() > 100, "guia pequeno demais: {}", grade.len());
    }
}
