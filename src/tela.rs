//! O desenho, no molde do app do Mac.
//!
//! Mesma organização: barra lateral com a busca, o acervo em pílulas e os
//! canais por seção; o vídeo ocupando o resto, com a faixa do que está no ar e
//! a barra de controles em cápsula, que somem sozinhas; e o acervo abrindo em
//! grade de capas por cima do vídeo. As cores são as do modo escuro do macOS,
//! com o azul do sistema como destaque.
//!
//! Todo comando da barra tem o atalho na dica, e o painel de atalhos (H ou
//! F1) lista todos: nada depende de alguém adivinhar tecla.

use crate::{atualizacao, epg, progresso, vod, Aba, App};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use eframe::glow::HasContext;
use std::sync::Arc;
use std::time::Duration;

// Cores do modo escuro do macOS.
const LATERAL: Color32 = Color32::from_rgba_premultiplied(30, 30, 32, 250);
const PAINEL: Color32 = Color32::from_rgb(28, 28, 30);
const ELEVADO: Color32 = Color32::from_rgb(44, 44, 46);
const LINHA: Color32 = Color32::from_rgb(58, 58, 60);
const TEXTO: Color32 = Color32::from_rgb(242, 242, 247);
const SECUNDARIO: Color32 = Color32::from_rgb(152, 152, 157);
const TERCIARIO: Color32 = Color32::from_rgb(99, 99, 102);
const AZUL: Color32 = Color32::from_rgb(10, 132, 255);
const AMARELO: Color32 = Color32::from_rgb(255, 214, 10);
const VERMELHO: Color32 = Color32::from_rgb(255, 69, 58);
const VERDE: Color32 = Color32::from_rgb(48, 209, 88);
const VIDRO: Color32 = Color32::from_rgba_premultiplied(24, 24, 28, 225);

const LARGURA_LATERAL: f32 = 272.0;
const ALTURA_LINHA: f32 = 46.0;

/// Visual base e, no Windows, a fonte do sistema (Segoe UI) no lugar da
/// padrão do egui — é o que mais aproxima a cara de um app nativo.
pub fn aplicar_estilo(ctx: &egui::Context) {
    let mut fontes = egui::FontDefinitions::default();
    let candidatas = [
        ("sistema", "C:\\Windows\\Fonts\\segoeui.ttf"),
        ("sistema-forte", "C:\\Windows\\Fonts\\seguisb.ttf"),
    ];
    let mut achou = false;
    for (nome, caminho) in candidatas {
        if let Ok(bytes) = std::fs::read(caminho) {
            fontes.font_data.insert(nome.into(), egui::FontData::from_owned(bytes));
            achou = true;
        }
    }
    if achou {
        let familia = fontes.families.entry(egui::FontFamily::Proportional).or_default();
        familia.insert(0, "sistema".into());
        if fontes.font_data.contains_key("sistema-forte") {
            fontes
                .families
                .insert(egui::FontFamily::Name("forte".into()), vec!["sistema-forte".into(), "sistema".into()]);
        }
    }
    if !fontes.families.contains_key(&egui::FontFamily::Name("forte".into())) {
        let padrao = fontes.families[&egui::FontFamily::Proportional].clone();
        fontes.families.insert(egui::FontFamily::Name("forte".into()), padrao);
    }
    ctx.set_fonts(fontes);

    let mut estilo = (*ctx.style()).clone();
    let mut visual = egui::Visuals::dark();
    visual.panel_fill = PAINEL;
    visual.window_fill = PAINEL;
    visual.window_stroke = Stroke::new(1.0, LINHA);
    visual.window_rounding = 12.0.into();
    visual.extreme_bg_color = ELEVADO;
    visual.faint_bg_color = ELEVADO;
    visual.selection.bg_fill = AZUL;
    visual.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    visual.hyperlink_color = AZUL;
    visual.override_text_color = Some(TEXTO);
    for w in [
        &mut visual.widgets.inactive,
        &mut visual.widgets.hovered,
        &mut visual.widgets.active,
        &mut visual.widgets.open,
        &mut visual.widgets.noninteractive,
    ] {
        w.rounding = 6.0.into();
    }
    visual.widgets.inactive.weak_bg_fill = ELEVADO;
    visual.widgets.inactive.bg_fill = ELEVADO;
    visual.widgets.hovered.weak_bg_fill = Color32::from_rgb(58, 58, 62);
    visual.widgets.active.weak_bg_fill = AZUL;
    visual.slider_trailing_fill = true;
    estilo.visuals = visual;
    estilo.spacing.item_spacing = egui::vec2(8.0, 6.0);
    estilo.spacing.button_padding = egui::vec2(10.0, 5.0);
    estilo.spacing.slider_width = 90.0;
    ctx.set_style(estilo);
}

fn forte(tamanho: f32) -> FontId {
    FontId::new(tamanho, egui::FontFamily::Name("forte".into()))
}

fn normal(tamanho: f32) -> FontId {
    FontId::proportional(tamanho)
}

pub fn desenhar(app: &mut App, ctx: &egui::Context) {
    video(app, ctx);

    if app.lista_aberta {
        barra_lateral(app, ctx);
    }
    if app.guia_aberto && app.aba == Aba::Canais {
        guia(app, ctx);
    }
    if app.aba == Aba::Canais {
        palco(app, ctx);
    } else {
        acervo(app, ctx);
    }
    avisos(app, ctx);
    if app.ajuda_aberta {
        atalhos(app, ctx);
    }
    atualizacao_na_tela(app, ctx);
}

// MARK: - Ícones desenhados
//
// Desenhados com linhas e polígonos, e não com caracteres: a fonte do sistema
// nem sempre tem o glifo, e um quadradinho no lugar do "play" seria pior que
// nada.

#[derive(Clone, Copy)]
enum Icone {
    Tocar,
    Pausar,
    Anterior,
    Proximo,
    Som,
    Mudo,
    TelaCheia,
    Lista,
    Guia,
    Filmes,
    Series,
    Fontes,
    Ajuda,
    Estrela,
    Cadeado,
    Preencher,
    Voltar,
    Fechar,
}

fn pintar_icone(pintor: &egui::Painter, icone: Icone, centro: Pos2, tamanho: f32, cor: Color32) {
    let s = tamanho / 2.0;
    let traco = Stroke::new((tamanho / 11.0).max(1.4), cor);
    let p = |x: f32, y: f32| Pos2::new(centro.x + x * s, centro.y + y * s);
    match icone {
        Icone::Tocar => {
            pintor.add(egui::Shape::convex_polygon(vec![p(-0.55, -0.8), p(0.8, 0.0), p(-0.55, 0.8)], cor, Stroke::NONE));
        }
        Icone::Pausar => {
            pintor.rect_filled(Rect::from_min_max(p(-0.65, -0.8), p(-0.15, 0.8)), 1.5, cor);
            pintor.rect_filled(Rect::from_min_max(p(0.15, -0.8), p(0.65, 0.8)), 1.5, cor);
        }
        Icone::Anterior => {
            pintor.add(egui::Shape::convex_polygon(vec![p(0.7, -0.7), p(-0.35, 0.0), p(0.7, 0.7)], cor, Stroke::NONE));
            pintor.rect_filled(Rect::from_min_max(p(-0.75, -0.7), p(-0.45, 0.7)), 1.0, cor);
        }
        Icone::Proximo => {
            pintor.add(egui::Shape::convex_polygon(vec![p(-0.7, -0.7), p(0.35, 0.0), p(-0.7, 0.7)], cor, Stroke::NONE));
            pintor.rect_filled(Rect::from_min_max(p(0.45, -0.7), p(0.75, 0.7)), 1.0, cor);
        }
        Icone::Som | Icone::Mudo => {
            pintor.add(egui::Shape::convex_polygon(
                vec![p(-0.85, -0.3), p(-0.45, -0.3), p(0.05, -0.8), p(0.05, 0.8), p(-0.45, 0.3), p(-0.85, 0.3)],
                cor,
                Stroke::NONE,
            ));
            if matches!(icone, Icone::Som) {
                pintor.line_segment([p(0.35, -0.35), p(0.35, 0.35)], traco);
                pintor.line_segment([p(0.65, -0.6), p(0.65, 0.6)], traco);
            } else {
                pintor.line_segment([p(0.3, -0.35), p(0.85, 0.35)], traco);
                pintor.line_segment([p(0.3, 0.35), p(0.85, -0.35)], traco);
            }
        }
        Icone::TelaCheia => {
            for (a, b, c) in [
                ((-0.8, -0.3), (-0.8, -0.8), (-0.3, -0.8)),
                ((0.3, -0.8), (0.8, -0.8), (0.8, -0.3)),
                ((0.8, 0.3), (0.8, 0.8), (0.3, 0.8)),
                ((-0.3, 0.8), (-0.8, 0.8), (-0.8, 0.3)),
            ] {
                pintor.line_segment([p(a.0, a.1), p(b.0, b.1)], traco);
                pintor.line_segment([p(b.0, b.1), p(c.0, c.1)], traco);
            }
        }
        Icone::Preencher => {
            pintor.rect_stroke(Rect::from_min_max(p(-0.85, -0.6), p(0.85, 0.6)), 2.0, traco);
            pintor.line_segment([p(-0.45, 0.0), p(0.45, 0.0)], traco);
            pintor.add(egui::Shape::convex_polygon(vec![p(-0.55, 0.0), p(-0.3, -0.22), p(-0.3, 0.22)], cor, Stroke::NONE));
            pintor.add(egui::Shape::convex_polygon(vec![p(0.55, 0.0), p(0.3, -0.22), p(0.3, 0.22)], cor, Stroke::NONE));
        }
        Icone::Lista => {
            for y in [-0.55, 0.0, 0.55] {
                pintor.circle_filled(p(-0.7, y), tamanho * 0.07, cor);
                pintor.line_segment([p(-0.35, y), p(0.8, y)], traco);
            }
        }
        Icone::Guia => {
            pintor.rect_stroke(Rect::from_min_max(p(-0.8, -0.65), p(0.8, 0.8)), 2.0, traco);
            pintor.line_segment([p(-0.8, -0.25), p(0.8, -0.25)], traco);
            pintor.line_segment([p(-0.4, -0.9), p(-0.4, -0.5)], traco);
            pintor.line_segment([p(0.4, -0.9), p(0.4, -0.5)], traco);
            pintor.circle_filled(p(-0.35, 0.25), tamanho * 0.06, cor);
            pintor.circle_filled(p(0.1, 0.25), tamanho * 0.06, cor);
        }
        Icone::Filmes => {
            pintor.rect_stroke(Rect::from_min_max(p(-0.85, -0.65), p(0.85, 0.65)), 2.0, traco);
            for x in [-0.55, 0.55] {
                for y in [-0.35, 0.0, 0.35] {
                    pintor.circle_filled(p(x, y), tamanho * 0.05, cor);
                }
            }
        }
        Icone::Series => {
            pintor.rect_stroke(Rect::from_min_max(p(-0.85, -0.55), p(0.85, 0.6)), 2.0, traco);
            pintor.line_segment([p(-0.3, 0.85), p(0.3, 0.85)], traco);
            pintor.add(egui::Shape::convex_polygon(vec![p(-0.2, -0.28), p(0.3, 0.02), p(-0.2, 0.32)], cor, Stroke::NONE));
        }
        Icone::Fontes => {
            pintor.line_segment([p(-0.8, -0.4), p(0.7, -0.4)], traco);
            pintor.add(egui::Shape::convex_polygon(vec![p(0.85, -0.4), p(0.5, -0.7), p(0.5, -0.1)], cor, Stroke::NONE));
            pintor.line_segment([p(0.8, 0.4), p(-0.7, 0.4)], traco);
            pintor.add(egui::Shape::convex_polygon(vec![p(-0.85, 0.4), p(-0.5, 0.1), p(-0.5, 0.7)], cor, Stroke::NONE));
        }
        Icone::Ajuda => {
            pintor.circle_stroke(centro, s * 0.85, traco);
            pintor.text(centro, Align2::CENTER_CENTER, "?", forte(tamanho * 0.75), cor);
        }
        Icone::Estrela => {
            let pontos: Vec<Pos2> = (0..10)
                .map(|i| {
                    let raio = if i % 2 == 0 { 0.9 } else { 0.4 };
                    let angulo = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                    p(raio * angulo.cos(), raio * angulo.sin())
                })
                .collect();
            pintor.add(egui::Shape::closed_line(pontos, traco));
        }
        Icone::Cadeado => {
            pintor.rect_filled(Rect::from_min_max(p(-0.65, -0.1), p(0.65, 0.85)), 2.0, cor);
            pintor.add(egui::Shape::line(
                (0..=12)
                    .map(|i| {
                        let a = std::f32::consts::PI + i as f32 * std::f32::consts::PI / 12.0;
                        p(0.4 * a.cos(), -0.1 + 0.55 * a.sin())
                    })
                    .collect(),
                traco,
            ));
        }
        Icone::Voltar => {
            pintor.line_segment([p(0.4, -0.75), p(-0.35, 0.0)], traco);
            pintor.line_segment([p(-0.35, 0.0), p(0.4, 0.75)], traco);
        }
        Icone::Fechar => {
            pintor.line_segment([p(-0.6, -0.6), p(0.6, 0.6)], traco);
            pintor.line_segment([p(-0.6, 0.6), p(0.6, -0.6)], traco);
        }
    }
}

