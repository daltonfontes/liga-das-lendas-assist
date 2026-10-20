// Sem janela de console no Windows em builds de release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    liga_das_lendas_assist_lib::run();
}
