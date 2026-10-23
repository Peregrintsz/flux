#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
    time::Duration,
};

use eframe::egui::{self, Align, Color32, Layout, RichText, Stroke, Vec2};
use enigo::{Button, Direction, Enigo, Mouse, Settings};
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{HotKey, Modifiers},
};

const GREEN: Color32 = Color32::from_rgb(39, 133, 87);

#[derive(Clone, Copy, PartialEq)]
enum Theme {
    Dark,
    Light,
}

#[derive(Clone, Copy)]
struct Palette {
    bg: Color32,
    panel: Color32,
    panel_light: Color32,
    accent: Color32,
    muted: Color32,
    border: Color32,
    text: Color32,
}

impl Theme {
    fn palette(self) -> Palette {
        match self {
            Self::Dark => Palette {
                bg: Color32::from_rgb(24, 25, 27),
                panel: Color32::from_rgb(34, 35, 38),
                panel_light: Color32::from_rgb(46, 48, 52),
                accent: Color32::from_rgb(91, 98, 108),
                muted: Color32::from_rgb(168, 170, 176),
                border: Color32::from_rgb(59, 61, 66),
                text: Color32::from_rgb(218, 219, 222),
            },
            Self::Light => Palette {
                bg: Color32::from_rgb(234, 235, 237),
                panel: Color32::from_rgb(243, 244, 245),
                panel_light: Color32::from_rgb(226, 228, 231),
                accent: Color32::from_rgb(61, 66, 73),
                muted: Color32::from_rgb(87, 92, 100),
                border: Color32::from_rgb(210, 212, 216),
                text: Color32::from_rgb(43, 46, 51),
            },
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ClickButton {
    Left,
    Right,
    Middle,
}

impl ClickButton {
    fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Middle => "Middle",
        }
    }

    fn enigo(self) -> Button {
        match self {
            Self::Left => Button::Left,
            Self::Right => Button::Right,
            Self::Middle => Button::Middle,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SpeedMode {
    Cps,
    Milliseconds,
}

#[derive(Clone, Copy, PartialEq)]
enum ActivationMode {
    Toggle,
    Hold,
}

impl ActivationMode {
    fn hotkey_enabled(self, enabled: bool, down: &mut bool, state: HotKeyState) -> bool {
        let pressed = state == HotKeyState::Pressed;
        let next = match self {
            Self::Toggle if pressed && !*down => !enabled,
            Self::Toggle => enabled,
            Self::Hold => pressed,
        };
        *down = pressed;
        next
    }
}

#[derive(Clone, Copy, PartialEq)]
struct ClickSettings {
    button: ClickButton,
    interval: Duration,
}

enum WorkerCommand {
    SetEnabled(bool),
    Update(ClickSettings),
}

struct ClickerApp {
    theme: Theme,
    activation_mode: ActivationMode,
    shortcut_down: bool,
    waiting_for_release: bool,
    button: ClickButton,
    mode: SpeedMode,
    cps: u32,
    milliseconds: u32,
    hotkey: HotKey,
    hotkey_label: String,
    capturing_hotkey: bool,
    enabled: bool,
    clicks: Arc<AtomicU64>,
    last_settings: ClickSettings,
    worker_tx: Sender<WorkerCommand>,
    hotkey_manager: GlobalHotKeyManager,
    hotkey_id: u32,
    error: Option<String>,
}

impl ClickerApp {
    fn new() -> Self {
        let (worker_tx, worker_rx) = mpsc::channel();
        let clicks = Arc::new(AtomicU64::new(0));
        let worker_clicks = Arc::clone(&clicks);
        thread::Builder::new()
            .name("flux-clicker".into())
            .spawn(move || click_worker(worker_rx, worker_clicks))
            .expect("Could not start click worker thread");

        let hotkey = HotKey::try_from("F6").expect("F6 is a valid hotkey");
        let hotkey_manager = GlobalHotKeyManager::new().expect("Could not initialize hotkey manager");
        let error = hotkey_manager
            .register(hotkey)
            .err()
            .map(|e| format!("Could not register hotkey: {e}"));

        let settings = ClickSettings {
            button: ClickButton::Left,
            interval: Duration::from_millis(100),
        };

        Self {
            theme: Theme::Dark,
            activation_mode: ActivationMode::Toggle,
            shortcut_down: false,
            waiting_for_release: false,
            button: ClickButton::Left,
            mode: SpeedMode::Cps,
            cps: 10,
            milliseconds: 100,
            hotkey,
            hotkey_label: "F6".to_owned(),
            capturing_hotkey: false,
            enabled: false,
            clicks,
            last_settings: settings,
            worker_tx,
            hotkey_manager,
            hotkey_id: hotkey.id(),
            error,
        }
    }

    fn interval(&self) -> Duration {
        let millis = match self.mode {
            SpeedMode::Cps => 1000u64.div_ceil(u64::from(self.cps.max(1))).max(1) as u32,
            SpeedMode::Milliseconds => self.milliseconds.max(1),
        };
        Duration::from_millis(millis as u64)
    }

    fn click_settings(&self) -> ClickSettings {
        ClickSettings {
            button: self.button,
            interval: self.interval(),
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        if self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        let _ = self.worker_tx.send(WorkerCommand::SetEnabled(enabled));
    }

    fn register_hotkey(&mut self, next_hotkey: HotKey, label: String) {
        self.waiting_for_release = true;
        if self.hotkey == next_hotkey {
            self.hotkey_label = label;
            self.capturing_hotkey = false;
            return;
        }
        if let Err(error) = self.hotkey_manager.register(next_hotkey) {
            self.error = Some(format!("Could not register shortcut: {error}"));
            self.capturing_hotkey = false;
            return;
        }
        let _ = self.hotkey_manager.unregister(self.hotkey);
        self.hotkey = next_hotkey;
        self.hotkey_label = label;
        self.hotkey_id = next_hotkey.id();
        self.capturing_hotkey = false;
        self.error = None;
    }
}

impl eframe::App for ClickerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let previous_theme = self.theme;
        if self.capturing_hotkey || self.waiting_for_release {
            let captured = ctx.input(|input| {
                input.events.iter().find_map(|event| match event {
                    egui::Event::Key { key, physical_key, pressed: true, repeat: false, modifiers } => {
                        make_hotkey(physical_key.unwrap_or(*key), *modifiers)
                    }
                    _ => None,
                })
            });
            if self.capturing_hotkey {
                if let Some((hotkey, label)) = captured {
                    self.register_hotkey(hotkey, label);
                }
            }
            while GlobalHotKeyEvent::receiver().try_recv().is_ok() {}
            if self.waiting_for_release && ctx.input(|input| input.keys_down.is_empty()) {
                self.waiting_for_release = false;
            }
        } else {
            while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                if event.id == self.hotkey_id {
                    let enabled = self.activation_mode.hotkey_enabled(
                        self.enabled, &mut self.shortcut_down, event.state,
                    );
                    self.set_enabled(enabled);
                }
            }
        }

        let palette = self.theme.palette();
        ui.painter().rect_filled(ui.max_rect(), 0.0, palette.bg);
        egui::Frame::new()
            .fill(palette.bg)
            .inner_margin(egui::Margin::same(22))
            .show(ui, |ui| {
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    self.top_bar(ui, palette);
                    ui.add_space(14.0);
                    self.status_card(ui);
                    ui.add_space(17.0);
                    self.section_title(ui, "Mouse button");
                    ui.add_space(7.0);
                    self.click_options(ui);
                    ui.add_space(16.0);
                    self.section_title(ui, "Click speed");
                    ui.add_space(7.0);
                    self.speed_options(ui);
                    ui.add_space(16.0);
                    self.section_title(ui, "Activation");
                    ui.add_space(7.0);
                    self.hotkey_options(ui);
                    ui.add_space(18.0);
                    self.start_button(ui);
                    if let Some(error) = &self.error {
                        ui.add_space(8.0);
                        ui.label(RichText::new(error).size(12.0).color(Color32::from_rgb(220, 90, 90)));
                    }
                });
            });

        if self.theme != previous_theme {
            ctx.set_visuals(visuals_for(self.theme));
        }

        let settings = self.click_settings();
        if settings != self.last_settings {
            let _ = self.worker_tx.send(WorkerCommand::Update(settings));
            self.last_settings = settings;
        }
        ctx.request_repaint_after(Duration::from_millis(40));
    }
}