/// Botão só de ícone, com a dica (e o atalho) ao passar o mouse.
fn botao_icone(ui: &mut egui::Ui, icone: Icone, tamanho: f32, dica: &str) -> egui::Response {
    let (rect, resposta) = ui.allocate_exact_size(Vec2::splat(tamanho + 12.0), Sense::click());
    let cor = if resposta.hovered() { Color32::WHITE } else { Color32::from_gray(225) };
    if resposta.hovered() {
        ui.painter().rect_filled(rect, 7.0, Color32::from_white_alpha(22));
    }
    pintar_icone(ui.painter(), icone, rect.center(), tamanho, cor);
    resposta.on_hover_text(dica)
}

/// Botão com ícone e texto, como os "Guia" e "Filmes" da faixa do Mac.
fn botao_rotulo(ui: &mut egui::Ui, icone: Icone, texto: &str, dica: &str) -> egui::Response {
    let galeria = ui.painter().layout_no_wrap(texto.to_string(), forte(12.0), Color32::WHITE);
    let largura = 22.0 + galeria.size().x + 10.0;
    let (rect, resposta) = ui.allocate_exact_size(egui::vec2(largura, 26.0), Sense::click());
    let fundo = if resposta.hovered() { Color32::from_white_alpha(34) } else { Color32::from_white_alpha(16) };
    ui.painter().rect_filled(rect, 7.0, fundo);
    pintar_icone(ui.painter(), icone, Pos2::new(rect.left() + 13.0, rect.center().y), 12.0, Color32::WHITE);
    ui.painter().galley(Pos2::new(rect.left() + 24.0, rect.center().y - galeria.size().y / 2.0), galeria, Color32::WHITE);
    resposta.on_hover_text(dica)
}

/// Pílula selecionável (acervo, letras).
fn pilula(ui: &mut egui::Ui, icone: Option<Icone>, texto: &str, ativa: bool) -> egui::Response {
    let fonte = if ativa { forte(12.0) } else { normal(12.0) };
    let galeria = ui.painter().layout_no_wrap(texto.to_string(), fonte, TEXTO);
    let extra = if icone.is_some() { 18.0 } else { 0.0 };
    let tamanho = egui::vec2(galeria.size().x + 18.0 + extra, 24.0);
    let (rect, resposta) = ui.allocate_exact_size(tamanho, Sense::click());
    let fundo = if ativa {
        AZUL.gamma_multiply(0.45)
    } else if resposta.hovered() {
        Color32::from_white_alpha(24)
    } else {
        Color32::from_white_alpha(14)
    };
    ui.painter().rect_filled(rect, 6.0, fundo);
    let mut x = rect.left() + 9.0;
    if let Some(icone) = icone {
        pintar_icone(ui.painter(), icone, Pos2::new(x + 6.0, rect.center().y), 11.0, TEXTO);
        x += extra;
    }
    ui.painter().galley(Pos2::new(x, rect.center().y - galeria.size().y / 2.0), galeria, TEXTO);
    resposta
}

fn campo_de_busca(ui: &mut egui::Ui, texto: &mut String, dica: &str, id: &str) -> egui::Response {
    egui::Frame::none()
        .fill(ELEVADO)
        .rounding(7.0)
        .inner_margin(egui::Margin::symmetric(8.0, 5.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                ui.painter().circle_stroke(rect.center() - egui::vec2(1.5, 1.5), 4.5, Stroke::new(1.4, SECUNDARIO));
                ui.painter().line_segment(
                    [rect.center() + egui::vec2(2.0, 2.0), rect.center() + egui::vec2(5.5, 5.5)],
                    Stroke::new(1.6, SECUNDARIO),
                );
                ui.add(
                    egui::TextEdit::singleline(texto)
                        .id(egui::Id::new(id))
                        .hint_text(egui::RichText::new(dica).color(TERCIARIO))
                        .frame(false)
                        .desired_width(f32::INFINITY),
                )
            })
            .inner
        })
        .inner
}

// MARK: - Vídeo

/// O vídeo, no fundo de tudo.
///
/// Era aqui a tela preta do Windows. O vídeo morava numa `Area` de ordem
/// `Background`, na crença de que isso o deixaria embaixo de tudo — mas os
/// painéis (`SidePanel`, `CentralPanel`) desenham nessa mesma ordem, e entre
/// camadas de mesma ordem o egui põe por último a que foi criada por último.
/// A `Area` do vídeo nascia a cada quadro, então ia parar em cima: o retângulo
/// preto que ela pinta antes do quadro cobria a interface inteira.
///
/// Só acontecia onde o mpv carrega — sem ele esta função sai na primeira
/// linha, nada é pintado por cima e a tela aparece. Por isso o defeito parecia
/// vir do vídeo, e mexer na decodificação nunca resolveu.
///
/// A correção é pintar no próprio fundo dos painéis, antes deles. Dentro de
/// uma camada vale a ordem de inserção, e `video` é a primeira coisa que
/// `desenhar` chama — o quadro fica embaixo, e a interface por cima.
fn video(app: &mut App, ctx: &egui::Context) {
    let Some(mpv) = app.mpv.clone() else { return };
    let rect = ctx.screen_rect();
    let pintor = ctx.layer_painter(egui::LayerId::background());
    pintor.rect_filled(rect, 0.0, Color32::BLACK);
    pintor.add(egui::PaintCallback {
        rect,
        callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |info, pintor| {
            if let Err(erro) = mpv.ligar_video() {
                crate::telemetria::erro(erro);
                return;
            }
            let gl = pintor.gl();
            let fbo = unsafe { gl.get_parameter_i32(eframe::glow::DRAW_FRAMEBUFFER_BINDING) };
            let [largura, altura] = info.screen_size_px;
            unsafe { estado_padrao_de_opengl(gl) };
            mpv.desenhar(fbo, largura as i32, altura as i32);
        })),
    });
}

/// Devolve o OpenGL ao estado padrão antes de entregar o quadro ao mpv.
///
/// A documentação do mpv é explícita: `mpv_render_context_render` exige o
/// contexto "reasonably set to OpenGL standard defaults", e cita
/// `GL_SCISSOR_TEST` e `GL_BLEND` nos valores de fábrica — desligados.
///
/// O egui chama o callback com o oposto disso, porque é assim que ele desenha
/// a própria interface: teste de tesoura ligado, caixa de recorte posta e
/// mistura ligada. Medido aqui, o mpv recebia `SCISSOR_TEST=1 BLEND=1`. Ele
/// não desliga nada — confia no que a documentação pede — e desenha o quadro
/// em várias passagens, cada uma num alvo próprio, limpando-as com `glClear`,
/// que obedece à tesoura. O que sai daí depende da placa e do tamanho do
/// vídeo: imagem recortada, cor lavada pela mistura, ou nada.
///
/// Não era esta a causa da tela preta — essa era a camada, acima — mas é um
/// contrato quebrado que estragaria o quadro assim que ele voltasse a
/// aparecer.
///
/// Depois do callback o egui refaz o estado dele sozinho
/// (`prepare_painting`), e a caixa de recorte volta a cada primitiva, então
/// não há o que restaurar aqui.
unsafe fn estado_padrao_de_opengl(gl: &eframe::glow::Context) {
    gl.disable(eframe::glow::SCISSOR_TEST);
    gl.disable(eframe::glow::BLEND);
    gl.disable(eframe::glow::CULL_FACE);
    gl.disable(eframe::glow::DEPTH_TEST);
    gl.disable(eframe::glow::STENCIL_TEST);
    gl.color_mask(true, true, true, true);
    gl.depth_mask(true);
    gl.stencil_mask(0xFF);
    // O egui deixa o programa e o VAO dele ligados; o padrão é nenhum.
    gl.use_program(None);
    gl.bind_vertex_array(None);
    gl.bind_buffer(eframe::glow::ARRAY_BUFFER, None);
    gl.bind_buffer(eframe::glow::ELEMENT_ARRAY_BUFFER, None);
    gl.active_texture(eframe::glow::TEXTURE0);
    gl.bind_texture(eframe::glow::TEXTURE_2D, None);
}

// MARK: - Barra lateral

/// Uma linha da lista: título de seção ou canal. Altura única, para desenhar
/// só o que está à vista.
enum Item {
    Secao(String),
    Canal(usize),
}

