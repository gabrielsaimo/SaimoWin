//! O desenho: vídeo no fundo, lista e letreiro por cima.
//!
//! Tudo numa passada só do egui. O vídeo entra como uma chamada de desenho
//! OpenGL antes de qualquer widget, então a interface fica sempre por cima da
//! imagem sem precisar de uma segunda janela.

use crate::{atualizacao, catalogo, App};
use eframe::egui;
use eframe::glow::HasContext;
use std::sync::Arc;
use std::time::{Duration, Instant};

const ROXO: egui::Color32 = egui::Color32::from_rgb(139, 92, 246);
const ROSA: egui::Color32 = egui::Color32::from_rgb(236, 72, 153);

pub fn desenhar(app: &mut App, ctx: &egui::Context) {
    video(app, ctx);

    if app.lista_aberta {
        lista(app, ctx);
    }
    letreiro(app, ctx);
    avisos(app, ctx);
    atualizacao_na_tela(app, ctx);
}

/// O quadro do mpv, desenhado no framebuffer da janela.
fn video(app: &mut App, ctx: &egui::Context) {
    let Some(mpv) = app.mpv.clone() else {
        sem_mpv(app, ctx);
        return;
    };
    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(egui::Color32::BLACK))
        .show(ctx, |ui| {
            let rect = ui.available_rect_before_wrap();
            let callback = egui::PaintCallback {
                rect,
                callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |info, pintor| {
                    // A primeira chamada acontece já com o contexto OpenGL de
                    // pé, que é o que o mpv precisa para se ligar à janela.
                    if let Err(erro) = mpv.ligar_video() {
                        crate::telemetria::erro(erro);
                        return;
                    }
                    let gl = pintor.gl();
                    let fbo = unsafe { gl.get_parameter_i32(eframe::glow::DRAW_FRAMEBUFFER_BINDING) };
                    let [largura, altura] = info.screen_size_px;
                    mpv.desenhar(fbo, largura as i32, altura as i32);
                })),
            };
            ui.painter().add(callback);
        });
}

fn sem_mpv(app: &App, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(120.0);
            ui.heading("Falta o mpv");
            ui.label(app.erro_do_mpv.clone().unwrap_or_default());
            ui.label("O arquivo libmpv-2.dll precisa estar na mesma pasta do Saimo TV.exe.");
        });
    });
}

fn lista(app: &mut App, ctx: &egui::Context) {
    let largura = (ctx.screen_rect().width() * 0.34).clamp(320.0, 520.0);
    egui::SidePanel::left("canais")
        .exact_width(largura)
        .frame(
            egui::Frame::none()
                .fill(egui::Color32::from_rgba_unmultiplied(12, 12, 16, 235))
                .inner_margin(egui::Margin::symmetric(12.0, 12.0)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Saimo").size(20.0).strong());
                ui.label(egui::RichText::new("TV").size(20.0).strong().color(ROSA));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(format!("{} canais", app.canais.len())).weak());
                });
            });
            ui.add_space(6.0);
            let busca = ui.add(
                egui::TextEdit::singleline(&mut app.busca)
                    .hint_text("Buscar canal…")
                    .desired_width(f32::INFINITY),
            );
            if busca.changed() {
                if let Some(primeiro) = app.visiveis().first() {
                    app.foco = *primeiro;
                }
            }
            ui.add_space(8.0);

            let visiveis = app.visiveis();
            let tocando = app.tocando.as_ref().map(|t| t.canal);
            let foco = app.foco;
            let mut clicado: Option<usize> = None;
            let mut favoritar: Option<String> = None;
            let mut secao_atual = String::new();

            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for indice in visiveis {
                    let (nome, secao, logo, fontes) = {
                        let canal = &app.canais[indice];
                        (canal.nome.clone(), canal.secao().to_string(), canal.logo.clone(), canal.fontes.len())
                    };
                    if secao != secao_atual {
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(secao.to_uppercase()).size(11.0).color(ROXO).strong());
                        ui.add_space(2.0);
                        secao_atual = secao;
                    }
                    let favorito = app.favoritos.iter().any(|f| *f == nome);
                    let textura = logo.as_ref().and_then(|url| app.logo(url));
                    let resposta = linha_do_canal(
                        ui,
                        indice + 1,
                        &nome,
                        fontes,
                        textura,
                        favorito,
                        Some(indice) == tocando,
                        indice == foco,
                    );
                    if resposta.clicked() {
                        clicado = Some(indice);
                    }
                    if resposta.secondary_clicked() {
                        favoritar = Some(nome.clone());
                    }
                    if indice == foco && ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { .. }))) {
                        resposta.scroll_to_me(Some(egui::Align::Center));
                    }
                }
            });

            if let Some(canal) = favoritar {
                app.favoritar(canal);
            }
            if let Some(indice) = clicado {
                app.foco = indice;
                app.tocar(indice, 0, true);
                app.lista_aberta = false;
            }

            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Enter assiste · → troca a fonte · L favorita · F tela cheia · Esc esconde a lista",
                )
                .size(11.0)
                .weak(),
            );
        });
}

