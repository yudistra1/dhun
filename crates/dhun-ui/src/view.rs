//! The egui views: transport bar, step sequencer, and mixer — styled with the
//! neon-Tokyo [`crate::theme`].

use egui::{Color32, RichText, Rounding, Sense, Stroke};

use dhun_project::{EffectSpec, Instrument};

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
    mixer_panel(ctx, state, &mut commands);
    piano_roll_panel(ctx, state, &mut commands);
    sequencer_panel(ctx, state, &mut commands);

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
                if ui
                    .add(
                        egui::Slider::new(&mut state.project.tempo, 40.0..=240.0)
                            .suffix(" BPM")
                            .trailing_fill(true),
                    )
                    .changed()
                {
                    commands.push(UiCommand::ProjectEdited);
                }
            });
        });
}

fn mixer_panel(ctx: &egui::Context, state: &mut EditorState, commands: &mut Vec<UiCommand>) {
    egui::SidePanel::right("mixer")
        .default_width(250.0)
        .show(ctx, |ui| {
            ui.add_space(6.0);
            ui.label(RichText::new("MIXER").heading().color(theme::CYAN).strong());
            ui.add_space(6.0);

            // Snapshot meter levels up front so the mutable channel loop below
            // does not borrow `state.meters` at the same time.
            let channel_meters: Vec<(f32, bool)> = (0..state.project.channels.len())
                .map(|i| state.meters.channel(i))
                .collect();
            let (master_l, master_r, master_clip) = state.meters.master();

            let mut edited = false;
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
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
                                    ui.label(
                                        RichText::new(&channel.name).color(theme::TEXT).strong(),
                                    );
                                });
                                edited |= ui
                                    .add(
                                        egui::Slider::new(&mut channel.mixer.gain_db, -60.0..=6.0)
                                            .text("dB")
                                            .trailing_fill(true),
                                    )
                                    .changed();
                                edited |= ui
                                    .add(
                                        egui::Slider::new(&mut channel.mixer.pan, -1.0..=1.0)
                                            .text("pan"),
                                    )
                                    .changed();
                                ui.horizontal(|ui| {
                                    edited |=
                                        ui.toggle_value(&mut channel.mixer.mute, "Mute").changed();
                                    edited |=
                                        ui.toggle_value(&mut channel.mixer.solo, "Solo").changed();
                                });
                                let (level, clipped) =
                                    channel_meters.get(i).copied().unwrap_or((0.0, false));
                                meter_bar(ui, level, clipped);
                                ui.push_id(("ch_fx", i), |ui| {
                                    edited |= fx_rack(ui, &mut channel.mixer.effects, accent);
                                });
                            });
                        ui.add_space(8.0);
                    }

                    egui::Frame::none()
                        .fill(theme::SURFACE)
                        .rounding(Rounding::same(10.0))
                        .inner_margin(egui::Margin::same(10.0))
                        .stroke(Stroke::new(1.0_f32, theme::LINE))
                        .show(ui, |ui| {
                            ui.label(RichText::new("MASTER").color(theme::AMBER).strong());
                            edited |= ui
                                .add(
                                    egui::Slider::new(
                                        &mut state.project.master.gain_db,
                                        -60.0..=6.0,
                                    )
                                    .text("dB")
                                    .trailing_fill(true),
                                )
                                .changed();
                            meter_bar(ui, master_l.max(master_r), master_clip);
                            ui.push_id("master_fx", |ui| {
                                edited |=
                                    fx_rack(ui, &mut state.project.master.effects, theme::AMBER);
                            });
                        });
                    ui.add_space(8.0);
                });

            if edited {
                commands.push(UiCommand::ProjectEdited);
            }
        });
}

