// Impede que uma janela de console apareca junto do app no Windows em release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use instalador_harmonika_karaoke_lib as instalador;

fn main() {
    // Modo silencioso: instala sem abrir janela, para implantar em varias
    // maquinas de uma vez. Sem argumento, abre a janela normal.
    if std::env::args().skip(1).any(|a| a == "--instalar") {
        std::process::exit(instalador::instalar_cli());
    }
    instalador::run()
}
