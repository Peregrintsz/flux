use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
    time::Duration,
};

use eframe::egui::{self, Align, Color32, FontId, Layout, RichText, Stroke, Vec2};
use enigo::{Button, Direction, Enigo, Mouse, Settings};
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager,
    hotkey::{Code, HotKey, HotKeyState},
};

const BG: Color32 = Color32::from_rgb(13, 17, 25);
const PANEL: Color32 = Color32::from_rgb(22, 28, 39);
const PANEL_LIGHT: Color32 = Color32::from_rgb(29, 37, 51);
const ACCENT: Color32 = Color32::from_rgb(125, 92, 255);
const MUTED: Color32 = Color32::from_rgb(143, 154, 174);
const GREEN: Color32 = Color32::from_rgb(65, 211, 155);

#[derive(Clone, Copy, PartialEq)]
enum ClickButton { Left, Right, Middle }
impl ClickButton {
    fn label(self) -> &'static str { match self { Self::Left => "Left", Self::Right => "Right", Self::Middle => "Middle" } }
    fn enigo(self) -> Button { match self { Self::Left => Button::Left, Self::Right => Button::Right, Self::Middle => Button::Middle } }
}

#[derive(Clone, Copy, PartialEq)]
enum SpeedMode { Cps, Milliseconds }

#[derive(Clone, Copy)]
enum KeyChoice { F6, F7, F8, F9, F10, F11, F12 }
impl KeyChoice {
    const ALL: [Self; 7] = [Self::F6, Self::F7, Self::F8, Self::F9, Self::F10, Self::F11, Self::F12];
    fn label(self) -> &'static str { match self { Self::F6=>"F6",Self::F7=>"F7",Self::F8=>"F8",Self::F9=>"F9",Self::F10=>"F10",Self::F11=>"F11",Self::F12=>"F12" } }
    fn code(self) -> Code { match self { Self::F6=>Code::F6,Self::F7=>Code::F7,Self::F8=>Code::F8,Self::F9=>Code::F9,Self::F10=>Code::F10,Self::F11=>Code::F11,Self::F12=>Code::F12 } }
    fn hotkey(self) -> HotKey { HotKey::new(None, self.code()) }
}

#[derive(Clone, Copy, PartialEq)]
struct ClickSettings { button: ClickButton, interval: Duration }
enum WorkerCommand { SetEnabled(bool), Update(ClickSettings) }

struct ClickerApp {
    button: ClickButton,
    mode: SpeedMode,
    cps: u32,
    milliseconds: u32,
    key: KeyChoice,
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
        thread::Builder::new().name("flux-clicker".into())
            .spawn(move || click_worker(worker_rx, worker_clicks))
            .expect("Could not start click worker thread");
        let key = KeyChoice::F6;
        let hotkey_manager = GlobalHotKeyManager::new().expect("Could not initialize hotkey manager");
        let hotkey = key.hotkey();
        let error = hotkey_manager.register(hotkey).err().map(|e| format!("Could not register hotkey: {e}"));
        let settings = ClickSettings { button: ClickButton::Left, interval: Duration::from_millis(100) };
        Self { button: ClickButton::Left, mode: SpeedMode::Cps, cps: 10, milliseconds: 100, key, enabled: false, clicks, last_settings: settings, worker_tx, hotkey_manager, hotkey_id: hotkey.id(), error }
    }
    fn interval(&self) -> Duration {
        let millis = match self.mode { SpeedMode::Cps => 1000 / self.cps.max(1), SpeedMode::Milliseconds => self.milliseconds.max(1) };
        Duration::from_millis(millis as u64)
    }
    fn click_settings(&self) -> ClickSettings { ClickSettings { button: self.button, interval: self.interval() } }
    fn set_enabled(&mut self, enabled: bool) { self.enabled = enabled; let _ = self.worker_tx.send(WorkerCommand::SetEnabled(enabled)); }
    fn register_key(&mut self, next: KeyChoice) {
        let next_hotkey = next.hotkey();
        if let Err(error) = self.hotkey_manager.register(next_hotkey) { self.error = Some(format!("Could not register {}: {error}", next.label())); return; }
        let _ = self.hotkey_manager.unregister(self.key.hotkey());
        self.key = next; self.hotkey_id = next_hotkey.id(); self.error = None;
    }
}