/// Renders an effect-insert rack for a chain: an add menu, each effect's params,
/// and a remove button. Returns whether anything changed this frame.
fn fx_rack(ui: &mut egui::Ui, effects: &mut Vec<EffectSpec>, accent: Color32) -> bool {
    let mut edited = false;

    ui.horizontal(|ui| {
        ui.label(RichText::new("FX").small().color(theme::TEXT_DIM));
        ui.menu_button("+ add", |ui| {
            for spec in [
                EffectSpec::gain(),
                EffectSpec::eq(),
                EffectSpec::compressor(),
                EffectSpec::reverb(),
            ] {
                if ui.button(spec.label()).clicked() {
                    effects.push(spec);
                    edited = true;
                    ui.close_menu();
                }
            }
        });
    });

    let mut remove = None;
    for (i, fx) in effects.iter_mut().enumerate() {
        ui.push_id(i, |ui| {
            egui::Frame::none()
                .fill(theme::BG)
                .rounding(Rounding::same(6.0))
                .inner_margin(egui::Margin::same(6.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        dot(ui, accent);
                        ui.label(RichText::new(fx.label()).small().color(theme::TEXT));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✕").on_hover_text("Remove").clicked() {
                                remove = Some(i);
                            }
                        });
                    });
                    edited |= fx_params(ui, fx);
                });
        });
        ui.add_space(4.0);
    }
    if let Some(i) = remove {
        effects.remove(i);
        edited = true;
    }
    edited
}

/// Parameter sliders for one effect. Returns whether a value changed.
fn fx_params(ui: &mut egui::Ui, fx: &mut EffectSpec) -> bool {
    let mut edited = false;
    let mut slider = |ui: &mut egui::Ui, value: &mut f32, range, text: &str| {
        edited |= ui.add(egui::Slider::new(value, range).text(text)).changed();
    };
    match fx {
        EffectSpec::Gain { gain_db } => slider(ui, gain_db, -24.0..=24.0, "dB"),
        EffectSpec::Eq {
            low_db,
            mid_db,
            high_db,
        } => {
            slider(ui, low_db, -18.0..=18.0, "low");
            slider(ui, mid_db, -18.0..=18.0, "mid");
            slider(ui, high_db, -18.0..=18.0, "high");
        }
        EffectSpec::Compressor {
            threshold_db,
            ratio,
            attack,
            release,
            makeup_db,
        } => {
            slider(ui, threshold_db, -60.0..=0.0, "thr");
            slider(ui, ratio, 1.0..=20.0, "ratio");
            slider(ui, attack, 0.0..=0.1, "atk");
            slider(ui, release, 0.01..=1.0, "rel");
            slider(ui, makeup_db, 0.0..=24.0, "gain");
        }
        EffectSpec::Reverb {
            room_size,
            damping,
            wet,
            dry,
        } => {
            slider(ui, room_size, 0.0..=1.0, "size");
            slider(ui, damping, 0.0..=1.0, "damp");
            slider(ui, wet, 0.0..=1.0, "wet");
            slider(ui, dry, 0.0..=1.0, "dry");
        }
    }
    edited
}

fn sequencer_panel(ctx: &egui::Context, state: &mut EditorState, commands: &mut Vec<UiCommand>) {
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
                    commands.push(UiCommand::ProjectEdited);
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
                        commands.push(UiCommand::ProjectEdited);
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

fn piano_roll_panel(ctx: &egui::Context, state: &mut EditorState, commands: &mut Vec<UiCommand>) {
    let Some(channel) = state.first_synth_channel() else {
        return;
    };
    let accent = state
        .project
        .channels
        .iter()
        .position(|c| c.id == channel)
        .map_or(theme::LIME, theme::channel_accent);
    let name = state
        .project
        .channel(channel)
        .map(|c| c.name.clone())
        .unwrap_or_default();
    let steps = state.current_pattern().map_or(0, |p| p.steps) as usize;

    // Two octaves, C3..=C5, drawn high note at the top.
    const LO: u8 = 48;
    const HI: u8 = 72;

    egui::TopBottomPanel::bottom("piano_roll")
        .resizable(true)
        .default_height(230.0)
        .show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("PIANO ROLL")
                        .heading()
                        .color(theme::LIME)
                        .strong(),
                );
                ui.label(RichText::new(format!("· {name}")).color(theme::TEXT_DIM));
                if ui
                    .button("Clear")
                    .on_hover_text("Remove all notes from this lane")
                    .clicked()
                {
                    state.clear_notes(channel);
                    commands.push(UiCommand::ProjectEdited);
                }
            });
            ui.add_space(6.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for pitch in (LO..=HI).rev() {
                        let black = is_black_key(pitch);
                        ui.horizontal(|ui| {
                            let col = if black { theme::TEXT_DIM } else { theme::TEXT };
                            ui.add_sized(
                                [48.0, 20.0],
                                egui::Label::new(
                                    RichText::new(note_name(pitch)).small().color(col),
                                ),
                            );
                            for step in 0..steps {
                                if step > 0 && step % 4 == 0 {
                                    ui.add_space(6.0);
                                }
                                let on = state.note_at(channel, pitch, step);
                                if note_cell(ui, on, accent, black)
                                    .on_hover_text(format!(
                                        "{} · step {}",
                                        note_name(pitch),
                                        step + 1
                                    ))
                                    .clicked()
                                {
                                    state.toggle_note(channel, pitch, step);
                                    commands.push(UiCommand::ProjectEdited);
                                }
                            }
                        });
                    }
                });
        });
}

