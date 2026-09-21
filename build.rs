//! Põe o ícone e a ficha de versão dentro do próprio .exe.
//!
//! Sem isto o Windows desenha o executável com o ícone genérico, o atalho da
//! Área de Trabalho sai em branco, e a aba "Detalhes" das propriedades não diz
//! nem o nome nem a versão do programa — o que um instalador não conserta,
//! porque essas coisas moram dentro do binário.
//!
//! O recurso é montado com o `windres` do mingw, que já é o que compila este
//! alvo. Quem estiver compilando para outro sistema (o desenvolvimento roda no
//! Mac) simplesmente não passa por aqui.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/saimo.ico");
    println!("cargo:rerun-if-changed=build.rs");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let raiz = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let saida = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let versao = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let quatro: Vec<&str> = versao.split('.').collect();
    let (a, b, c) = (
        quatro.first().copied().unwrap_or("0"),
        quatro.get(1).copied().unwrap_or("0"),
        quatro.get(2).copied().unwrap_or("0"),
    );

    let icone = raiz.join("assets/saimo.ico");
    let rc = saida.join("saimo.rc");
    std::fs::write(
        &rc,
        format!(
            r#"1 ICON "{icone}"

1 VERSIONINFO
FILEVERSION {a},{b},{c},0
PRODUCTVERSION {a},{b},{c},0
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "CompanyName", "Saimo"
            VALUE "FileDescription", "Saimo TV"
            VALUE "FileVersion", "{versao}"
            VALUE "InternalName", "saimo-tv"
            VALUE "LegalCopyright", "Saimo"
            VALUE "OriginalFilename", "Saimo TV.exe"
            VALUE "ProductName", "Saimo TV"
            VALUE "ProductVersion", "{versao}"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x409, 1200
    END
END
"#,
            icone = icone.display().to_string().replace('\\', "\\\\"),
        ),
    )
    .expect("não deu para escrever o .rc");

    let objeto = saida.join("saimo-rc.o");
    let windres = std::env::var("WINDRES").unwrap_or_else(|_| "x86_64-w64-mingw32-windres".into());
    let feito = Command::new(&windres)
        .arg(&rc)
        .arg("-O")
        .arg("coff")
        .arg("-o")
        .arg(&objeto)
        .status();

    match feito {
        Ok(saiu) if saiu.success() => {
            println!("cargo:rustc-link-arg-bins={}", objeto.display());
        }
        // Sem o windres o programa compila igual, só sem ícone: melhor um
        // executável sem enfeite que uma compilação que não termina.
        _ => println!("cargo:warning=sem {windres}: o .exe sai sem ícone e sem ficha de versão"),
    }
}
