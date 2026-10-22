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

const BG: Color32 = Color32::from_rgb(246, 247, 248);
const PANEL: Color32 = Color32::WHITE;
const PANEL_LIGHT: Color32 = Color32::from_rgb(237, 239, 242);
const ACCENT: Color32 = Color32::from_rgb(36, 39, 44);
const MUTED: Color32 = Color32::from_rgb(105, 111, 120);
const GREEN: Color32 = Color32::from_rgb(39, 133, 87);

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
struct ClickSettings {
    button: ClickButton,
    interval: Duration,
}

enum WorkerCommand {
    SetEnabled(bool),
    Update(ClickSettings),
}

struct ClickerApp {
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
        self.enabled = enabled;
        let _ = self.worker_tx.send(WorkerCommand::SetEnabled(enabled));
    }

    fn register_hotkey(&mut self, next_hotkey: HotKey, label: String) {
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
        if self.capturing_hotkey {
            let captured = ctx.input(|input| {
                input.events.iter().find_map(|event| match event {
                    egui::Event::Key { key, physical_key, pressed: true, repeat: false, modifiers } => {
                        make_hotkey(physical_key.unwrap_or(*key), *modifiers)
                    }
                    _ => None,
                })
            });
            if let Some((hotkey, label)) = captured {
                self.register_hotkey(hotkey, label);
            }
            while GlobalHotKeyEvent::receiver().try_recv().is_ok() {}
        } else {
            while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                if event.id == self.hotkey_id && event.state == HotKeyState::Pressed {
                    self.set_enabled(!self.enabled);
                }
            }
        }

        egui::Frame::new()
            .fill(BG)
            .inner_margin(egui::Margin::same(22))
            .show(ui, |ui| {
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        draw_flux_mark(ui, 32.0);
                        ui.add_space(9.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Flux").size(20.0).strong().color(ACCENT));
                            ui.label(RichText::new("Auto clicker").size(12.0).color(MUTED));
                        });
                    });

                    ui.add_space(20.0);
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
                    self.section_title(ui, "Toggle shortcut");
                    ui.add_space(7.0);
                    self.hotkey_options(ui);
                    ui.add_space(18.0);
                    self.start_button(ui);
                    if let Some(error) = &self.error {
                        ui.add_space(8.0);
                        ui.label(RichText::new(error).size(12.0).color(Color32::from_rgb(180, 55, 55)));
                    }
                });
            });

        let settings = self.click_settings();
        if settings != self.last_settings {
            let _ = self.worker_tx.send(WorkerCommand::Update(settings));
            self.last_settings = settings;
        }
        ctx.request_repaint_after(Duration::from_millis(40));
    }
}