impl ClickerApp {
    fn top_bar(&mut self, ui: &mut egui::Ui, palette: Palette) {
        ui.horizontal(|ui| {
            draw_flux_mark(ui, 30.0);
            ui.add_space(8.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let next_theme = if self.theme == Theme::Dark { "Switch to Light" } else { "Switch to Dark" };
                if ui.add(
                    egui::Button::new(RichText::new(next_theme).size(12.0).color(palette.text))
                        .fill(palette.panel_light)
                        .stroke(Stroke::new(1.0, palette.border))
                        .corner_radius(6.0),
                ).clicked() {
                    self.theme = if self.theme == Theme::Dark { Theme::Light } else { Theme::Dark };
                }
            });
        });
    }

    fn status_card(&self, ui: &mut egui::Ui) {
        let palette = self.theme.palette();
        egui::Frame::new()
            .fill(palette.panel)
            .stroke(Stroke::new(1.0, palette.border))
            .corner_radius(8.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let dot = if self.enabled { GREEN } else { palette.muted };
                    let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
                    ui.painter().circle_filled(dot_rect.center(), 4.0, dot);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(if self.enabled { "Running" } else { "Ready" }).strong().color(palette.text));
                        ui.label(RichText::new(format!("{} clicks", self.clicks.load(Ordering::Relaxed))).size(12.0).color(palette.muted));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(format!("{} ms", self.interval().as_millis())).color(palette.muted));
                    });
                });
            });
    }

    fn section_title(&self, ui: &mut egui::Ui, title: &str) {
        ui.label(RichText::new(title).size(14.0).strong().color(self.theme.palette().text));
    }

    fn click_options(&mut self, ui: &mut egui::Ui) {
        let palette = self.theme.palette();
        egui::Frame::new().fill(palette.panel).stroke(Stroke::new(1.0, palette.border)).corner_radius(8.0).inner_margin(5.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                let item_width = (ui.available_width() - 12.0) / 3.0;
                for value in [ClickButton::Left, ClickButton::Right, ClickButton::Middle] {
                    let selected = self.button == value;
                    let response = ui.add_sized(
                        Vec2::new(item_width, 40.0),
                        egui::Button::new(RichText::new(value.label()).size(13.0).color(if selected { Color32::WHITE } else { palette.text }))
                            .fill(if selected { palette.accent } else { palette.panel })
                            .stroke(Stroke::new(1.0, if selected { palette.accent } else { palette.border }))
                            .corner_radius(6.0),
                    );
                    if response.clicked() { self.button = value; }
                }
            });
        });
    }

    fn speed_options(&mut self, ui: &mut egui::Ui) {
        let palette = self.theme.palette();
        egui::Frame::new().fill(palette.panel).stroke(Stroke::new(1.0, palette.border)).corner_radius(8.0).inner_margin(10.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                let tab_width = (ui.available_width() - 8.0) / 2.0;
                for (value, label) in [(SpeedMode::Cps, "CPS"), (SpeedMode::Milliseconds, "Milliseconds")] {
                    let selected = self.mode == value;
                    if ui.add_sized(Vec2::new(tab_width, 30.0), egui::Button::new(RichText::new(label).size(13.0).color(if selected { Color32::WHITE } else { palette.text })).fill(if selected { palette.accent } else { palette.panel_light }).corner_radius(6.0)).clicked() {
                        self.mode = value;
                    }
                }
            });
            ui.add_space(11.0);
            ui.horizontal(|ui| {
                let (label, suffix, value) = match self.mode {
                    SpeedMode::Cps => ("Speed", "clicks / sec", &mut self.cps),
                    SpeedMode::Milliseconds => ("Interval", "ms", &mut self.milliseconds),
                };
                ui.label(RichText::new(label).size(13.0).color(palette.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(suffix).size(11.0).color(palette.muted));
                    ui.add(egui::DragValue::new(value).speed(1).min_decimals(0).max_decimals(0));
                });
            });
            self.cps = self.cps.max(1);
            self.milliseconds = self.milliseconds.max(1);
        });
    }

    fn hotkey_options(&mut self, ui: &mut egui::Ui) {
        let palette = self.theme.palette();
        egui::Frame::new().fill(palette.panel).stroke(Stroke::new(1.0, palette.border)).corner_radius(8.0).inner_margin(10.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                let width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
                for (mode, label, hint) in [
                    (ActivationMode::Toggle, "Toggle", "Press the shortcut to start or stop."),
                    (ActivationMode::Hold, "Hold", "Click only while the shortcut is held."),
                ] {
                    let selected = self.activation_mode == mode;
                    if ui.add_sized(
                        Vec2::new(width, 30.0),
                        egui::Button::new(RichText::new(label).color(if selected { Color32::WHITE } else { palette.text }))
                            .fill(if selected { palette.accent } else { palette.panel_light })
                            .corner_radius(6.0),
                    ).on_hover_text(hint).clicked() && !selected {
                        self.set_enabled(false);
                        self.shortcut_down = false;
                        self.activation_mode = mode;
                    }
                }
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(if self.capturing_hotkey { "Press a key or combo" } else { "Shortcut" }).size(13.0).color(palette.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let label = if self.capturing_hotkey { "Listening..." } else { &self.hotkey_label };
                    if ui.add(egui::Button::new(RichText::new(label).strong().color(palette.text)).fill(palette.panel_light).corner_radius(6.0)).clicked() {
                        self.set_enabled(false);
                        self.shortcut_down = false;
                        self.capturing_hotkey = true;
                        self.error = None;
                    }
                });
            });
        });
    }

    fn start_button(&mut self, ui: &mut egui::Ui) {
        let palette = self.theme.palette();
        if self.activation_mode == ActivationMode::Hold {
            ui.add_sized(
                Vec2::new(ui.available_width(), 42.0),
                egui::Button::new(RichText::new(format!("Hold {} to click", self.hotkey_label)).size(14.0).strong().color(palette.text))
                    .fill(palette.panel_light).corner_radius(7.0).sense(egui::Sense::hover()),
            );
            return;
        }
        let (text, color) = if self.enabled { ("Stop", Color32::from_rgb(180, 68, 68)) } else { ("Start", palette.accent) };
        if ui.add_sized(Vec2::new(ui.available_width(), 42.0), egui::Button::new(RichText::new(text).size(14.0).strong().color(Color32::WHITE)).fill(color).corner_radius(7.0)).clicked() {
            self.set_enabled(!self.enabled);
        }
    }
}

