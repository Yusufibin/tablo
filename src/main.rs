mod app;
mod load;
mod sheet;
mod theme;
mod ui;

use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1).peekable();
    if let Some(arg) = args.peek() {
        if arg == "--help" || arg == "-h" {
            print_help();
            return;
        }
        if arg == "--version" || arg == "-V" {
            println!("tablo {}", env!("CARGO_PKG_VERSION"));
            return;
        }
    }
    let paths: Vec<PathBuf> = args.map(PathBuf::from).collect();
    if let Err(err) = app::run(paths) {
        eprintln!("tablo : {err}");
        std::process::exit(1);
    }
}

fn print_help() {
    println!(
        "\
tablo {} — lecteur CSV et JSON

Usage:
  tablo [fichier ...]
  tablo -

Ouvre une fenêtre. Glisse un fichier, ou passe-le en argument.

Raccourcis:
  Ctrl+O            Ouvrir
  Ctrl+F            Filtrer
  Ctrl+C            Copier la cellule
  Ctrl+Shift+C      Copier la ligne en JSON
  Ctrl+G            Aller à la ligne
  Ctrl+S            Exporter la vue filtrée
  F5                Recharger
  F1                Aide
",
        env!("CARGO_PKG_VERSION")
    );
}