#[allow(clippy::too_many_arguments)]
fn linha_do_canal(
    ui: &mut egui::Ui,
    numero: usize,
    nome: &str,
    fontes: usize,
    logo: Option<egui::TextureHandle>,
    favorito: bool,
    tocando: bool,
    focado: bool,
) -> egui::Response {
    let altura = 46.0;
    let (rect, resposta) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), altura), egui::Sense::click());
    let pintor = ui.painter();
    if focado || tocando || resposta.hovered() {
        let cor = if tocando {
            egui::Color32::from_rgba_unmultiplied(139, 92, 246, 60)
        } else if focado {
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 26)
        } else {
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 14)
        };
        pintor.rect_filled(rect, 8.0, cor);
    }
    if focado {
        pintor.rect_stroke(rect, 8.0, egui::Stroke::new(1.5, ROXO));
    }

    let mut x = rect.left() + 10.0;
    pintor.text(
        egui::pos2(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        format!("{numero:03}"),
        egui::FontId::monospace(12.0),
        egui::Color32::from_gray(140),
    );
    x += 40.0;
    if let Some(textura) = logo {
        let quadro = egui::Rect::from_min_size(egui::pos2(x, rect.center().y - 14.0), egui::vec2(34.0, 28.0));
        egui::Image::new(&textura)
            .maintain_aspect_ratio(true)
            .fit_to_exact_size(quadro.size())
            .paint_at(ui, quadro);
    }
    x += 42.0;
    pintor.text(
        egui::pos2(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        nome,
        egui::FontId::proportional(14.5),
        egui::Color32::from_gray(235),
    );
    let direita = rect.right() - 10.0;
    if favorito {
        pintor.text(
            egui::pos2(direita, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            "★",
            egui::FontId::proportional(14.0),
            ROSA,
        );
    } else if fontes > 1 {
        pintor.text(
            egui::pos2(direita, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            format!("{fontes} fontes"),
            egui::FontId::proportional(10.5),
            egui::Color32::from_gray(110),
        );
    }
    resposta
}

/// Nome, número e fonte do que está tocando; some sozinho.
fn letreiro(app: &App, ctx: &egui::Context) {
    let Some(tocando) = app.tocando.as_ref() else { return };
    if Instant::now() > app.letreiro_ate && !app.lista_aberta {
        return;
    }
    let Some(canal) = app.canais.get(tocando.canal) else { return };
    let host = canal
        .fontes
        .get(tocando.fonte)
        .and_then(|f| f.url.split("://").nth(1))
        .and_then(|resto| resto.split('/').next())
        .unwrap_or("");
    egui::Area::new(egui::Id::new("letreiro"))
        .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(24.0, -24.0))
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 170))
                .rounding(12.0)
                .inner_margin(egui::Margin::symmetric(16.0, 12.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{:03}", tocando.canal + 1))
                                .size(22.0)
                                .color(ROXO)
                                .strong(),
                        );
                        ui.label(egui::RichText::new(&canal.nome).size(22.0).strong());
                        if app.pausado {
                            ui.label(egui::RichText::new("pausado").size(12.0).weak());
                        }
                    });
                    let qualidade = app
                        .mpv
                        .as_ref()
                        .and_then(|m| m.ler("video-params/h"))
                        .map(|altura| format!(" · {altura}p"))
                        .unwrap_or_default();
                    ui.label(
                        egui::RichText::new(format!(
                            "fonte {}/{} · {host}{qualidade}",
                            tocando.fonte + 1,
                            canal.fontes.len()
                        ))
                        .size(12.0)
                        .weak(),
                    );
                });
        });
}

fn avisos(app: &mut App, ctx: &egui::Context) {
    let Some((texto, quando)) = app.aviso.clone() else { return };
    if quando.elapsed() > Duration::from_secs(4) {
        app.aviso = None;
        return;
    }
    ctx.request_repaint_after(Duration::from_millis(250));
    egui::Area::new(egui::Id::new("aviso"))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-24.0, 24.0))
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(egui::Color32::from_rgba_unmultiplied(20, 20, 26, 230))
                .rounding(10.0)
                .inner_margin(egui::Margin::symmetric(14.0, 10.0))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(texto).size(13.0));
                });
        });
}

fn atualizacao_na_tela(app: &mut App, ctx: &egui::Context) {
    let Some(versao) = app.nova_versao.clone() else { return };
    let mut aberta = true;
    egui::Window::new("Atualização")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .open(&mut aberta)
        .show(ctx, |ui| {
            ui.set_max_width(460.0);
            ui.label(
                egui::RichText::new(format!("Versão {} disponível", versao.numero))
                    .size(16.0)
                    .strong(),
            );
            ui.label(egui::RichText::new(format!("Você está na {}", crate::VERSAO)).weak());
            if !versao.notas.is_empty() {
                ui.add_space(8.0);
                egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                    ui.label(versao.notas.chars().take(800).collect::<String>());
                });
            }
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(
                    "O download abre no navegador. Depois é só fechar o Saimo TV e descompactar por cima.",
                )
                .size(11.0)
                .weak(),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Baixar").clicked() {
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

/// Usado pela lista: nome sem acento para a busca.
pub fn _chave(texto: &str) -> String {
    catalogo::chave_de_ordem(texto)
}