impl eframe::App for ClickerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.id == self.hotkey_id && event.state == HotKeyState::Pressed { self.set_enabled(!self.enabled); }
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin::same(20))).show(ctx, |ui| {
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("●").color(ACCENT).size(23.0));
                    ui.vertical(|ui| {
                        ui.label(RichText::new("FLUX").font(FontId::proportional(21.0)).strong().color(Color32::WHITE));
                        ui.label(RichText::new("COMPACT AUTO CLICKER").size(9.5).color(MUTED));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| { ui.label(RichText::new("v0.1.0").size(10.0).color(MUTED)); });
                });
                ui.add_space(17.0); self.status_card(ui); ui.add_space(14.0);
                self.section_title(ui, "CLICK SETTINGS", "01"); ui.add_space(8.0); self.click_options(ui); ui.add_space(14.0);
                self.section_title(ui, "SPEED", "02"); ui.add_space(8.0); self.speed_options(ui); ui.add_space(14.0);
                self.section_title(ui, "HOTKEY", "03"); ui.add_space(8.0); self.hotkey_options(ui); ui.add_space(15.0); self.start_button(ui);
                if let Some(error) = &self.error { ui.add_space(8.0); ui.label(RichText::new(error).size(11.0).color(Color32::from_rgb(255,128,128))); }
            });
        });
        let settings = self.click_settings();
        if settings != self.last_settings { let _ = self.worker_tx.send(WorkerCommand::Update(settings)); self.last_settings = settings; }
        ctx.request_repaint_after(Duration::from_millis(40));
    }
}

impl ClickerApp {
    fn status_card(&self, ui: &mut egui::Ui) {
        egui::Frame::new().fill(PANEL).stroke(Stroke::new(1.0, Color32::from_rgb(40,49,65))).corner_radius(12.0).inner_margin(14.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                let dot = if self.enabled { GREEN } else { MUTED };
                ui.label(RichText::new("●").color(dot).size(14.0));
                ui.vertical(|ui| {
                    ui.label(RichText::new(if self.enabled { "Clicking is active" } else { "Ready" }).strong().color(Color32::WHITE));
                    ui.label(RichText::new(format!("{} clicks sent", self.clicks.load(Ordering::Relaxed))).size(11.0).color(MUTED));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| { ui.label(RichText::new(format!("{} ms",self.interval().as_millis())).font(FontId::monospace(13.0)).color(if self.enabled { GREEN } else { Color32::WHITE })); });
            });
        });
    }
    fn section_title(&self, ui: &mut egui::Ui, title: &str, number: &str) {
        ui.horizontal(|ui| { ui.label(RichText::new(number).size(10.0).color(ACCENT).strong()); ui.label(RichText::new(title).size(10.0).color(MUTED).strong()); });
    }
    fn click_options(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new().fill(PANEL).corner_radius(11.0).inner_margin(5.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                let width=(ui.available_width()-12.0)/3.0;
                for (value,icon) in [(ClickButton::Left,"◀"),(ClickButton::Right,"▶"),(ClickButton::Middle,"●")] {
                    let selected=self.button==value;
                    let response=ui.add_sized(Vec2::new(width,48.0),egui::Button::new(RichText::new(format!("{icon}
{}",value.label())).size(12.0).color(if selected {Color32::WHITE}else{MUTED})).fill(if selected {PANEL_LIGHT}else{PANEL}).stroke(Stroke::new(if selected {1.0}else{0.0},ACCENT)).corner_radius(8.0));
                    if response.clicked(){self.button=value;}
                }
            });
        });
    }
    fn speed_options(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new().fill(PANEL).corner_radius(11.0).inner_margin(12.0).show(ui,|ui|{
            ui.horizontal(|ui|{
                let width=(ui.available_width()-8.0)/2.0;
                for (value,label) in [(SpeedMode::Cps,"CPS"),(SpeedMode::Milliseconds,"Milliseconds")] {
                    let selected=self.mode==value;
                    if ui.add_sized(Vec2::new(width,29.0),egui::Button::new(RichText::new(label).size(11.0).color(if selected {Color32::WHITE}else{MUTED})).fill(if selected {ACCENT}else{PANEL_LIGHT}).corner_radius(7.0)).clicked(){self.mode=value;}
                }
            });
            ui.add_space(11.0);
            ui.horizontal(|ui|{
                let (label,suffix,value)=match self.mode {SpeedMode::Cps=>("Speed","clicks / sec",&mut self.cps),SpeedMode::Milliseconds=>("Interval","ms",&mut self.milliseconds)};
                ui.label(RichText::new(label).size(12.0).color(Color32::WHITE));
                ui.with_layout(Layout::right_to_left(Align::Center),|ui|{
                    ui.label(RichText::new(suffix).size(10.0).color(MUTED));
                    ui.add(egui::DragValue::new(value).range(if self.mode==SpeedMode::Cps {1..=1000}else{1..=60000}).speed(1).min_decimals(0).max_decimals(0));
                });
            });
            ui.add_space(4.0);
            let hint=if self.mode==SpeedMode::Cps {"Set between 1 and 1,000 CPS"}else{"Set between 1 and 60,000 ms"};
            ui.label(RichText::new(hint).size(10.0).color(MUTED));
        });
    }
    fn hotkey_options(&mut self,ui:&mut egui::Ui){
        egui::Frame::new().fill(PANEL).corner_radius(11.0).inner_margin(12.0).show(ui,|ui|{
            ui.horizontal(|ui|{
                ui.vertical(|ui|{ui.label(RichText::new("Start / stop").size(12.0).color(Color32::WHITE));ui.label(RichText::new("Works while the app is in the background").size(10.0).color(MUTED));});
                ui.with_layout(Layout::right_to_left(Align::Center),|ui|{
                    egui::ComboBox::from_id_salt("hotkey").selected_text(RichText::new(self.key.label()).color(Color32::WHITE)).width(67.0).show_ui(ui,|ui|{
                        for choice in KeyChoice::ALL {if ui.selectable_label(self.key.label()==choice.label(),choice.label()).clicked()&&self.key.label()!=choice.label(){self.register_key(choice);}}
                    });
                });
            });
        });
    }
    fn start_button(&mut self,ui:&mut egui::Ui){
        let (text,color)=if self.enabled {("■   STOP",Color32::from_rgb(180,68,87))}else{("▶   START",ACCENT)};
        if ui.add_sized(Vec2::new(ui.available_width(),44.0),egui::Button::new(RichText::new(text).size(13.0).strong().color(Color32::WHITE)).fill(color).corner_radius(10.0)).clicked(){self.set_enabled(!self.enabled);}
    }
}

