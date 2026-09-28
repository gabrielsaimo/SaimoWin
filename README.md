# Saimo TV para Windows

Aplicativo nativo: janela, lista e letreiro desenhados pela GPU (egui sobre
OpenGL) e vídeo pelo mpv. Não há navegador nem página HTML em lugar nenhum.

Versão atual: **2.0.0**. O instalador sai no release único de
[SaimoPlayer](https://github.com/gabrielsaimo/SaimoPlayer/releases/latest), e o
programa avisa quando há versão nova.

## Visual

Mesma identidade da TV Box: azul-marinho com acento ciano, menu do topo com
todas as seções (Início, Ao vivo, Filmes, Séries, Animes, Doramas, Favoritos)
em todas as telas, lista de canais com seções em chips e número do canal, e o
acervo sobre a imagem de fundo do app (`assets/fundo.jpg`). Filmes e Séries
abrem com a fileira "Em alta" e a grade vem dos lançamentos para trás.

## Novidades da 2.0

- **Pular abertura e recapitulação** com os tempos do
  [TheIntroDB](https://theintrodb.org) (`src/pulos.rs`): o botão aparece só
  quando o trecho foi marcado para aquele episódio. Enter pula.
- **Próximo episódio** nos créditos, seguindo sozinho em 10 s. Enter assiste
  agora, Esc dispensa. Sem créditos marcados, o cartão sobe nos últimos 30 s.
- Episódio que acaba emenda no seguinte, na mesma versão (dublado/legendado).
- Fonte que "termina" nos primeiros 30 s é tratada como quebrada: vai para a
  próxima em vez de fechar.
- Abre no último canal assistido.
- Ao vivo não pausa: o botão de pausa e o Espaço só valem para filme e série.
- Trocar de canal não derruba mais a fonte nova: o fim do arquivo anterior
  (motivo STOP do mpv) era lido como queda, e o canal passava por todas as
  fontes em um segundo até dizer que estava fora do ar. `mpv::Vigia` só conta
  o fim do arquivo que está no ar, e por erro ou fim de verdade.
- Guia sem "No Data" no lugar do programa.

## Testes

```bash
cargo test                                      # lógica, sem rede
cargo test bateria -- --ignored --nocapture     # contra a rede de verdade
```

A bateria sonda a lista publicada (até três fontes por canal), uma amostra do
acervo e o TheIntroDB, e lista o que está fora e por quê. Serve para separar
"o app está quebrado" de "a fonte caiu".

Para ver as telas no Mac sem o mpv: `SAIMO_DEMO=1 SAIMO_ABA=filmes cargo run`.

| Peça | O que faz |
|---|---|
| `src/main.rs` | estado do app, teclado, troca de fonte |
| `src/tela.rs` | desenho: vídeo no fundo, lista e letreiro por cima |
| `src/mpv.rs` | ponte com o libmpv, carregado da DLL em tempo de execução |
| `src/catalogo.rs` | a mesma lista publicada dos outros aplicativos |
| `src/telemetria.rs` | Saimo Monitor, plataforma `windows` |
| `src/atualizacao.rs` | versão nova no GitHub, download pelo navegador |
| `src/pulos.rs` | TheIntroDB: abertura, recapitulação e créditos |
| `src/vod.rs`, `src/progresso.rs` | acervo e onde cada título parou |

## Versões

egui/eframe 0.36 (os painéis agora são desenhados dentro do `Ui` que o
`eframe::App::ui` recebe), ureq 3 para a rede e libloading 0.9 para carregar a
libmpv. Compila sem nenhum aviso.

## Montar o pacote

Do próprio Mac, com o mingw-w64 (`brew install mingw-w64`):

```bash
cd "/Volumes/SSD 1TB/DEV/Saimo/SaimoWin" && ./empacotar.sh
```

Sai `dist/SaimoTV-Instalador.msi`: o executável, a `libmpv-2.dll` e o
instalador que põe os dois em `%LOCALAPPDATA%\Programs\Saimo TV`, com atalho
no menu Iniciar e na Área de Trabalho, ficha em "Aplicativos instalados" e
desinstalação pelo painel do Windows. Instala sem senha de administrador,
porque instala só para quem está usando o computador.

Precisa do `wixl` (`brew install msitools`). As telas do assistente estão em
`instalador/ui`, que é a cópia traduzida das do WiX. O `--baixar` pega a
biblioteca do mpv de novo (compilação oficial do shinchiro/mpv-winbuild-cmake).

## Rodar no Mac para conferir a interface

O binário do Mac serve para ver lista, busca e teclado sem um PC por perto:

```bash
cargo run --release
cargo test                                   # inclui a leitura do TheIntroDB
cargo check --target x86_64-pc-windows-gnu   # confere o alvo de verdade
```

`SAIMO_MPV` aponta para outra biblioteca do mpv e `SAIMO_SEM_TELEMETRIA=1`
impede que o teste vire aparelho no painel.

## Guia e acervo

O guia sai das mesmas fontes dos outros aplicativos: meuguia.tv para a TV aberta
e os canais grandes, o guia da própria Pluto TV casado pelo id que está no link
do canal, e dois feeds XMLTV para o resto. Fica guardado por seis horas.

Filmes e séries vêm do acervo publicado em `vod/`, fatiado por letra, com capa
do TMDB (a mesma busca pontuada do TV Box), favoritos e "continuar de onde parou".

A capa é procurada só para as linhas visíveis: a letra "A" tem 2.672 filmes.
