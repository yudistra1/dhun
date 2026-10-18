//! The egui views: transport bar, step sequencer, and mixer.

use dhun_project::Instrument;

use crate::command::UiCommand;
use crate::model::EditorState;

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
    egui::TopBottomPanel::top("transport").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Dhun");
            ui.separator();

            let play_label = if state.playing {
                "⏹ Stop"
            } else {
                "▶ Play"
            };
            if ui.button(play_label).clicked() {
                if state.playing {
                    state.playing = false;
                    commands.push(UiCommand::Stop);
                } else {
                    state.playing = true;
                    commands.push(UiCommand::Play);
                }
            }
            if ui.button("⏮ Rewind").clicked() {
                commands.push(UiCommand::Rewind);
            }

            let mut looping = state.looping;
            if ui.checkbox(&mut looping, "Loop").changed() {
                state.looping = looping;
                commands.push(UiCommand::SetLooping(looping));
            }

            ui.separator();
            ui.label("Tempo");
            ui.add(egui::Slider::new(&mut state.project.tempo, 40.0..=240.0).suffix(" BPM"));
        });
    });
}

fn mixer_panel(ctx: &egui::Context, state: &mut EditorState) {
    egui::SidePanel::right("mixer")
        .default_width(220.0)
        .show(ctx, |ui| {
            ui.heading("Mixer");
            for channel in &mut state.project.channels {
                ui.group(|ui| {
                    ui.label(&channel.name);
                    ui.add(
                        egui::Slider::new(&mut channel.mixer.gain_db, -60.0..=6.0)
                            .text("dB")
                            .clamping(egui::SliderClamping::Always),
                    );
                    ui.add(egui::Slider::new(&mut channel.mixer.pan, -1.0..=1.0).text("pan"));
                    ui.horizontal(|ui| {
                        ui.toggle_value(&mut channel.mixer.mute, "Mute");
                        ui.toggle_value(&mut channel.mixer.solo, "Solo");
                    });
                });
            }
            ui.separator();
            ui.label("Master");
            ui.add(egui::Slider::new(&mut state.project.master.gain_db, -60.0..=6.0).text("dB"));
        });
}

fn sequencer_panel(ctx: &egui::Context, state: &mut EditorState) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("Step Sequencer");
        ui.add_space(4.0);

        let steps = state.current_pattern().map_or(0, |p| p.steps) as usize;
        // Snapshot the drum channels so we can mutate steps while iterating.
        let drum_channels: Vec<(u64, String)> = state
            .project
            .channels
            .iter()
            .filter(|c| matches!(c.instrument, Instrument::Drum { .. }))
            .map(|c| (c.id, c.name.clone()))
            .collect();

        for (id, name) in &drum_channels {
            ui.horizontal(|ui| {
                ui.add_sized([96.0, 24.0], egui::Label::new(name.clone()));
                for step in 0..steps {
                    let on = state.step_on(*id, step);
                    // A subtle gap every 4 steps marks the beats.
                    if step > 0 && step % 4 == 0 {
                        ui.add_space(6.0);
                    }
                    if ui
                        .add_sized([24.0, 24.0], egui::SelectableLabel::new(on, ""))
                        .clicked()
                    {
                        state.toggle_step(*id, step);
                    }
                }
            });
        }

        if drum_channels.is_empty() {
            ui.label("No drum channels. (Synth lanes are edited in the piano roll — M6.)");
        }
    });
}
