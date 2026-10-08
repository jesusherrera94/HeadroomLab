#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::cell::RefCell;
use std::rc::Rc;

use eframe::egui::ViewportBuilder;

use HeadroomLab::{
    application::{
        file_system_service::FileSystemService,
        graph_service::GraphService,
        ports::{
            AudioEnginePort, ClipboardPort, DoomPort, FileWatcherPort, ProjectFileSystemPort,
            ProjectGeneratorPort, TerminalPort, UpdaterPort,
        },
        recent_projects_service::RecentProjectsService,
        simulator_service::SimulatorService,
        update_service::UpdateService,
    },
    infrastructure::{
        audio_engine::AudioEngine, doom_engine::NeurodoomEngine, github_updater::GitHubUpdater,
        notify_file_watcher::NotifyFileWatcher, project_generator::TemplateProjectGenerator,
        pty_terminal::PtyTerminal, recent_projects_store::FileRecentProjectsStore,
        std_fs_project_file_system::StdFsProjectFileSystem, system_clipboard::SystemClipboard,
    },
    presentation::{app_controller::HeadroomApp, theme},
};

const ICON_PNG: &[u8] = include_bytes!("../packaging/icon-256.png");

const ICON_PNG_1024: &[u8] = include_bytes!("../packaging/icon.png");

fn decode_icon(bytes: &[u8]) -> Option<eframe::egui::IconData> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().ok()?;
    let mut rgba: Vec<u8> = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut rgba).ok()?;

    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        eprintln!("[icon] packaging/icon-256.png is not RGBA8; falling back to no icon");
        return None;
    }

    rgba.truncate(info.buffer_size());
    Some(eframe::egui::IconData {
        rgba,
        width: info.width,
        height: info.height,
    })
}

fn app_icon() -> Option<eframe::egui::IconData> {
    decode_icon(if cfg!(target_os = "macos") {
        ICON_PNG_1024
    } else {
        ICON_PNG
    })
}

fn main() -> eframe::Result<()> {
    let audio_engine: Rc<dyn AudioEnginePort> = Rc::new(AudioEngine::new());
    let sim_service = Rc::new(SimulatorService::new(audio_engine.clone()));
    let graph_service = Rc::new(GraphService::new(audio_engine.clone()));

    let recents_store = Rc::new(FileRecentProjectsStore::new());
    let recents = Rc::new(RefCell::new(RecentProjectsService::new(recents_store)));

    let generator: Rc<dyn ProjectGeneratorPort> = Rc::new(TemplateProjectGenerator::new());

    let file_system: Rc<dyn ProjectFileSystemPort> = Rc::new(StdFsProjectFileSystem::new());
    let fs_service = Rc::new(FileSystemService::new(file_system.clone()));
    let file_watcher: Rc<dyn FileWatcherPort> = Rc::new(NotifyFileWatcher::new());
    let clipboard: Rc<dyn ClipboardPort> = Rc::new(SystemClipboard::new());
    let doom: Rc<dyn DoomPort> = Rc::new(NeurodoomEngine::new());

    let updater: std::sync::Arc<dyn UpdaterPort> = std::sync::Arc::new(GitHubUpdater::new());
    let update_service = UpdateService::new(updater);

    let mut viewport = ViewportBuilder::default()
        .with_title("HeadroomLab")
        .with_inner_size([380.0, 280.0])
        .with_resizable(false);
    if let Some(icon) = app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "HeadroomLab",
        options,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            theme::install_icon_font(&cc.egui_ctx);

            let ctx = cc.egui_ctx.clone();
            let terminal: Rc<dyn TerminalPort> =
                Rc::new(PtyTerminal::new(std::sync::Arc::new(move || {
                    ctx.request_repaint();
                })));

            Ok(Box::new(HeadroomApp::new(
                sim_service,
                graph_service,
                recents,
                generator,
                file_system,
                fs_service,
                file_watcher,
                clipboard,
                terminal,
                doom,
                update_service,
            )))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_icon_decodes_to_256_square_rgba() {
        let icon = decode_icon(ICON_PNG).expect("the committed icon should decode");
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        assert!(
            icon.rgba.chunks_exact(4).any(|px| px[3] > 0),
            "the icon is entirely transparent"
        );
    }

    #[test]
    fn the_embedded_master_decodes_to_1024_square_rgba() {
        let icon = decode_icon(ICON_PNG_1024).expect("the committed master should decode");
        assert_eq!((icon.width, icon.height), (1024, 1024));
        assert_eq!(icon.rgba.len(), 1024 * 1024 * 4);
        assert!(
            icon.rgba.chunks_exact(4).any(|px| px[3] > 0),
            "the master icon is entirely transparent"
        );
    }

    #[test]
    fn a_corrupt_icon_yields_no_icon_rather_than_a_panic() {
        assert!(decode_icon(b"not a png").is_none());
        assert!(decode_icon(&[]).is_none());
    }
}
