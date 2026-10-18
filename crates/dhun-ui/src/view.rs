//! The egui views: transport bar, step sequencer, and mixer — styled with the
//! neon-Tokyo [`crate::theme`].

use egui::{Color32, RichText, Rounding, Sense, Stroke};

use dhun_project::Instrument;

use crate::command::UiCommand;
use crate::model::{EditorState, Preset};
use crate::theme;

/// Draws the whole editor for one frame and returns any commands the user's
/// interactions produced.
///
/// The UI reads and edits `state` directly (patterns, mixer, tempo) but only
/// *communicates* with the engine through the returned [`UiCommand`]s (SPEC §6).
pub fn draw(ctx: &egui::Context, state: &mut EditorState) -> Vec<UiCommand> {
    let mut commands = Vec::new();

    transport_bar(ctx, state, &mut commands);
    mixer_panel(ctx, state);
    sequencer_panel(ctx, state);

    commands
}

fn transport_bar(ctx: &egui::Context, state: &mut EditorState, commands: &mut Vec<UiCommand>) {
    egui::TopBottomPanel::top("transport")
        .exact_height(56.0)
        .show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.label(
                    RichText::new("◆ DHUN")
                        .heading()
                        .color(theme::MAGENTA)
                        .strong(),
                );
                ui.add_space(6.0);
                neon_separator(ui);

                let (label, fill, glow_c) = if state.playing {
                    ("⏹  STOP", theme::MAGENTA, theme::MAGENTA)
                } else {
                    ("▶  PLAY", theme::CYAN, theme::CYAN)
                };
                if neon_button(ui, label, fill, glow_c).clicked() {
                    if state.playing {
                        state.playing = false;
                        commands.push(UiCommand::Stop);
                    } else {
                        state.playing = true;
                        commands.push(UiCommand::Play);
                    }
                }

                if ui
                    .button(RichText::new("⏮  Rewind").color(theme::TEXT))
                    .clicked()
                {
                    commands.push(UiCommand::Rewind);
                }

                let mut looping = state.looping;
                if ui.toggle_value(&mut looping, "⟲  Loop").changed() {
                    state.looping = looping;
                    commands.push(UiCommand::SetLooping(looping));
                }

                neon_separator(ui);
                ui.label(RichText::new("TEMPO").small().color(theme::TEXT_DIM));
                ui.add(
                    egui::Slider::new(&mut state.project.tempo, 40.0..=240.0)
                        .suffix(" BPM")
                        .trailing_fill(true),
                );
            });
        });
}

fn mixer_panel(ctx: &egui::Context, state: &mut EditorState) {
    egui::SidePanel::right("mixer")
        .default_width(240.0)
        .show(ctx, |ui| {
            ui.add_space(6.0);
            ui.label(RichText::new("MIXER").heading().color(theme::CYAN).strong());
            ui.add_space(6.0);

            for (i, channel) in state.project.channels.iter_mut().enumerate() {
                let accent = theme::channel_accent(i);
                egui::Frame::none()
                    .fill(theme::SURFACE)
                    .rounding(Rounding::same(10.0))
                    .inner_margin(egui::Margin::same(10.0))
                    .stroke(Stroke::new(1.0_f32, theme::LINE))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            dot(ui, accent);
                            ui.label(RichText::new(&channel.name).color(theme::TEXT).strong());
                        });
                        ui.add(
                            egui::Slider::new(&mut channel.mixer.gain_db, -60.0..=6.0)
                                .text("dB")
                                .trailing_fill(true),
                        );
                        ui.add(egui::Slider::new(&mut channel.mixer.pan, -1.0..=1.0).text("pan"));
                        ui.horizontal(|ui| {
                            ui.toggle_value(&mut channel.mixer.mute, "Mute");
                            ui.toggle_value(&mut channel.mixer.solo, "Solo");
                        });
                    });
                ui.add_space(8.0);
            }

            ui.separator();
            ui.label(RichText::new("MASTER").color(theme::AMBER).strong());
            ui.add(
                egui::Slider::new(&mut state.project.master.gain_db, -60.0..=6.0)
                    .text("dB")
                    .trailing_fill(true),
            );
        });
}