fn barra_lateral(app: &mut App, ctx: &egui::Context) {
    egui::SidePanel::left("lateral")
        .exact_width(LARGURA_LATERAL)
        .resizable(false)
        .frame(
            egui::Frame::none()
                .fill(LATERAL)
                .stroke(Stroke::new(1.0, Color32::from_rgb(40, 40, 42)))
                .inner_margin(egui::Margin { left: 10.0, right: 10.0, top: 12.0, bottom: 8.0 }),
        )
        .show(ctx, |ui| {
            let busca = campo_de_busca(ui, &mut app.busca, "Buscar canal", "busca-canais");
            if busca.changed() {
                if let Some(primeiro) = app.visiveis().first() {
                    app.foco = *primeiro;
                }
            }

            // Acervo, como no Mac: pílulas lado a lado, quebrando linha.
            ui.add_space(10.0);
            ui.label(egui::RichText::new("Acervo").font(forte(11.0)).color(SECUNDARIO));
            ui.add_space(2.0);
            let mut secoes: Vec<(Aba, Icone, &str)> = vec![
                (Aba::Inicio, Icone::Preencher, "Início"),
                (Aba::Filmes, Icone::Filmes, "Filmes"),
                (Aba::Series, Icone::Series, "Séries"),
                (Aba::Animes, Icone::Series, "Animes"),
                (Aba::Doramas, Icone::Series, "Doramas"),
            ];
            if !app.favoritos_vod.is_empty() {
                secoes.push((Aba::Favoritos, Icone::Estrela, "Favoritos"));
            }
            if app.liberado {
                secoes.push((Aba::Extras, Icone::Cadeado, "Extras"));
            }
            let mut escolhida = None;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                for (aba, icone, nome) in &secoes {
                    if pilula(ui, Some(*icone), nome, app.aba == *aba).clicked() {
                        escolhida = Some(if app.aba == *aba { Aba::Canais } else { *aba });
                    }
                }
            });
            if let Some(aba) = escolhida {
                app.abrir_secao(aba);
            }
            ui.add_space(8.0);
            ui.painter().hline(ui.max_rect().x_range(), ui.cursor().top(), Stroke::new(1.0, LINHA));
            ui.add_space(4.0);

            let visiveis = app.visiveis();
            let mut itens = Vec::with_capacity(visiveis.len() + 12);
            let mut secao_atual = String::new();
            for indice in &visiveis {
                let canal = &app.canais[*indice];
                let secao = if app.favoritos.iter().any(|f| *f == canal.nome) {
                    "Favoritos".to_string()
                } else {
                    canal.secao().to_string()
                };
                if secao != secao_atual {
                    itens.push(Item::Secao(secao.clone()));
                    secao_atual = secao;
                }
                itens.push(Item::Canal(*indice));
            }

            let tocando = app.tocando.as_ref().map(|t| t.canal);
            let foco = app.foco;
            let mut clicado = None;
            let mut favoritar = None;
            let rolar_para_foco = ctx.input(|i| {
                i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::ArrowUp)
            });

            let altura_rodape = 26.0;
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(ui.available_height() - altura_rodape)
                .show_rows(ui, ALTURA_LINHA, itens.len(), |ui, faixa| {
                    for item in &itens[faixa] {
                        match item {
                            Item::Secao(nome) => {
                                let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ALTURA_LINHA), Sense::hover());
                                ui.painter().text(
                                    Pos2::new(rect.left() + 6.0, rect.bottom() - 8.0),
                                    Align2::LEFT_BOTTOM,
                                    nome,
                                    forte(11.0),
                                    SECUNDARIO,
                                );
                            }
                            Item::Canal(indice) => {
                                let (nome, logo, fontes) = {
                                    let c = &app.canais[*indice];
                                    (c.nome.clone(), c.logo.clone(), c.fontes.len())
                                };
                                let favorito = app.favoritos.iter().any(|f| *f == nome);
                                let textura = logo.as_ref().and_then(|url| app.logo(url));
                                let no_ar = epg::agora_e_depois(&nome).map(|(p, _)| p);
                                let (resposta, estrela) = linha_do_canal(
                                    ui, &nome, fontes, textura, favorito,
                                    Some(*indice) == tocando, *indice == foco, no_ar.as_ref(),
                                );
                                if estrela {
                                    favoritar = Some(nome.clone());
                                } else if resposta.clicked() {
                                    clicado = Some(*indice);
                                }
                                if resposta.secondary_clicked() {
                                    favoritar = Some(nome.clone());
                                }
                                if *indice == foco && rolar_para_foco {
                                    resposta.scroll_to_me(Some(egui::Align::Center));
                                }
                            }
                        }
                    }
                });

            if let Some(canal) = favoritar {
                app.favoritar(canal);
            }
            if let Some(indice) = clicado {
                app.foco = indice;
                app.tocar(indice, 0, true);
                app.abrir_secao(Aba::Canais);
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{} canais", visiveis.len()))
                            .font(normal(11.0))
                            .color(TERCIARIO),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Label::new(egui::RichText::new("Atalhos (H)").font(normal(11.0)).color(AZUL)).sense(Sense::click()))
                            .on_hover_text("Todos os comandos do teclado")
                            .clicked()
                        {
                            app.ajuda_aberta = true;
                        }
                    });
                });
            });
        });
}

/// Linha do canal: logo, nome, o que está no ar e a barrinha de andamento.
/// Devolve também se o clique foi na estrela.
#[allow(clippy::too_many_arguments)]
fn linha_do_canal(
    ui: &mut egui::Ui,
    nome: &str,
    fontes: usize,
    logo: Option<egui::TextureHandle>,
    favorito: bool,
    tocando: bool,
    focado: bool,
    no_ar: Option<&epg::Programa>,
) -> (egui::Response, bool) {
    let (rect, resposta) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ALTURA_LINHA), Sense::click());
    let pintor = ui.painter().clone();
    let caixa = rect.shrink2(egui::vec2(0.0, 2.0));
    if tocando {
        pintor.rect_filled(caixa, 7.0, AZUL);
    } else if focado {
        pintor.rect_filled(caixa, 7.0, Color32::from_white_alpha(28));
    } else if resposta.hovered() {
        pintor.rect_filled(caixa, 7.0, Color32::from_white_alpha(12));
    }

    // Logo em ladrilho arredondado, como no Mac.
    let lado = 30.0;
    let ladrilho = Rect::from_min_size(Pos2::new(caixa.left() + 6.0, caixa.center().y - lado / 2.0), Vec2::splat(lado));
    pintor.rect_filled(ladrilho, 8.0, Color32::from_white_alpha(20));
    match logo {
        Some(textura) => {
            egui::Image::new(&textura)
                .maintain_aspect_ratio(true)
                .fit_to_exact_size(Vec2::splat(lado - 4.0))
                .paint_at(ui, ladrilho.shrink(2.0));
        }
        None => {
            let iniciais: String = nome.split_whitespace().take(2).filter_map(|p| p.chars().next()).collect::<String>().to_uppercase();
            pintor.text(ladrilho.center(), Align2::CENTER_CENTER, iniciais, forte(11.0), TEXTO);
        }
    }

    let x = ladrilho.right() + 10.0;
    let secundaria = if tocando { Color32::from_white_alpha(200) } else { SECUNDARIO };
    let direita = caixa.right() - 26.0;
    let largura_texto = (direita - x).max(20.0);
    let cortar = |texto: &str, fonte: FontId| -> std::sync::Arc<egui::Galley> {
        let mut job = egui::text::LayoutJob::simple_singleline(texto.to_string(), fonte, TEXTO);
        job.wrap = egui::text::TextWrapping::truncate_at_width(largura_texto);
        ui.fonts(|f| f.layout_job(job))
    };
    if let Some(programa) = no_ar {
        let titulo = cortar(nome, forte(13.0));
        pintor.galley(Pos2::new(x, caixa.top() + 4.0), titulo, TEXTO);
        let subtitulo = cortar(&programa.titulo, normal(10.5));
        pintor.galley(Pos2::new(x, caixa.top() + 21.0), subtitulo, secundaria);
        let trilho = Rect::from_min_size(Pos2::new(x, caixa.bottom() - 6.0), egui::vec2(largura_texto, 2.0));
        pintor.rect_filled(trilho, 1.0, Color32::from_white_alpha(35));
        let feito = Rect::from_min_size(trilho.min, egui::vec2(largura_texto * programa.andamento(epg::agora()), 2.0));
        pintor.rect_filled(feito, 1.0, if tocando { Color32::WHITE } else { AZUL });
    } else {
        let titulo = cortar(nome, forte(13.0));
        pintor.galley(Pos2::new(x, caixa.center().y - titulo.size().y / 2.0), titulo, TEXTO);
        let _ = fontes;
    }

    // Estrela: sempre nos favoritos, e ao passar o mouse nos outros.
    let mut na_estrela = false;
    if favorito || resposta.hovered() {
        let centro = Pos2::new(caixa.right() - 13.0, caixa.center().y);
        let alvo = Rect::from_center_size(centro, Vec2::splat(20.0));
        let sobre = ui.rect_contains_pointer(alvo);
        if favorito {
            desenhar_estrela_cheia(&pintor, centro, 12.0, if tocando { Color32::WHITE } else { AMARELO });
        } else {
            pintar_icone(&pintor, Icone::Estrela, centro, 12.0, if sobre { TEXTO } else { SECUNDARIO });
        }
        na_estrela = sobre && resposta.clicked();
    }
    let dica = if favorito { "Clique na estrela ou S para tirar dos favoritos" } else { "Enter assiste · S favorita" };
    (resposta.on_hover_text(dica), na_estrela)
}

fn desenhar_estrela_cheia(pintor: &egui::Painter, centro: Pos2, tamanho: f32, cor: Color32) {
    let s = tamanho / 2.0;
    // Um polígono côncavo não é aceito como preenchido: a estrela vira cinco
    // triângulos em volta de um pentágono.
    let ponto = |raio: f32, i: usize| {
        let angulo = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
        Pos2::new(centro.x + raio * s * angulo.cos(), centro.y + raio * s * angulo.sin())
    };
    let miolo: Vec<Pos2> = (0..5).map(|k| ponto(0.4, 2 * k + 1)).collect();
    pintor.add(egui::Shape::convex_polygon(miolo, cor, Stroke::NONE));
    for k in 0..5 {
        pintor.add(egui::Shape::convex_polygon(
            vec![ponto(0.4, (2 * k + 9) % 10), ponto(0.95, 2 * k), ponto(0.4, 2 * k + 1)],
            cor,
            Stroke::NONE,
        ));
    }
}

// MARK: - Palco

