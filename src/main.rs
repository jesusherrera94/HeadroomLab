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

/// The window icon's artwork. 256×256: large enough for a HiDPI taskbar, small
/// enough that decoding it at startup is not worth measuring.
const ICON_PNG: &[u8] = include_bytes!("../packaging/icon-256.png");

/// Decodes the embedded artwork.
///
/// Deliberately *not* behind a `cfg`, even though only two of the three
/// platforms use the result: code excluded on the machine it is written on is
/// code nobody compiles until it breaks someone else's build. Only the decision
/// to *use* it is platform-specific.
fn decode_icon(bytes: &[u8]) -> Option<eframe::egui::IconData> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().ok()?;
    let mut rgba = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut rgba).ok()?;

    // eframe wants straight RGBA8. The generator writes exactly that, so
    // anything else means the committed artwork was replaced with a different
    // format — better a generic icon than a smear of misread bytes.
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

/// The icon for the **title bar, taskbar and alt-tab** — which is not the
/// launcher icon: on Windows that comes from a resource inside the `.exe` (see
/// `build.rs`) and on Linux from the `.desktop` entry. Both are needed, and
/// neither supplies the other.
///
/// macOS takes both from the `.app` bundle's `CFBundleIconFile`, and winit
/// ignores a window icon there outright, so it is not offered one.
fn app_icon() -> Option<eframe::egui::IconData> {
    if cfg!(target_os = "macos") {
        return None;
    }
    decode_icon(ICON_PNG)
}

fn main() -> eframe::Result<()> {
    // Compose dependencies (the only place that picks concrete impls)
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

    // The one port held as an `Arc` rather than an `Rc`: the updater blocks on
    // network IO, so `UpdateWorker` runs it on a thread of its own.
    let updater: std::sync::Arc<dyn UpdaterPort> = std::sync::Arc::new(GitHubUpdater::new());
    let update_service = UpdateService::new(updater);

    // The root window is the Splash screen.
    let mut viewport = ViewportBuilder::default()
        .with_title("HeadroomLab")
        .with_inner_size([380.0, 240.0])
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

            // Terminal output arrives on the PTY reader thread, so it has to be
            // able to wake the UI. Injected as a bare callback rather than an
            // `egui::Context`, which would put the UI framework inside an
            // infrastructure adapter.
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

    /// The committed artwork has to be what the decoder expects. Replacing
    /// `packaging/icon-256.png` with a palettised or RGB PNG would otherwise
    /// only show up as a missing icon on a platform nobody is testing on.
    #[test]
    fn the_embedded_icon_decodes_to_256_square_rgba() {
        let icon = decode_icon(ICON_PNG).expect("the committed icon should decode");
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        // A fully transparent icon would decode cleanly and show nothing.
        assert!(
            icon.rgba.chunks_exact(4).any(|px| px[3] > 0),
            "the icon is entirely transparent"
        );
    }

    #[test]
    fn a_corrupt_icon_yields_no_icon_rather_than_a_panic() {
        assert!(decode_icon(b"not a png").is_none());
        assert!(decode_icon(&[]).is_none());
    }
}