fn sequencer_panel(ctx: &egui::Context, state: &mut EditorState) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add_space(4.0);
        ui.label(
            RichText::new("STEP SEQUENCER")
                .heading()
                .color(theme::PURPLE)
                .strong(),
        );
        ui.add_space(6.0);

        // Preset row.
        ui.horizontal(|ui| {
            ui.label(RichText::new("PRESETS").small().color(theme::TEXT_DIM));
            ui.add_space(4.0);
            for preset in Preset::ALL {
                if ui
                    .button(preset.label())
                    .on_hover_text("Load this groove into the current pattern")
                    .clicked()
                {
                    state.apply_preset(preset);
                }
            }
        });
        ui.add_space(12.0);

        let steps = state.current_pattern().map_or(0, |p| p.steps) as usize;
        // Snapshot the drum channels so we can mutate steps while iterating.
        let drum_channels: Vec<(u64, String)> = state
            .project
            .channels
            .iter()
            .filter(|c| matches!(c.instrument, Instrument::Drum { .. }))
            .map(|c| (c.id, c.name.clone()))
            .collect();

        // Step-number ruler so beats are easy to count.
        if !drum_channels.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(116.0); // align past the channel label column
                for step in 0..steps {
                    if step > 0 && step % 4 == 0 {
                        ui.add_space(8.0);
                    }
                    let strong = step % 4 == 0;
                    let color = if strong { theme::TEXT } else { theme::TEXT_DIM };
                    ui.add_sized(
                        [26.0, 14.0],
                        egui::Label::new(
                            RichText::new(format!("{}", step + 1)).small().color(color),
                        ),
                    );
                }
            });
            ui.add_space(4.0);
        }

        for (row, (id, name)) in drum_channels.iter().enumerate() {
            let accent = theme::channel_accent(row);
            ui.horizontal(|ui| {
                ui.horizontal(|ui| {
                    dot(ui, accent);
                    ui.add_sized(
                        [92.0, 28.0],
                        egui::Label::new(RichText::new(name).color(theme::TEXT)),
                    );
                });
                for step in 0..steps {
                    if step > 0 && step % 4 == 0 {
                        ui.add_space(8.0);
                    }
                    let on = state.step_on(*id, step);
                    if step_cell(ui, on, accent, step % 4 == 0)
                        .on_hover_text(format!("{name} · step {}", step + 1))
                        .clicked()
                    {
                        state.toggle_step(*id, step);
                    }
                }
            });
            ui.add_space(6.0);
        }

        if drum_channels.is_empty() {
            ui.label(
                RichText::new("No drum channels. (Synth lanes live in the piano roll — M6.)")
                    .color(theme::TEXT_DIM),
            );
        }
    });
}

// --- Neon widgets --------------------------------------------------------

/// A glowing sequencer step cell. Returns the click response.
fn step_cell(ui: &mut egui::Ui, on: bool, accent: Color32, downbeat: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(26.0, 30.0), Sense::click());
    let painter = ui.painter();
    let rounding = Rounding::same(6.0);

    if on {
        // Glow halo behind the lit cell.
        painter.rect_filled(
            rect.expand(3.0_f32),
            Rounding::same(9.0),
            theme::glow(accent),
        );
        painter.rect_filled(rect, rounding, accent);
        painter.rect_stroke(
            rect,
            rounding,
            Stroke::new(1.5_f32, Color32::from_white_alpha(150)),
        );
    } else {
        let fill = if downbeat { theme::SURFACE } else { theme::BG };
        painter.rect_filled(rect, rounding, fill);
        let stroke = if resp.hovered() {
            Stroke::new(1.5_f32, accent)
        } else {
            Stroke::new(1.0_f32, theme::LINE)
        };
        painter.rect_stroke(rect, rounding, stroke);
    }
    resp
}

/// A filled, glowing action button (used for Play/Stop).
fn neon_button(ui: &mut egui::Ui, text: &str, fill: Color32, glow_c: Color32) -> egui::Response {
    let button = egui::Button::new(RichText::new(text).color(theme::BG).strong())
        .fill(fill)
        .rounding(Rounding::same(8.0));
    let resp = ui.add(button);
    if resp.hovered() {
        ui.painter().rect_stroke(
            resp.rect.expand(2.0_f32),
            Rounding::same(10.0),
            Stroke::new(2.0_f32, theme::glow(glow_c)),
        );
    }
    resp
}

/// A small filled accent dot.
fn dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 5.0_f32, color);
    ui.painter().circle_stroke(
        rect.center(),
        5.0_f32,
        Stroke::new(1.0_f32, theme::glow(color)),
    );
}

/// A faint vertical separator sized for the transport bar.
fn neon_separator(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, 24.0), Sense::hover());
    ui.painter().rect_filled(rect, Rounding::ZERO, theme::LINE);
    ui.add_space(4.0);
}