impl ClickerApp {
    fn status_card(&self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .fill(PANEL)
            .stroke(Stroke::new(1.0, Color32::from_rgb(229, 231, 234)))
            .corner_radius(8.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let dot = if self.enabled { GREEN } else { MUTED };
                    let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
                    ui.painter().circle_filled(dot_rect.center(), 4.0, dot);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(if self.enabled { "Running" } else { "Ready" }).strong().color(ACCENT));
                        ui.label(RichText::new(format!("{} clicks", self.clicks.load(Ordering::Relaxed))).size(12.0).color(MUTED));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(format!("{} ms", self.interval().as_millis())).color(MUTED));
                    });
                });
            });
    }

    fn section_title(&self, ui: &mut egui::Ui, title: &str) {
        ui.label(RichText::new(title).size(14.0).strong().color(ACCENT));
    }

    fn click_options(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new().fill(PANEL).stroke(Stroke::new(1.0, Color32::from_rgb(229, 231, 234))).corner_radius(8.0).inner_margin(5.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                let item_width = (ui.available_width() - 12.0) / 3.0;
                for value in [ClickButton::Left, ClickButton::Right, ClickButton::Middle] {
                    let selected = self.button == value;
                    let response = ui.add_sized(
                        Vec2::new(item_width, 40.0),
                        egui::Button::new(RichText::new(value.label()).size(13.0).color(if selected { Color32::WHITE } else { ACCENT }))
                            .fill(if selected { ACCENT } else { PANEL })
                            .stroke(Stroke::new(1.0, if selected { ACCENT } else { Color32::from_rgb(229, 231, 234) }))
                            .corner_radius(6.0),
                    );
                    if response.clicked() { self.button = value; }
                }
            });
        });
    }

    fn speed_options(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new().fill(PANEL).stroke(Stroke::new(1.0, Color32::from_rgb(229, 231, 234))).corner_radius(8.0).inner_margin(10.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                let tab_width = (ui.available_width() - 8.0) / 2.0;
                for (value, label) in [(SpeedMode::Cps, "CPS"), (SpeedMode::Milliseconds, "Milliseconds")] {
                    let selected = self.mode == value;
                    if ui.add_sized(Vec2::new(tab_width, 30.0), egui::Button::new(RichText::new(label).size(13.0).color(if selected { Color32::WHITE } else { ACCENT })).fill(if selected { ACCENT } else { PANEL_LIGHT }).corner_radius(6.0)).clicked() {
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
                ui.label(RichText::new(label).size(13.0).color(ACCENT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(suffix).size(11.0).color(MUTED));
                    ui.add(egui::DragValue::new(value).speed(1).min_decimals(0).max_decimals(0));
                });
            });
            self.cps = self.cps.max(1);
            self.milliseconds = self.milliseconds.max(1);
        });
    }

    fn hotkey_options(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new().fill(PANEL).stroke(Stroke::new(1.0, Color32::from_rgb(229, 231, 234))).corner_radius(8.0).inner_margin(10.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(if self.capturing_hotkey { "Press a key or combo" } else { "Shortcut" }).size(13.0).color(ACCENT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let label = if self.capturing_hotkey { "Listening..." } else { &self.hotkey_label };
                    if ui.add(egui::Button::new(RichText::new(label).strong().color(ACCENT)).fill(PANEL_LIGHT).corner_radius(6.0)).clicked() {
                        self.capturing_hotkey = true;
                        self.error = None;
                    }
                });
            });
        });
    }

    fn start_button(&mut self, ui: &mut egui::Ui) {
        let (text, color) = if self.enabled { ("Stop", Color32::from_rgb(180, 68, 68)) } else { ("Start", ACCENT) };
        if ui.add_sized(Vec2::new(ui.available_width(), 42.0), egui::Button::new(RichText::new(text).size(14.0).strong().color(Color32::WHITE)).fill(color).corner_radius(7.0)).clicked() {
            self.set_enabled(!self.enabled);
        }
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

fn draw_flux_mark(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, egui::CornerRadius::same(9), Color32::BLACK);

    let x = rect.left() + size * 0.27;
    let y = rect.top() + size * 0.20;
    let mark = [
        egui::pos2(x, y),
        egui::pos2(rect.left() + size * 0.73, y),
        egui::pos2(rect.left() + size * 0.73, rect.top() + size * 0.35),
        egui::pos2(rect.left() + size * 0.45, rect.top() + size * 0.35),
        egui::pos2(rect.left() + size * 0.45, rect.top() + size * 0.45),
        egui::pos2(rect.left() + size * 0.68, rect.top() + size * 0.45),
        egui::pos2(rect.left() + size * 0.68, rect.top() + size * 0.60),
        egui::pos2(rect.left() + size * 0.45, rect.top() + size * 0.60),
        egui::pos2(rect.left() + size * 0.45, rect.bottom() - size * 0.20),
        egui::pos2(x, rect.bottom() - size * 0.20),
    ];
    painter.add(egui::Shape::convex_polygon(mark.to_vec(), Color32::WHITE, Stroke::NONE));
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
            .with_inner_size([370.0, 590.0])
            .with_min_inner_size([340.0, 540.0])
            .with_resizable(false)
            .with_title("Flux - Autoclicker"),
        ..Default::default()
    };

    eframe::run_native(
        "Flux",
        options,
        Box::new(|cc| {
            let mut visuals = egui::Visuals::light();
            visuals.panel_fill = BG;
            visuals.window_fill = BG;
            visuals.extreme_bg_color = PANEL;
            visuals.widgets.inactive.bg_fill = PANEL;
            visuals.widgets.hovered.bg_fill = PANEL_LIGHT;
            visuals.selection.bg_fill = ACCENT;
            visuals.override_text_color = Some(ACCENT);
            cc.egui_ctx.set_fonts(app_fonts());
            cc.egui_ctx.set_visuals(visuals);
            Ok(Box::new(ClickerApp::new()))
        }),
    )
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