fn draw_flux_mark(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, egui::CornerRadius::same(7), Color32::from_rgb(16, 17, 19));

    let scale = size / 128.0;
    let polygons = [
        [(42.0, 24.0), (55.0, 42.0), (47.0, 104.0), (30.0, 104.0)],
        [(42.0, 24.0), (102.0, 24.0), (91.0, 42.0), (55.0, 42.0)],
        [(48.0, 53.0), (83.0, 53.0), (73.0, 71.0), (45.0, 71.0)],
    ];
    for polygon in polygons {
        let points = polygon
            .iter()
            .map(|(x, y)| egui::pos2(rect.left() + x * scale, rect.top() + y * scale))
            .collect();
        painter.add(egui::Shape::convex_polygon(
            points,
            Color32::from_rgb(242, 243, 245),
            Stroke::NONE,
        ));
    }
}

fn make_hotkey(key: egui::Key, modifiers: egui::Modifiers) -> Option<(HotKey, String)> {
    let key_name = match key {
        egui::Key::Num0 => "Digit0".to_owned(),
        egui::Key::Num1 => "Digit1".to_owned(),
        egui::Key::Num2 => "Digit2".to_owned(),
        egui::Key::Num3 => "Digit3".to_owned(),
        egui::Key::Num4 => "Digit4".to_owned(),
        egui::Key::Num5 => "Digit5".to_owned(),
        egui::Key::Num6 => "Digit6".to_owned(),
        egui::Key::Num7 => "Digit7".to_owned(),
        egui::Key::Num8 => "Digit8".to_owned(),
        egui::Key::Num9 => "Digit9".to_owned(),
        _ => format!("{key:?}"),
    };
    let code = HotKey::try_from(key_name.as_str()).ok()?.key;

    let mut hotkey_modifiers = Modifiers::empty();
    if modifiers.ctrl {
        hotkey_modifiers |= Modifiers::CONTROL;
    }
    if modifiers.shift {
        hotkey_modifiers |= Modifiers::SHIFT;
    }
    if modifiers.alt {
        hotkey_modifiers |= Modifiers::ALT;
    }
    if modifiers.mac_cmd {
        hotkey_modifiers |= Modifiers::SUPER;
    }

    let hotkey = HotKey::new(Some(hotkey_modifiers), code);
    let mut label = Vec::new();
    if hotkey_modifiers.contains(Modifiers::CONTROL) {
        label.push("Ctrl".to_owned());
    }
    if hotkey_modifiers.contains(Modifiers::ALT) {
        label.push("Alt".to_owned());
    }
    if hotkey_modifiers.contains(Modifiers::SHIFT) {
        label.push("Shift".to_owned());
    }
    if hotkey_modifiers.contains(Modifiers::SUPER) {
        label.push("Win".to_owned());
    }
    label.push(match key {
        egui::Key::Num0 => "0".to_owned(),
        egui::Key::Num1 => "1".to_owned(),
        egui::Key::Num2 => "2".to_owned(),
        egui::Key::Num3 => "3".to_owned(),
        egui::Key::Num4 => "4".to_owned(),
        egui::Key::Num5 => "5".to_owned(),
        egui::Key::Num6 => "6".to_owned(),
        egui::Key::Num7 => "7".to_owned(),
        egui::Key::Num8 => "8".to_owned(),
        egui::Key::Num9 => "9".to_owned(),
        egui::Key::ArrowDown => "Down".to_owned(),
        egui::Key::ArrowLeft => "Left".to_owned(),
        egui::Key::ArrowRight => "Right".to_owned(),
        egui::Key::ArrowUp => "Up".to_owned(),
        egui::Key::PageDown => "Page Down".to_owned(),
        egui::Key::PageUp => "Page Up".to_owned(),
        _ => format!("{key:?}"),
    });

    Some((hotkey, label.join(" + ")))
}