fn palco(app: &mut App, ctx: &egui::Context) {
    egui::CentralPanel::default()
        .frame(egui::Frame::none())
        .show(ctx, |ui| {
            let area = ui.max_rect();

            // SAIMO_DEMO: só para conferir a tela sem o mpv (fora do Windows).
            let demo = std::env::var("SAIMO_DEMO").is_ok();
            if demo && app.tocando.is_none() && !app.canais.is_empty() {
                app.tocando = Some(crate::Tocando { canal: 1, fonte: 0, desde: std::time::Instant::now(), confirmado: true });
            }
            if app.mpv.is_none() && !demo {
                // Aberto de dentro do ZIP é o caso comum, e tem saída simples:
                // dizer isso é mais útil que anunciar que falta um arquivo que
                // a pessoa nem sabia que existia.
                let do_zip = crate::aberto_de_dentro_do_zip();
                let (titulo, recado) = if do_zip {
                    (
                        "Descompacte a pasta antes de abrir",
                        "O Windows copiou só o programa para uma pasta temporária e deixou a \
                         libmpv-2.dll para trás. Clique com o botão direito no arquivo \
                         SaimoTV-Windows.zip, escolha \"Extrair tudo\" e abra o Saimo TV.exe \
                         de dentro da pasta extraída.",
                    )
                } else {
                    (
                        "Falta o mpv",
                        "O arquivo libmpv-2.dll precisa estar na mesma pasta do Saimo TV.exe.",
                    )
                };

                ui.painter().text(
                    area.center() - egui::vec2(0.0, 44.0),
                    Align2::CENTER_CENTER,
                    titulo,
                    forte(20.0),
                    TEXTO,
                );
                let mut job = egui::text::LayoutJob::simple(recado.to_string(), normal(13.0), SECUNDARIO, 560.0);
                job.halign = egui::Align::Center;
                let galeria = ui.fonts(|f| f.layout_job(job));
                ui.painter().galley(area.center() - egui::vec2(0.0, 18.0), galeria, SECUNDARIO);

                let pasta = crate::pasta_do_programa();
                let detalhe = if do_zip && !pasta.is_empty() {
                    format!("abriu daqui: {pasta}")
                } else {
                    app.erro_do_mpv.clone().unwrap_or_default()
                };
                if !detalhe.is_empty() {
                    let mut job = egui::text::LayoutJob::simple(detalhe, normal(11.0), TERCIARIO, 560.0);
                    job.halign = egui::Align::Center;
                    let galeria = ui.fonts(|f| f.layout_job(job));
                    ui.painter().galley(area.center() + egui::vec2(0.0, 46.0), galeria, TERCIARIO);
                }
                return;
            }

            if let Some(erro) = app.mpv.as_ref().and_then(|mpv| mpv.erro_de_video()) {
                ui.painter().text(
                    area.center() - egui::vec2(0.0, 12.0),
                    Align2::CENTER_CENTER,
                    "Não foi possível iniciar o vídeo",
                    forte(20.0),
                    TEXTO,
                );
                ui.painter().text(
                    area.center() + egui::vec2(0.0, 18.0),
                    Align2::CENTER_CENTER,
                    erro,
                    normal(12.0),
                    SECUNDARIO,
                );
                return;
            }

            if app.tocando.is_none() && app.tocando_vod.is_none() {
                estado_vazio(app, ui, area);
                return;
            }

            if !app.controles_visiveis() {
                return;
            }

            // Botão de atalhos no canto, como lembrete permanente.
            let canto = Rect::from_min_size(Pos2::new(area.right() - 118.0, area.top() + 12.0), egui::vec2(106.0, 28.0));
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(canto), |ui| {
                egui::Frame::none().fill(VIDRO).rounding(8.0).inner_margin(egui::Margin::symmetric(4.0, 1.0)).show(ui, |ui| {
                    if botao_rotulo(ui, Icone::Ajuda, "Atalhos", "Todos os comandos (H ou F1)").clicked() {
                        app.ajuda_aberta = true;
                    }
                });
            });

            // Posições fixas, de baixo para cima: a barra e, acima dela, a
            // faixa do que está no ar. Empilhadas pelo layout, uma cobria a outra.
            let largura = (area.width() - 32.0).min(980.0);
            let esquerda = area.center().x - largura / 2.0;
            let barra = Rect::from_min_size(Pos2::new(esquerda, area.bottom() - 16.0 - 48.0), egui::vec2(largura, 48.0));
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(barra), |ui| {
                barra_de_controles(app, ui);
            });
            // A faixa tem altura variável (uma a três linhas): desenhada numa
            // camada própria, ancorada logo acima da barra.
            let de_baixo = ui.ctx().screen_rect().bottom() - (barra.top() - 10.0);
            egui::Area::new(egui::Id::new("faixa-no-ar"))
                .fade_in(false)
                .order(egui::Order::Middle)
                .anchor(Align2::LEFT_BOTTOM, egui::vec2(esquerda, -de_baixo))
                .show(ui.ctx(), |ui| {
                    ui.set_width(largura);
                    faixa_no_ar(app, ui);
                });
        });
}

fn estado_vazio(app: &mut App, ui: &mut egui::Ui, area: Rect) {
    let centro = area.center();
    pintar_icone(ui.painter(), Icone::Series, centro - egui::vec2(0.0, 60.0), 46.0, TERCIARIO);
    ui.painter().text(centro, Align2::CENTER_CENTER, "Escolha um canal na lista", forte(18.0), TEXTO);
    ui.painter().text(
        centro + egui::vec2(0.0, 24.0),
        Align2::CENTER_CENTER,
        "Setas andam · Enter assiste · Tab abre filmes e séries · H mostra todos os atalhos",
        normal(12.5),
        SECUNDARIO,
    );
    let botoes = Rect::from_center_size(centro + egui::vec2(0.0, 66.0), egui::vec2(330.0, 30.0));
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(botoes), |ui| {
        ui.horizontal_centered(|ui| {
            if botao_rotulo(ui, Icone::Lista, "Canais", "Mostrar a lista (L)").clicked() {
                app.lista_aberta = true;
            }
            if botao_rotulo(ui, Icone::Filmes, "Filmes", "Filmes e séries (Tab)").clicked() {
                app.abrir_secao(Aba::Filmes);
            }
            if botao_rotulo(ui, Icone::Ajuda, "Atalhos", "Todos os comandos (H)").clicked() {
                app.ajuda_aberta = true;
            }
        });
    });
}

/// O que está no ar e o que vem depois — ou, num filme, capa e quanto falta.
fn faixa_no_ar(app: &mut App, ui: &mut egui::Ui) {
    if let Some(filme) = app.tocando_vod.as_ref() {
        let titulo = filme.titulo.clone();
        let (nome, detalhe) = match titulo.split_once(" · ") {
            Some((serie, episodio)) => (serie.to_string(), episodio.to_string()),
            None => (titulo.clone(), String::new()),
        };
        let serie = !detalhe.is_empty();
        let capa = app.capa(&nome, serie);
        let restante = app.mpv.as_ref().and_then(|m| m.posicao()).map(|(p, d)| {
            let falta = ((d - p).max(0.0) / 60.0) as i64;
            if falta > 0 { format!("faltam {falta} min de {}", (d / 60.0) as i64) } else { "terminando".into() }
        });
        vidro(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(40.0, 58.0), Sense::hover());
                ui.painter().rect_filled(rect, 5.0, Color32::from_white_alpha(25));
                if let Some(textura) = capa {
                    egui::Image::new(&textura).fit_to_exact_size(rect.size()).rounding(5.0).paint_at(ui, rect);
                } else {
                    pintar_icone(ui.painter(), Icone::Filmes, rect.center(), 18.0, SECUNDARIO);
                }
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(&nome).font(forte(13.0)));
                    if !detalhe.is_empty() {
                        ui.label(egui::RichText::new(&detalhe).font(normal(11.0)).color(SECUNDARIO));
                    }
                    if let Some(r) = &restante {
                        ui.label(egui::RichText::new(r).font(normal(11.0)).color(SECUNDARIO));
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if botao_rotulo(ui, Icone::Filmes, "Filmes", "Voltar à lista de filmes e séries (Tab)").clicked() {
                        app.abrir_secao(if serie { Aba::Series } else { Aba::Filmes });
                    }
                });
            });
        });
        return;
    }

    let Some(canal) = app.tocando.as_ref().and_then(|t| app.canais.get(t.canal)).map(|c| c.nome.clone()) else { return };
    let Some((atual, depois)) = epg::agora_e_depois(&canal) else { return };
    let instante = epg::agora();
    let resposta = vidro(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&atual.titulo).font(forte(13.0)));
                    ui.label(
                        egui::RichText::new(format!("faltam {} min", ((atual.fim - instante).max(0) / 60)))
                            .font(normal(11.0))
                            .color(SECUNDARIO),
                    );
                });
                let detalhe = [atual.categoria.as_str(), &atual.horario()]
                    .iter()
                    .filter(|t| !t.is_empty())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" · ");
                ui.label(egui::RichText::new(detalhe).font(normal(11.0)).color(SECUNDARIO));
                if let Some(proximo) = &depois {
                    ui.label(
                        egui::RichText::new(format!("a seguir · {} ({})", proximo.titulo, epg::hora_local(proximo.inicio)))
                            .font(normal(11.0))
                            .color(SECUNDARIO),
                    );
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if botao_rotulo(ui, Icone::Filmes, "Filmes", "Filmes e séries (Tab)").clicked() {
                    app.abrir_secao(Aba::Filmes);
                }
                if botao_rotulo(ui, Icone::Guia, "Guia", "Programação completa (G)").clicked() {
                    app.guia_aberto = !app.guia_aberto;
                }
            });
        });
    });
    // A linha vermelha do andamento, embaixo da faixa, como no Mac.
    let r = resposta.rect;
    let feito = Rect::from_min_size(Pos2::new(r.left() + 6.0, r.bottom() - 3.0), egui::vec2((r.width() - 12.0) * atual.andamento(instante), 2.0));
    ui.painter().rect_filled(feito, 1.0, VERMELHO);
}

fn vidro<R>(ui: &mut egui::Ui, conteudo: impl FnOnce(&mut egui::Ui) -> R) -> egui::Response {
    egui::Frame::none()
        .fill(VIDRO)
        .rounding(12.0)
        .stroke(Stroke::new(1.0, Color32::from_white_alpha(18)))
        .shadow(egui::epaint::Shadow { offset: egui::vec2(0.0, 4.0), blur: 14.0, spread: 0.0, color: Color32::from_black_alpha(90) })
        .inner_margin(egui::Margin::symmetric(16.0, 9.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), conteudo).inner
        })
        .response
}

