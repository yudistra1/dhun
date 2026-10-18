//! `dhun-app` — the Dhun desktop application.
//!
//! Top layer (SPEC §6): it owns the [`EditorState`] and the audio [`Playback`],
//! renders the UI with [`dhun_ui::draw`], and turns the UI's [`UiCommand`]s into
//! transport actions on the engine. The UI never talks to the engine directly.

#![forbid(unsafe_code)]

use dhun_io::Playback;
use dhun_ui::{draw, EditorState, UiCommand};
use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([900.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Dhun",
        options,
        Box::new(|cc| {
            dhun_ui::apply_theme(&cc.egui_ctx);
            Ok(Box::new(DhunApp::new()))
        }),
    )
}

/// Application state: the editor plus the live audio session (if any).
struct DhunApp {
    editor: EditorState,
    playback: Option<Playback>,
    error: Option<String>,
}

impl DhunApp {
    fn new() -> Self {
        Self {
            editor: EditorState::demo(),
            playback: None,
            error: None,
        }
    }

    /// (Re)starts playback from the current project so edits take effect.
    fn start_playback(&mut self) {
        match Playback::start(&self.editor.project) {
            Ok(mut playback) => {
                playback.set_looping(self.editor.looping);
                playback.play();
                self.playback = Some(playback);
                self.error = None;
            }
            Err(e) => {
                self.editor.playing = false;
                self.error = Some(e.to_string());
            }
        }
    }

    fn apply(&mut self, command: UiCommand) {
        match command {
            UiCommand::Play => self.start_playback(),
            UiCommand::Stop => {
                if let Some(pb) = &mut self.playback {
                    pb.stop();
                }
            }
            UiCommand::Rewind => {
                if let Some(pb) = &mut self.playback {
                    pb.rewind();
                }
            }
            UiCommand::SetLooping(on) => {
                if let Some(pb) = &mut self.playback {
                    pb.set_looping(on);
                }
            }
            UiCommand::ProjectEdited => {
                // Push the live edit to any running session so it is heard as
                // the loop plays (the engine swaps the program in place).
                if let Some(pb) = self.playback.as_mut() {
                    pb.update_program(&self.editor.project);
                }
            }
        }
    }
}

impl eframe::App for DhunApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        for command in draw(ctx, &mut self.editor) {
            self.apply(command);
        }

        // Reflect natural end-of-song back into the transport state.
        if let Some(pb) = &self.playback {
            if pb.is_finished() {
                self.editor.playing = false;
            }
        }

        if let Some(err) = &self.error {
            egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
                ui.colored_label(egui::Color32::LIGHT_RED, format!("Audio error: {err}"));
            });
        }

        // While playing, keep repainting so the finished state is noticed
        // promptly (and so future meters animate).
        if self.editor.playing {
            ctx.request_repaint();
        }
    }
}