// --- Neon widgets --------------------------------------------------------

/// A glowing grid cell: lit with the accent + halo when `on`, else a dark tile
/// (`bg_off`) with a hover outline. Shared by the sequencer and piano roll.
fn cell(
    ui: &mut egui::Ui,
    on: bool,
    accent: Color32,
    bg_off: Color32,
    size: [f32; 2],
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size.into(), Sense::click());
    let painter = ui.painter();
    let rounding = Rounding::same(5.0);

    if on {
        painter.rect_filled(
            rect.expand(2.0_f32),
            Rounding::same(8.0),
            theme::glow(accent),
        );
        painter.rect_filled(rect, rounding, accent);
        painter.rect_stroke(
            rect,
            rounding,
            Stroke::new(1.5_f32, Color32::from_white_alpha(140)),
        );
    } else {
        painter.rect_filled(rect, rounding, bg_off);
        let stroke = if resp.hovered() {
            Stroke::new(1.5_f32, accent)
        } else {
            Stroke::new(1.0_f32, theme::LINE)
        };
        painter.rect_stroke(rect, rounding, stroke);
    }
    resp
}

/// A horizontal level meter (green → amber → red) with a clip indicator.
fn meter_bar(ui: &mut egui::Ui, level: f32, clipped: bool) {
    let w = ui.available_width().clamp(60.0, 190.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 8.0), Sense::hover());
    let painter = ui.painter();
    let rounding = Rounding::same(3.0);
    painter.rect_filled(rect, rounding, theme::BG);

    // sqrt gives quiet signals more visible travel.
    let d = level.clamp(0.0, 1.0).sqrt();
    if d > 0.0 {
        let fill = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * d, rect.height()));
        let col = if d < 0.6 {
            theme::LIME
        } else if d < 0.85 {
            theme::AMBER
        } else {
            theme::MAGENTA
        };
        painter.rect_filled(fill, rounding, col);
    }
    painter.rect_stroke(rect, rounding, Stroke::new(1.0_f32, theme::LINE));

    if clipped {
        let s = 6.0_f32;
        let dot = egui::Rect::from_min_size(
            egui::pos2(rect.right() - s, rect.top() + 1.0),
            egui::vec2(s - 1.0, rect.height() - 2.0),
        );
        painter.rect_filled(dot, Rounding::same(2.0), theme::MAGENTA);
    }
}

/// A glowing sequencer step cell. Returns the click response.
fn step_cell(ui: &mut egui::Ui, on: bool, accent: Color32, downbeat: bool) -> egui::Response {
    let bg = if downbeat { theme::SURFACE } else { theme::BG };
    cell(ui, on, accent, bg, [26.0, 30.0])
}

/// A piano-roll note cell; black-key rows get a slightly lighter off tile.
fn note_cell(ui: &mut egui::Ui, on: bool, accent: Color32, black_key: bool) -> egui::Response {
    let bg = if black_key { theme::SURFACE } else { theme::BG };
    cell(ui, on, accent, bg, [24.0, 20.0])
}

/// The MIDI note name for a pitch, e.g. 60 -> "C4".
fn note_name(pitch: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let name = NAMES[(pitch % 12) as usize];
    let octave = i32::from(pitch) / 12 - 1;
    format!("{name}{octave}")
}

/// Whether a pitch is a black key (sharp).
fn is_black_key(pitch: u8) -> bool {
    matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
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