fn click_worker(rx: Receiver<WorkerCommand>, clicks: Arc<AtomicU64>) {
    let mut clicker = Enigo::new(&Settings::default()).ok();
    let mut settings = ClickSettings { button: ClickButton::Left, interval: Duration::from_millis(100) };
    let mut enabled = false;
    loop {
        if enabled {
            match rx.recv_timeout(settings.interval) {
                Ok(WorkerCommand::SetEnabled(value)) => enabled = value,
                Ok(WorkerCommand::Update(value)) => settings = value,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if let Some(enigo) = clicker.as_mut() {
                        if enigo.button(settings.button.enigo(), Direction::Click).is_ok() {
                            clicks.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match rx.recv() {
                Ok(WorkerCommand::SetEnabled(value)) => enabled = value,
                Ok(WorkerCommand::Update(value)) => settings = value,
                Err(_) => break,
            }
        }
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([370.0, 620.0])
            .with_min_inner_size([340.0, 580.0])
            .with_resizable(false)
            .with_title("Auto Clicker"),
        ..Default::default()
    };

    eframe::run_native(
        "Flux",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_fonts(app_fonts());
            cc.egui_ctx.set_visuals(visuals_for(Theme::Dark));
            Ok(Box::new(ClickerApp::new()))
        }),
    )
}

fn visuals_for(theme: Theme) -> egui::Visuals {
    let palette = theme.palette();
    let mut visuals = match theme {
        Theme::Dark => egui::Visuals::dark(),
        Theme::Light => egui::Visuals::light(),
    };
    visuals.panel_fill = palette.bg;
    visuals.window_fill = palette.bg;
    visuals.extreme_bg_color = palette.panel;
    visuals.widgets.inactive.bg_fill = palette.panel;
    visuals.widgets.inactive.fg_stroke.color = palette.text;
    visuals.widgets.hovered.bg_fill = palette.panel_light;
    visuals.widgets.hovered.fg_stroke.color = palette.text;
    visuals.widgets.active.bg_fill = palette.panel_light;
    visuals.selection.bg_fill = palette.accent;
    visuals.override_text_color = Some(palette.text);
    visuals
}

fn app_fonts() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();

    #[cfg(target_os = "windows")]
    if let Ok(font_bytes) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
        fonts.font_data.insert(
            "flux_segoe_ui".to_owned(),
            Arc::new(egui::FontData::from_owned(font_bytes)),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "flux_segoe_ui".to_owned());
    }

    fonts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_changes_once_per_press_and_stays_on_after_release() {
        let mut down = false;
        let mode = ActivationMode::Toggle;
        assert!(mode.hotkey_enabled(false, &mut down, HotKeyState::Pressed));
        assert!(mode.hotkey_enabled(true, &mut down, HotKeyState::Pressed));
        assert!(mode.hotkey_enabled(true, &mut down, HotKeyState::Released));
        assert!(!down);
        assert!(!mode.hotkey_enabled(true, &mut down, HotKeyState::Pressed));
    }

    #[test]
    fn hold_stops_on_release_and_can_start_again() {
        let mut down = false;
        let mode = ActivationMode::Hold;
        assert!(mode.hotkey_enabled(false, &mut down, HotKeyState::Pressed));
        assert!(mode.hotkey_enabled(true, &mut down, HotKeyState::Pressed));
        assert!(!mode.hotkey_enabled(true, &mut down, HotKeyState::Released));
        assert!(!down);
        assert!(mode.hotkey_enabled(false, &mut down, HotKeyState::Pressed));
    }

    #[test]
    fn hold_release_without_press_does_not_start_clicking() {
        let mut down = false;
        assert!(!ActivationMode::Hold.hotkey_enabled(false, &mut down, HotKeyState::Released));
    }
}