fn click_worker(rx:Receiver<WorkerCommand>,clicks:Arc<AtomicU64>){
    let mut clicker=Enigo::new(&Settings::default()).ok();
    let mut settings=ClickSettings{button:ClickButton::Left,interval:Duration::from_millis(100)};
    let mut enabled=false;
    loop {
        if enabled {
            match rx.recv_timeout(settings.interval){
                Ok(WorkerCommand::SetEnabled(value))=>enabled=value,
                Ok(WorkerCommand::Update(value))=>settings=value,
                Err(mpsc::RecvTimeoutError::Timeout)=>if let Some(enigo)=clicker.as_mut(){if enigo.button(settings.button.enigo(),Direction::Click).is_ok(){clicks.fetch_add(1,Ordering::Relaxed);}},
                Err(mpsc::RecvTimeoutError::Disconnected)=>break,
            }
        } else {
            match rx.recv(){Ok(WorkerCommand::SetEnabled(value))=>enabled=value,Ok(WorkerCommand::Update(value))=>settings=value,Err(_)=>break}
        }
    }
}

fn main()->eframe::Result{
    let options=eframe::NativeOptions{viewport:egui::ViewportBuilder::default().with_inner_size([370.0,590.0]).with_min_inner_size([340.0,540.0]).with_resizable(false).with_title("Flux · Autoclicker"),..Default::default()};
    eframe::run_native("Flux",options,Box::new(|cc|{
        let mut visuals=egui::Visuals::dark();
        visuals.panel_fill=BG;visuals.window_fill=BG;visuals.extreme_bg_color=PANEL;
        visuals.widgets.inactive.bg_fill=PANEL_LIGHT;visuals.widgets.hovered.bg_fill=Color32::from_rgb(42,52,70);visuals.selection.bg_fill=ACCENT;
        cc.egui_ctx.set_visuals(visuals);
        Ok(Box::new(ClickerApp::new()))
    }))
}