/// A cápsula de controles do Mac: tocar, canais vizinhos, ao vivo, qualidade,
/// fonte, avanço do filme, volume e o resto à direita.
fn barra_de_controles(app: &mut App, ui: &mut egui::Ui) {
    let filme = app.tocando_vod.is_some();
    egui::Frame::none()
        .fill(VIDRO)
        .rounding(26.0)
        .stroke(Stroke::new(1.0, Color32::from_white_alpha(18)))
        .shadow(egui::epaint::Shadow { offset: egui::vec2(0.0, 4.0), blur: 16.0, spread: 0.0, color: Color32::from_black_alpha(110) })
        .inner_margin(egui::Margin::symmetric(16.0, 6.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let (icone, dica) = if app.pausado { (Icone::Tocar, "Tocar (Espaço)") } else { (Icone::Pausar, "Pausar (Espaço)") };
                if botao_icone(ui, icone, 17.0, dica).clicked() {
                    app.alternar_pausa();
                }
                if !filme {
                    if botao_icone(ui, Icone::Anterior, 14.0, "Canal anterior (Page Up)").clicked() {
                        app.pular_canal(-1);
                    }
                    if botao_icone(ui, Icone::Proximo, 14.0, "Próximo canal (Page Down)").clicked() {
                        app.pular_canal(1);
                    }
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(66.0, 20.0), Sense::hover());
                    ui.painter().circle_filled(Pos2::new(rect.left() + 6.0, rect.center().y), 3.5, VERMELHO);
                    ui.painter().text(Pos2::new(rect.left() + 14.0, rect.center().y), Align2::LEFT_CENTER, "AO VIVO", forte(10.5), TEXTO);
                } else if botao_icone(ui, Icone::Anterior, 14.0, "Voltar 10 s (seta esquerda)").clicked() {
                    app.avancar(-10.0);
                }
                if filme && botao_icone(ui, Icone::Proximo, 14.0, "Avançar 10 s (seta direita)").clicked() {
                    app.avancar(10.0);
                }

                if let Some(altura) = app.mpv.as_ref().and_then(|m| m.ler("video-params/h")) {
                    selo(ui, &format!("{altura}p"), None, "Resolução do vídeo");
                }
                selo_da_fonte(app, ui);

                if filme {
                    barra_de_avanco(app, ui);
                }

                // Volume.
                let icone = if app.mudo || app.volume == 0 { Icone::Mudo } else { Icone::Som };
                if botao_icone(ui, icone, 15.0, "Sem som (M) · volume com + e −").clicked() {
                    app.alternar_mudo();
                }
                // Com o guia aberto a barra fica estreita: o volume passa a ser
                // só pelo botão e pelas teclas + e −.
                let larga = ui.available_width() > 430.0;
                if larga {
                    let mut volume = app.volume as f32;
                    let deslizou = ui.add(egui::Slider::new(&mut volume, 0.0..=130.0).show_value(false));
                    if deslizou.changed() {
                        app.definir_volume(volume as i32);
                    }
                    deslizou.on_hover_text(format!("Volume {}%", app.volume));
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if botao_icone(ui, Icone::TelaCheia, 14.0, "Tela cheia (F)").clicked() {
                        app.alternar_tela_cheia(ui.ctx());
                    }
                    if botao_icone(ui, Icone::Ajuda, 15.0, "Atalhos do teclado (H)").clicked() {
                        app.ajuda_aberta = true;
                    }
                    if botao_icone(ui, Icone::Lista, 14.0, "Mostrar ou esconder a lista (L)").clicked() {
                        app.lista_aberta = !app.lista_aberta;
                    }
                    if !filme && botao_icone(ui, Icone::Guia, 14.0, "Guia de programação (G)").clicked() {
                        app.guia_aberto = !app.guia_aberto;
                    }
                    let dica = if app.preencher { "Ajustar à tela" } else { "Preencher a tela" };
                    if larga && botao_icone(ui, Icone::Preencher, 15.0, dica).clicked() {
                        app.alternar_preencher();
                    }
                    if filme {
                        menu_de_velocidade(app, ui);
                    }
                    menu_de_faixas(app, ui);
                    menu_de_fontes(app, ui);
                });
            });
        });
}

fn menu_de_faixas(app: &mut App, ui: &mut egui::Ui) {
    let Some(mpv) = app.mpv.clone() else { return };
    let videos = mpv.faixas("video");
    let audios = mpv.faixas("audio");
    let legendas = mpv.faixas("sub");
    if videos.is_empty() && audios.is_empty() && legendas.is_empty() { return; }

    ui.menu_button(
        egui::RichText::new("A/V").font(forte(11.0)),
        |ui| {
            ui.set_min_width(190.0);
            ui.label(egui::RichText::new("Qualidade").font(forte(11.0)).color(SECUNDARIO));
            if ui.selectable_label(!videos.iter().any(|f| f.selecionada), "Automática").clicked() {
                mpv.escolher_faixa("video", None); ui.close_menu();
            }
            for faixa in &videos {
                if ui.selectable_label(faixa.selecionada, &faixa.titulo).clicked() {
                    mpv.escolher_faixa("video", Some(&faixa.id)); ui.close_menu();
                }
            }

            if !audios.is_empty() {
                ui.separator();
                ui.label(egui::RichText::new("Idioma do áudio").font(forte(11.0)).color(SECUNDARIO));
                for faixa in &audios {
                    if ui.selectable_label(faixa.selecionada, &faixa.titulo).clicked() {
                        mpv.escolher_faixa("audio", Some(&faixa.id)); ui.close_menu();
                    }
                }
            }

            if !legendas.is_empty() {
                ui.separator();
                ui.label(egui::RichText::new("Legendas").font(forte(11.0)).color(SECUNDARIO));
                let ligada = legendas.iter().any(|f| f.selecionada);
                if ui.selectable_label(!ligada, "Desligadas").clicked() {
                    mpv.escolher_faixa("sub", None); ui.close_menu();
                }
                for faixa in &legendas {
                    if ui.selectable_label(faixa.selecionada, &faixa.titulo).clicked() {
                        mpv.escolher_faixa("sub", Some(&faixa.id)); ui.close_menu();
                    }
                }
            }
        },
    ).response.on_hover_text("Qualidade, idioma do áudio e legendas");
}

fn selo(ui: &mut egui::Ui, texto: &str, ponto: Option<Color32>, dica: &str) {
    let galeria = ui.painter().layout_no_wrap(texto.to_string(), forte(10.5), TEXTO);
    let extra = if ponto.is_some() { 12.0 } else { 0.0 };
    let (rect, resposta) = ui.allocate_exact_size(egui::vec2(galeria.size().x + 12.0 + extra, 20.0), Sense::hover());
    ui.painter().rect_filled(rect, 5.0, Color32::from_white_alpha(38));
    if let Some(cor) = ponto {
        ui.painter().circle_filled(Pos2::new(rect.left() + 9.0, rect.center().y), 3.0, cor);
    }
    ui.painter().galley(Pos2::new(rect.left() + 6.0 + extra, rect.center().y - galeria.size().y / 2.0), galeria, TEXTO);
    resposta.on_hover_text(dica);
}

fn selo_da_fonte(app: &App, ui: &mut egui::Ui) {
    let (atual, total, url, pronto) = if let Some(t) = app.tocando.as_ref() {
        let Some(canal) = app.canais.get(t.canal) else { return };
        (t.fonte, canal.fontes.len(), canal.fontes.get(t.fonte).map(|f| f.url.clone()).unwrap_or_default(), t.confirmado)
    } else if let Some(f) = app.tocando_vod.as_ref() {
        (f.fonte, f.urls.len(), f.urls.get(f.fonte).cloned().unwrap_or_default(), f.confirmado)
    } else {
        return;
    };
    let host = url.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or("").to_string();
    let texto = if pronto {
        format!("fonte {}/{} · {host}", atual + 1, total)
    } else {
        format!("carregando fonte {}/{} · {host}", atual + 1, total)
    };
    let estado = if pronto { "No ar" } else { "Carregando" };
    selo(ui, &texto, Some(if pronto { VERDE } else { AMARELO }), &format!("{estado} pela fonte {} de {total}\n{url}\nSeta direita troca a fonte", atual + 1));
}

fn menu_de_fontes(app: &mut App, ui: &mut egui::Ui) {
    let total = if let Some(t) = app.tocando.as_ref() {
        app.canais.get(t.canal).map(|c| c.fontes.len()).unwrap_or(0)
    } else {
        app.tocando_vod.as_ref().map(|f| f.urls.len()).unwrap_or(0)
    };
    if total < 2 {
        return;
    }
    let atual = app.tocando.as_ref().map(|t| t.fonte).or_else(|| app.tocando_vod.as_ref().map(|f| f.fonte)).unwrap_or(0);
    let resposta = botao_icone(ui, Icone::Fontes, 15.0, "Escolher a fonte (seta direita passa para a próxima)");
    let id = ui.make_persistent_id("menu-fontes");
    if resposta.clicked() {
        ui.memory_mut(|m| m.toggle_popup(id));
    }
    egui::popup_above_or_below_widget(ui, id, &resposta, egui::AboveOrBelow::Above, egui::PopupCloseBehavior::CloseOnClick, |ui| {
        ui.set_min_width(160.0);
        ui.label(egui::RichText::new("Fontes").font(forte(11.0)).color(SECUNDARIO));
        for i in 0..total {
            let rotulo = if i == atual { format!("✓ Fonte {}", i + 1) } else { format!("   Fonte {}", i + 1) };
            if ui.selectable_label(i == atual, rotulo).clicked() {
                app.escolher_fonte(i);
            }
        }
    });
}

fn menu_de_velocidade(app: &mut App, ui: &mut egui::Ui) {
    let resposta = ui
        .add(egui::Button::new(egui::RichText::new(format!("{}×", app.velocidade)).font(forte(11.0))).frame(false))
        .on_hover_text("Velocidade");
    let id = ui.make_persistent_id("menu-velocidade");
    if resposta.clicked() {
        ui.memory_mut(|m| m.toggle_popup(id));
    }
    egui::popup_above_or_below_widget(ui, id, &resposta, egui::AboveOrBelow::Above, egui::PopupCloseBehavior::CloseOnClick, |ui| {
        ui.set_min_width(90.0);
        for v in [0.5, 0.75, 1.0, 1.25, 1.5, 2.0] {
            if ui.selectable_label(app.velocidade == v, format!("{v}×")).clicked() {
                app.definir_velocidade(v);
            }
        }
    });
}

fn barra_de_avanco(app: &mut App, ui: &mut egui::Ui) {
    let Some((posicao, duracao)) = app.mpv.as_ref().and_then(|m| m.posicao()) else { return };
    ui.label(egui::RichText::new(relogio(posicao)).font(FontId::monospace(10.5)).color(SECUNDARIO));
    let mut valor = posicao as f32;
    let resposta = ui.add_sized(
        egui::vec2(200.0, 18.0),
        egui::Slider::new(&mut valor, 0.0..=duracao as f32).show_value(false),
    );
    if resposta.drag_stopped() || (resposta.changed() && !resposta.dragged()) {
        app.ir_para(valor as f64);
    }
    resposta.on_hover_text("Arraste para avançar ou voltar (as setas pulam 10 s)");
    ui.label(egui::RichText::new(relogio(duracao)).font(FontId::monospace(10.5)).color(TERCIARIO));
}

fn relogio(segundos: f64) -> String {
    let total = segundos.max(0.0) as i64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m:02}:{s:02}") }
}

// MARK: - Guia

fn guia(app: &mut App, ctx: &egui::Context) {
    let canal = app.tocando.as_ref().map(|t| t.canal).unwrap_or(app.foco);
    let Some(nome) = app.canais.get(canal).map(|c| c.nome.clone()) else { return };
    let programas = epg::grade(&nome);
    let instante = epg::agora();

    egui::SidePanel::right("guia")
        .exact_width(340.0)
        .resizable(false)
        .frame(egui::Frame::none().fill(LATERAL).inner_margin(egui::Margin::symmetric(14.0, 12.0)))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                pintar_icone(ui.painter(), Icone::Guia, ui.cursor().left_center() + egui::vec2(8.0, 0.0), 14.0, AZUL);
                ui.add_space(20.0);
                ui.label(egui::RichText::new("Programação").font(forte(16.0)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if botao_icone(ui, Icone::Fechar, 11.0, "Fechar o guia (G)").clicked() {
                        app.guia_aberto = false;
                    }
                });
            });
            ui.label(egui::RichText::new(&nome).font(normal(12.0)).color(SECUNDARIO));
            ui.add_space(8.0);

            if programas.is_empty() {
                ui.add_space(20.0);
                ui.label(
                    egui::RichText::new(if epg::canais_com_guia() == 0 {
                        "Montando o guia…"
                    } else {
                        "Este canal não tem programação publicada."
                    })
                    .color(SECUNDARIO),
                );
                return;
            }

            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for programa in programas.iter().filter(|p| p.fim > instante) {
                    let ao_vivo = programa.no_ar(instante);
                    let fundo = if ao_vivo { AZUL.gamma_multiply(0.22) } else { Color32::from_white_alpha(10) };
                    let borda = if ao_vivo { Stroke::new(1.0, AZUL.gamma_multiply(0.55)) } else { Stroke::NONE };
                    egui::Frame::none().fill(fundo).stroke(borda).rounding(8.0).inner_margin(egui::Margin::symmetric(10.0, 7.0)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(epg::hora_local(programa.inicio)).font(FontId::monospace(11.5)).color(if ao_vivo { AZUL } else { SECUNDARIO }));
                            if ao_vivo {
                                ui.label(egui::RichText::new("AGORA").font(forte(9.5)).color(VERMELHO));
                            }
                        });
                        ui.label(egui::RichText::new(&programa.titulo).font(if ao_vivo { forte(13.0) } else { normal(13.0) }));
                        if !programa.categoria.is_empty() {
                            ui.label(egui::RichText::new(&programa.categoria).font(normal(10.5)).color(TERCIARIO));
                        }
                        if ao_vivo {
                            ui.add(egui::ProgressBar::new(programa.andamento(instante)).desired_height(3.0).fill(AZUL));
                            if !programa.descricao.is_empty() {
                                ui.label(egui::RichText::new(programa.descricao.chars().take(260).collect::<String>()).font(normal(11.0)).color(SECUNDARIO));
                            }
                        }
                    });
                    ui.add_space(4.0);
                }
            });
        });
}

// MARK: - Acervo em grade

fn acervo(app: &mut App, ctx: &egui::Context) {
    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(Color32::from_rgb(20, 20, 22)).inner_margin(egui::Margin::symmetric(24.0, 16.0)))
        .show(ctx, |ui| {
            let titulo = match app.aba {
                Aba::Inicio => "Início",
                Aba::Filmes => "Filmes",
                Aba::Series => "Séries",
                Aba::Animes => "Animes",
                Aba::Doramas => "Doramas",
                Aba::Favoritos => "Favoritos",
                Aba::Extras => "Extras",
                Aba::Canais => "",
            };
            ui.horizontal(|ui| {
                if botao_rotulo(ui, Icone::Voltar, "Voltar ao vídeo", "Fechar o acervo (Esc)").clicked() {
                    app.abrir_secao(Aba::Canais);
                    return;
                }
                ui.add_space(8.0);
                ui.label(egui::RichText::new(titulo).font(forte(22.0)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.set_max_width(320.0);
                    let dica = if matches!(app.aba, Aba::Series | Aba::Animes | Aba::Doramas) { "Buscar série" } else { "Buscar filme" };
                    if campo_de_busca(ui, &mut app.busca_vod, dica, "busca-acervo").changed() {
                        app.foco_vod = 0;
                    }
                });
            });
            if app.aba == Aba::Canais {
                return;
            }
            ui.add_space(10.0);

            if app.aba == Aba::Inicio {
                fileiras(app, ui);
                return;
            }

            // Letras.
            let colecao = matches!(app.aba, Aba::Animes | Aba::Doramas);
            let letras: Vec<(String, usize)> = if colecao { Vec::new() } else { app
                .gavetas
                .iter()
                .map(|g| (g.letra.clone(), if app.aba == Aba::Series { g.series } else { g.filmes }))
                .collect() };
            let mut escolhida = None;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                for (letra, quantos) in &letras {
                    if pilula(ui, None, letra, *letra == app.letra).on_hover_text(format!("{quantos} títulos")).clicked() {
                        escolhida = Some(letra.clone());
                    }
                }
            });
            if let Some(letra) = escolhida {
                app.busca_vod.clear();
                app.abrir_letra(letra);
            }
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Setas escolhem · Enter abre · S favorita · botão direito também favorita · Esc volta ao vídeo")
                    .font(normal(11.0))
                    .color(TERCIARIO),
            );
            ui.add_space(10.0);

            if let Some(serie) = app.serie_aberta.clone() {
                episodios(app, ui, &serie);
                return;
            }
            if app.carregando_vod {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(egui::RichText::new("Buscando…").color(SECUNDARIO));
                });
                return;
            }
            if !app.busca_vod.trim().is_empty() && app.busca_vod.trim().chars().count() >= 3 {
                if busca_no_acervo(app, ui) {
                    return;
                }
            }
            if app.aba == Aba::Filmes && app.busca_vod.trim().is_empty() {
                continuar_assistindo(app, ui);
            }
            grade(app, ui);
        });
}

/// A letra aberta não tem o título procurado: procura no acervo inteiro.
/// Devolve verdadeiro quando já desenhou os resultados.
fn busca_no_acervo(app: &mut App, ui: &mut egui::Ui) -> bool {
    let na_letra = if matches!(app.aba, Aba::Series | Aba::Animes | Aba::Doramas) { app.series_na_tela().len() } else { app.filmes_na_tela().len() };
    if na_letra > 0 {
        return false;
    }
    let busca = crate::catalogo::chave_de_ordem(app.busca_vod.trim());
    let serie = matches!(app.aba, Aba::Series | Aba::Animes | Aba::Doramas);
    let achados: Vec<vod::Achado> = app
        .acervo
        .iter()
        .filter(|a| a.serie == serie && crate::catalogo::chave_de_ordem(&a.titulo).contains(&busca))
        .take(200)
        .cloned()
        .collect();
    ui.label(egui::RichText::new(format!("Nada na letra {} — no acervo inteiro:", app.letra)).color(SECUNDARIO));
    ui.add_space(6.0);
    crate::telemetria::busca(1, &app.busca_vod, !achados.is_empty());
    if achados.is_empty() {
        ui.label(egui::RichText::new("Nenhum título com esse nome.").color(TERCIARIO));
        return true;
    }
    let mut ir = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for achado in &achados {
            let ano = if achado.ano.is_empty() { String::new() } else { format!(" ({})", achado.ano) };
            let texto = format!("{}{ano}   ·   letra {}", achado.titulo, achado.letra);
            if ui.selectable_label(false, texto).clicked() {
                ir = Some(achado.clone());
            }
        }
    });
    if let Some(achado) = ir {
        app.abrir_letra(achado.letra.clone());
        app.busca_vod = achado.titulo;
    }
    true
}

fn continuar_assistindo(app: &mut App, ui: &mut egui::Ui) {
    let pendentes = progresso::pendentes();
    if pendentes.is_empty() {
        return;
    }
    ui.label(egui::RichText::new("Continuar assistindo").font(forte(14.0)));
    ui.add_space(4.0);
    let mut escolhido = None;
    let mut esquecer = None;
    egui::ScrollArea::horizontal().id_salt("continuar").show(ui, |ui| {
        ui.horizontal(|ui| {
            for (titulo, marca) in pendentes.iter().take(20) {
                let (rect, resposta) = ui.allocate_exact_size(egui::vec2(230.0, 62.0), Sense::click());
                let fundo = if resposta.hovered() { Color32::from_white_alpha(26) } else { Color32::from_white_alpha(14) };
                ui.painter().rect_filled(rect, 10.0, fundo);
                let nome = titulo.split(" · ").next().unwrap_or(titulo).to_string();
                ui.painter().text(rect.left_top() + egui::vec2(12.0, 12.0), Align2::LEFT_TOP, &nome, forte(12.5), TEXTO);
                ui.painter().text(rect.left_top() + egui::vec2(12.0, 30.0), Align2::LEFT_TOP, progresso::falta(marca), normal(11.0), SECUNDARIO);
                let trilho = Rect::from_min_size(Pos2::new(rect.left() + 12.0, rect.bottom() - 10.0), egui::vec2(rect.width() - 24.0, 3.0));
                ui.painter().rect_filled(trilho, 1.5, Color32::from_white_alpha(40));
                let feito = Rect::from_min_size(trilho.min, egui::vec2(trilho.width() * (marca.posicao / marca.duracao) as f32, 3.0));
                ui.painter().rect_filled(feito, 1.5, VERMELHO);
                let resposta = resposta.on_hover_text("Clique para achar e continuar · botão direito tira da lista");
                if resposta.clicked() {
                    escolhido = Some(nome);
                }
                if resposta.secondary_clicked() {
                    esquecer = Some(titulo.clone());
                }
            }
        });
    });
    if let Some(titulo) = esquecer {
        progresso::esquecer(&titulo);
    }
    if let Some(nome) = escolhido {
        app.busca_vod = nome;
    }
    ui.add_space(12.0);
}

/// A primeira tela do acervo: fileiras de capa que correm para o lado.
///
/// Grade alfabética serve para achar o que já se sabe que existe; não serve
/// para descobrir. As fileiras mostram o que há — o que estava sendo
/// assistido, o que foi marcado, o que está em alta — e a busca continua no
/// mesmo lugar, filtrando dentro delas.
///
/// As capas dos destaques já vêm anotadas quando a lista chega (ver
/// `capas::anotar`), então desenhar esta tela não dispara busca nenhuma ao
/// TMDB. Só "continuar assistindo" e favoritos procuram capa, e são poucos.
fn fileiras(app: &mut App, ui: &mut egui::Ui) {
    let visiveis = filas_na_tela(app);
    if visiveis.is_empty() {
        let texto = if app.carregando_vod { "Carregando…" } else { "Nada aqui" };
        ui.label(egui::RichText::new(texto).color(SECUNDARIO));
        return;
    }

    let largura = 132.0;
    let altura = 238.0;
    let espaco = 14.0;
    app.foco_fila = app.foco_fila.min(visiveis.len() - 1);
    let na_fileira = visiveis[app.foco_fila].1.len();
    let foco = app.foco_vod.min(na_fileira.saturating_sub(1));
    let andou = ui.input(|i| {
        i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::ArrowUp)
            || i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::ArrowRight)
    });

    let mut escolhido: Option<(usize, usize)> = None;
    let mut favoritar: Option<String> = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (indice_fila, (nome, cartoes)) in visiveis.iter().enumerate() {
            ui.label(egui::RichText::new(nome).font(forte(15.0)));
            ui.add_space(5.0);
            egui::ScrollArea::horizontal()
                .id_salt(format!("fila-{indice_fila}"))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = espaco;
                        for (indice, item) in cartoes.iter().enumerate() {
                            let focado = indice_fila == app.foco_fila && indice == foco;
                            let capa = app.capa(&item.titulo, item.serie);
                            let marca = if item.serie {
                                None
                            } else {
                                progresso::onde_parou(&item.titulo)
                            };
                            let resposta = cartao(
                                ui, &item.rotulo, &item.detalhe, capa, item.favorito,
                                focado, marca, largura, altura);
                            if focado && andou {
                                resposta.scroll_to_me(None);
                            }
                            if resposta.clicked() {
                                escolhido = Some((indice_fila, indice));
                            }
                            if resposta.secondary_clicked() {
                                favoritar = Some(item.titulo.clone());
                            }
                        }
                    });
                });
            ui.add_space(espaco);
        }
    });

    if let Some(titulo) = favoritar {
        app.favoritar_vod(titulo);
        return;
    }
    if let Some((fila, indice)) = escolhido {
        app.foco_fila = fila;
        app.foco_vod = indice;
        abrir_da_fileira(app, fila, indice);
    }
}

/// Um cartão da fileira, já com o que a tela precisa desenhar.
struct CartaoDaFila {
    /// O nome como está no acervo — é por ele que se acha o título.
    titulo: String,
    /// O que aparece embaixo da capa: na série em andamento, com o episódio.
    rotulo: String,
    detalhe: String,
    serie: bool,
    favorito: bool,
    /// De onde este cartão veio, para saber como abri-lo.
    origem: Origem,
}

#[derive(Clone)]
enum Origem {
    /// Um destaque publicado: fila e posição dentro dela.
    Destaque(usize, usize),
    /// Um título que estava pela metade ou marcado: só o nome.
    PeloNome,
}

/// Quantos cartões cada fileira tem, para o teclado saber onde pode ir.
pub fn tamanho_das_filas(app: &App) -> Vec<usize> {
    filas_na_tela(app).iter().map(|(_, cartoes)| cartoes.len()).collect()
}

/// Abre o cartão em foco pelo teclado.
pub fn abrir_em_foco_na_fileira(app: &mut App) {
    let (fila, indice) = (app.foco_fila, app.foco_vod);
    abrir_da_fileira(app, fila, indice);
}

/// As fileiras já filtradas pela busca: digitar procura dentro do que está à
/// vista, e a fileira que ficou sem nada sai da tela.
fn filas_na_tela(app: &App) -> Vec<(String, Vec<CartaoDaFila>)> {
    let mut out: Vec<(String, Vec<CartaoDaFila>)> = Vec::new();

    // Só o que existe no acervo comum. Os extras ficam de fora do índice de
    // busca de propósito — o que não aparece sem o código também não pode
    // aparecer numa busca comum — e a mesma regra vale aqui: a tela inicial
    // abre sem código nenhum e não pode ser por onde um título reservado
    // reaparece. Enquanto o índice não chegou, a fileira fica vazia: mostrar
    // de menos é o erro certo a cometer.
    let pendentes: Vec<_> = progresso::pendentes()
        .into_iter()
        .filter(|(titulo, _)| {
            let nome = titulo.split(" · ").next().unwrap_or(titulo);
            app.acervo.iter().any(|a| a.titulo == nome)
        })
        .collect();
    if !pendentes.is_empty() {
        let cartoes = pendentes
            .iter()
            .take(20)
            .map(|(titulo, _)| {
                let nome = titulo.split(" · ").next().unwrap_or(titulo).to_string();
                CartaoDaFila {
                    favorito: app.favoritos_vod.iter().any(|t| *t == nome),
                    rotulo: titulo.clone(),
                    detalhe: String::new(),
                    serie: titulo.contains(" · "),
                    titulo: nome,
                    origem: Origem::PeloNome,
                }
            })
            .collect();
        out.push(("Continue assistindo".to_string(), cartoes));
    }

    if !app.favoritos_vod.is_empty() {
        let cartoes = app
            .favoritos_vod
            .iter()
            .map(|titulo| CartaoDaFila {
                titulo: titulo.clone(),
                rotulo: titulo.clone(),
                detalhe: String::new(),
                serie: false,
                favorito: true,
                origem: Origem::PeloNome,
            })
            .collect();
        out.push(("Favoritos".to_string(), cartoes));
    }

    for (indice_fila, fila) in app.filas.iter().enumerate() {
        let cartoes: Vec<CartaoDaFila> = fila
            .itens
            .iter()
            .enumerate()
            .map(|(indice, item)| CartaoDaFila {
                favorito: app.favoritos_vod.iter().any(|t| *t == item.titulo),
                rotulo: item.titulo.clone(),
                detalhe: if item.serie() { "Série".into() } else { "Filme".into() },
                serie: item.serie(),
                titulo: item.titulo.clone(),
                origem: Origem::Destaque(indice_fila, indice),
            })
            .collect();
        if !cartoes.is_empty() {
            out.push((fila.titulo.clone(), cartoes));
        }
    }

    let busca = app.busca_vod.trim().to_lowercase();
    if busca.is_empty() {
        return out;
    }
    out.into_iter()
        .filter_map(|(nome, cartoes)| {
            let filtrados: Vec<CartaoDaFila> = cartoes
                .into_iter()
                .filter(|c| c.titulo.to_lowercase().contains(&busca))
                .collect();
            if filtrados.is_empty() { None } else { Some((nome, filtrados)) }
        })
        .collect()
}

/// Abre o que foi escolhido na fileira.
///
/// Filme e série passam pelo caminho de sempre. Anime e dorama moram nas
/// coleções: abre-se a seção deles e a busca leva ao título, que é o mesmo
/// gesto de quem chegou lá pela lista.
fn abrir_da_fileira(app: &mut App, fila: usize, indice: usize) {
    let visiveis = filas_na_tela(app);
    let Some((_, cartoes)) = visiveis.get(fila) else { return };
    let Some(cartao) = cartoes.get(indice) else { return };
    let titulo = cartao.titulo.clone();
    match cartao.origem.clone() {
        Origem::Destaque(f, i) => {
            let Some(item) = app.filas.get(f).and_then(|fila| fila.itens.get(i)) else { return };
            let destino = if item.da_colecao() {
                if item.colecao() == "animes" { Aba::Animes } else { Aba::Doramas }
            } else if item.serie() {
                Aba::Series
            } else {
                Aba::Filmes
            };
            let letra = item.letra.clone();
            app.abrir_secao(destino);
            if !letra.is_empty() {
                app.abrir_letra(letra);
            }
            app.busca_vod = titulo;
            app.foco_vod = 0;
        }
        Origem::PeloNome => {
            app.abrir_secao(if cartao.serie { Aba::Series } else { Aba::Filmes });
            app.busca_vod = titulo;
            app.foco_vod = 0;
        }
    }
}

/// A grade de capas, só com as linhas à vista.
fn grade(app: &mut App, ui: &mut egui::Ui) {
    let serie = matches!(app.aba, Aba::Series | Aba::Animes | Aba::Doramas);
    let filmes = if serie { Vec::new() } else { app.filmes_na_tela() };
    let series = if serie { app.series_na_tela() } else { Vec::new() };
    let total = if serie { series.len() } else { filmes.len() };
    if total == 0 {
        let texto = match app.aba {
            Aba::Favoritos => "Nenhum favorito nesta letra. Marque com S ou com o botão direito.",
            Aba::Extras => "Nenhum título extra nesta letra.",
            _ => "Nada nesta letra.",
        };
        ui.label(egui::RichText::new(texto).color(SECUNDARIO));
        return;
    }

    let largura_cartao = 150.0;
    let altura_cartao = 268.0;
    let espaco = 16.0;
    let colunas = (((ui.available_width() + espaco) / (largura_cartao + espaco)).floor() as usize).max(1);
    app.colunas_vod = colunas;
    let linhas = total.div_ceil(colunas);
    let foco = app.foco_vod.min(total - 1);
    let rolar = ui.input(|i| {
        i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::ArrowUp)
            || i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::ArrowRight)
    });

    let mut acao: Option<(usize, bool)> = None; // (índice, é favoritar)
    egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, altura_cartao + espaco, linhas, |ui, faixa| {
        for linha in faixa {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = espaco;
                for coluna in 0..colunas {
                    let indice = linha * colunas + coluna;
                    if indice >= total {
                        break;
                    }
                    let (titulo, detalhe) = if serie {
                        let s = &series[indice];
                        let ano = if s.ano.is_empty() { String::new() } else { format!("{} · ", s.ano) };
                        (s.titulo.clone(), format!("{ano}{} episódios", s.episodios))
                    } else {
                        let f = &filmes[indice];
                        let versoes = f.versoes.iter().map(|(v, _)| v.to_uppercase()).collect::<Vec<_>>().join(" · ");
                        (f.titulo.clone(), versoes)
                    };
                    let favorito = app.favoritos_vod.iter().any(|t| *t == titulo);
                    let capa = app.capa(&titulo, serie);
                    let marca = if serie { None } else { progresso::onde_parou(&titulo) };
                    let resposta = cartao(ui, &titulo, &detalhe, capa, favorito, indice == foco, marca, largura_cartao, altura_cartao);
                    if indice == foco && rolar {
                        resposta.scroll_to_me(None);
                    }
                    if resposta.clicked() {
                        acao = Some((indice, false));
                    }
                    if resposta.secondary_clicked() {
                        acao = Some((indice, true));
                    }
                }
            });
            ui.add_space(espaco);
        }
    });

    if let Some((indice, favoritar)) = acao {
        app.foco_vod = indice;
        if favoritar {
            let titulo = if serie { series[indice].titulo.clone() } else { filmes[indice].titulo.clone() };
            app.favoritar_vod(titulo);
        } else {
            app.abrir_do_acervo();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn cartao(
    ui: &mut egui::Ui,
    titulo: &str,
    detalhe: &str,
    capa: Option<egui::TextureHandle>,
    favorito: bool,
    focado: bool,
    marca: Option<progresso::Marca>,
    largura: f32,
    altura: f32,
) -> egui::Response {
    let (rect, resposta) = ui.allocate_exact_size(egui::vec2(largura, altura), Sense::click());
    let pintor = ui.painter().clone();
    let quadro = Rect::from_min_size(rect.min, egui::vec2(largura, largura * 1.5));
    let quadro = if resposta.hovered() { quadro.expand(2.0) } else { quadro };
    pintor.rect_filled(quadro, 9.0, ELEVADO);
    match capa {
        Some(textura) => {
            egui::Image::new(&textura).fit_to_exact_size(quadro.size()).rounding(9.0).paint_at(ui, quadro);
        }
        None => {
            pintar_icone(&pintor, Icone::Filmes, quadro.center() - egui::vec2(0.0, 12.0), 30.0, TERCIARIO);
            let mut job = egui::text::LayoutJob::simple(titulo.to_string(), normal(12.0), SECUNDARIO, quadro.width() - 20.0);
            job.halign = egui::Align::Center;
            let galeria = ui.fonts(|f| f.layout_job(job));
            pintor.galley(Pos2::new(quadro.center().x, quadro.center().y + 14.0), galeria, SECUNDARIO);
        }
    }
    if focado {
        pintor.rect_stroke(quadro.expand(3.0), 11.0, Stroke::new(2.5, AZUL));
    }
    if favorito {
        let centro = quadro.right_top() + egui::vec2(-16.0, 16.0);
        pintor.circle_filled(centro, 12.0, Color32::from_black_alpha(160));
        desenhar_estrela_cheia(&pintor, centro, 13.0, AMARELO);
    }
    if let Some(marca) = marca {
        let trilho = Rect::from_min_size(Pos2::new(quadro.left() + 8.0, quadro.bottom() - 9.0), egui::vec2(quadro.width() - 16.0, 4.0));
        pintor.rect_filled(trilho, 2.0, Color32::from_black_alpha(170));
        let feito = Rect::from_min_size(trilho.min, egui::vec2(trilho.width() * (marca.posicao / marca.duracao) as f32, 4.0));
        pintor.rect_filled(feito, 2.0, VERMELHO);
    }

    let mut job = egui::text::LayoutJob::simple(titulo.to_string(), forte(12.5), TEXTO, largura);
    job.wrap.max_rows = 2;
    job.wrap.break_anywhere = false;
    job.wrap.overflow_character = Some('…');
    let galeria = ui.fonts(|f| f.layout_job(job));
    let y = rect.top() + largura * 1.5 + 7.0;
    pintor.galley(Pos2::new(rect.left(), y), galeria.clone(), TEXTO);
    pintor.text(
        Pos2::new(rect.left(), y + galeria.size().y + 2.0),
        Align2::LEFT_TOP,
        detalhe,
        normal(10.5),
        SECUNDARIO,
    );
    let dica = if favorito { "Enter assiste · S tira dos favoritos" } else { "Enter assiste · S favorita" };
    resposta.on_hover_text(dica)
}

fn episodios(app: &mut App, ui: &mut egui::Ui, serie: &vod::Serie) {
    let capa = app.capa(&serie.titulo, true);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(90.0, 135.0), Sense::hover());
        ui.painter().rect_filled(rect, 8.0, ELEVADO);
        if let Some(textura) = capa {
            egui::Image::new(&textura).fit_to_exact_size(rect.size()).rounding(8.0).paint_at(ui, rect);
        }
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(&serie.titulo).font(forte(20.0)));
            let ano = if serie.ano.is_empty() { String::new() } else { format!("{} · ", serie.ano) };
            ui.label(egui::RichText::new(format!("{ano}{} episódios", serie.episodios)).color(SECUNDARIO));
            ui.add_space(8.0);
            if botao_rotulo(ui, Icone::Voltar, "Todas as séries", "Voltar (Esc)").clicked() {
                app.serie_aberta = None;
            }
        });
    });
    ui.add_space(12.0);
    if app.carregando_vod {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(egui::RichText::new("Buscando episódios…").color(SECUNDARIO));
        });
        return;
    }

    let foco = app.foco_vod;
    let mut tocar = None;
    let mut temporada = u32::MAX;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (posicao, episodio) in app.episodios.iter().enumerate() {
            if episodio.temporada != temporada {
                temporada = episodio.temporada;
                ui.add_space(6.0);
                ui.label(egui::RichText::new(format!("Temporada {temporada}")).font(forte(14.0)));
                ui.add_space(2.0);
            }
            let nome = format!("T{} E{}", episodio.temporada, episodio.numero);
            let chave = format!("{} · {nome}", serie.titulo);
            let marca = progresso::onde_parou(&chave);
            let (rect, resposta) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(720.0), 40.0), Sense::click());
            let fundo = if posicao == foco {
                AZUL.gamma_multiply(0.35)
            } else if resposta.hovered() {
                Color32::from_white_alpha(20)
            } else {
                Color32::from_white_alpha(8)
            };
            ui.painter().rect_filled(rect, 8.0, fundo);
            pintar_icone(ui.painter(), Icone::Tocar, Pos2::new(rect.left() + 18.0, rect.center().y), 11.0, TEXTO);
            ui.painter().text(Pos2::new(rect.left() + 36.0, rect.center().y), Align2::LEFT_CENTER, format!("Episódio {}", episodio.numero), forte(13.0), TEXTO);
            let direita = match &marca {
                Some(m) => progresso::falta(m),
                None => episodio.versao.to_uppercase(),
            };
            ui.painter().text(Pos2::new(rect.right() - 12.0, rect.center().y), Align2::RIGHT_CENTER, direita, normal(11.0), SECUNDARIO);
            if let Some(m) = marca {
                let trilho = Rect::from_min_size(Pos2::new(rect.left() + 36.0, rect.bottom() - 6.0), egui::vec2(rect.width() - 50.0, 2.0));
                ui.painter().rect_filled(trilho, 1.0, Color32::from_white_alpha(35));
                let feito = Rect::from_min_size(trilho.min, egui::vec2(trilho.width() * (m.posicao / m.duracao) as f32, 2.0));
                ui.painter().rect_filled(feito, 1.0, VERMELHO);
            }
            if posicao == foco && ui.input(|i| i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::ArrowUp)) {
                resposta.scroll_to_me(Some(egui::Align::Center));
            }
            if resposta.clicked() {
                tocar = Some(posicao);
            }
            ui.add_space(4.0);
        }
    });
    if let Some(posicao) = tocar {
        app.foco_vod = posicao;
        app.abrir_do_acervo();
    }
}

// MARK: - Avisos, atalhos e atualização

fn avisos(app: &mut App, ctx: &egui::Context) {
    let Some((texto, quando)) = app.aviso.clone() else { return };
    if quando.elapsed() > Duration::from_secs(3) {
        app.aviso = None;
        return;
    }
    ctx.request_repaint_after(Duration::from_millis(250));
    egui::Area::new(egui::Id::new("aviso"))
        .order(egui::Order::Foreground)
        .anchor(Align2::CENTER_TOP, egui::vec2(0.0, 24.0))
        .show(ctx, |ui| {
            egui::Frame::none().fill(VIDRO).rounding(20.0).inner_margin(egui::Margin::symmetric(18.0, 9.0)).show(ui, |ui| {
                ui.label(egui::RichText::new(texto).font(forte(13.0)));
            });
        });
}

/// Todos os comandos numa tela só.
fn atalhos(app: &mut App, ctx: &egui::Context) {
    let linhas: [(&str, &str); 18] = [
        ("Espaço", "tocar ou pausar"),
        ("Seta cima / baixo", "andar pela lista"),
        ("Enter", "assistir o escolhido"),
        ("Page Up / Page Down", "canal anterior / próximo"),
        ("Seta direita", "trocar a fonte do canal"),
        ("Setas no filme", "voltar / avançar 10 s"),
        ("0 a 9", "ir direto ao número do canal"),
        ("G", "guia de programação"),
        ("L", "mostrar ou esconder a lista"),
        ("Tab", "canais, filmes e séries"),
        ("S", "favoritar (canal, filme ou série)"),
        ("Ctrl + F", "buscar canal"),
        ("F  ou  F11", "tela cheia"),
        ("M", "sem som"),
        ("+  −", "volume"),
        ("H  ou  F1", "esta tela"),
        ("Esc", "voltar / fechar / sair da tela cheia"),
        ("Botão direito", "favoritar na lista ou na grade"),
    ];
    let mut aberta = true;
    egui::Window::new(egui::RichText::new("Atalhos do teclado").font(forte(16.0)))
        .order(egui::Order::Foreground)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .open(&mut aberta)
        .frame(egui::Frame::window(&ctx.style()).fill(PAINEL).inner_margin(egui::Margin::same(18.0)))
        .show(ctx, |ui| {
            egui::Grid::new("atalhos").num_columns(2).spacing([28.0, 8.0]).show(ui, |ui| {
                for (tecla, acao) in linhas {
                    egui::Frame::none().fill(ELEVADO).rounding(5.0).inner_margin(egui::Margin::symmetric(8.0, 3.0)).show(ui, |ui| {
                        ui.label(egui::RichText::new(tecla).font(forte(12.0)));
                    });
                    ui.label(egui::RichText::new(acao).color(SECUNDARIO));
                    ui.end_row();
                }
            });
            ui.add_space(10.0);
            ui.label(egui::RichText::new("Todo botão da barra também mostra o atalho ao passar o mouse.").font(normal(11.0)).color(TERCIARIO));
        });
    if !aberta {
        app.ajuda_aberta = false;
    }
}

fn atualizacao_na_tela(app: &mut App, ctx: &egui::Context) {
    let Some(versao) = app.nova_versao.clone() else { return };
    let mut aberta = true;
    egui::Window::new(egui::RichText::new(format!("Versão {} disponível", versao.numero)).font(forte(15.0)))
        .order(egui::Order::Foreground)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .open(&mut aberta)
        .frame(egui::Frame::window(&ctx.style()).fill(PAINEL).inner_margin(egui::Margin::same(18.0)))
        .show(ctx, |ui| {
            ui.set_max_width(460.0);
            ui.label(egui::RichText::new(format!("Você está na {}", crate::VERSAO)).color(SECUNDARIO));
            if !versao.notas.is_empty() {
                ui.add_space(8.0);
                egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                    ui.label(versao.notas.chars().take(900).collect::<String>());
                });
            }
            ui.add_space(8.0);
            ui.label(egui::RichText::new("O download abre no navegador. Depois é só fechar o Saimo TV e descompactar por cima.").font(normal(11.0)).color(TERCIARIO));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(egui::RichText::new("Baixar").color(Color32::WHITE)).fill(AZUL)).clicked() {
                    atualizacao::abrir_no_navegador(&versao.link);
                    atualizacao::adiar(&versao);
                    app.nova_versao = None;
                }
                if ui.button("Depois").clicked() {
                    atualizacao::adiar(&versao);
                    app.nova_versao = None;
                }
            });
        });
    if !aberta {
        atualizacao::adiar(&versao);
        app.nova_versao = None;
    }
}
