use crate::vanguard::{self, Diagnosis, Stage};
use eframe::egui::{
    self, Align, Color32, FontData, FontFamily, FontId, Frame, Layout, RichText, Stroke,
};
use std::io;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

const BG: Color32 = Color32::from_rgb(16, 22, 25);
const PANEL: Color32 = Color32::from_rgb(24, 33, 37);
const LINE: Color32 = Color32::from_rgb(52, 68, 72);
const INK: Color32 = Color32::from_rgb(239, 248, 246);
const MUTED: Color32 = Color32::from_rgb(170, 189, 191);
const MINT: Color32 = Color32::from_rgb(119, 234, 203);
const WARN: Color32 = Color32::from_rgb(255, 191, 145);

pub fn run(language: Option<&str>) -> io::Result<()> {
    let ru = language.map_or_else(crate::locale::is_russian, |lang| lang == "ru");
    eframe::run_native(
        "Riot Fix",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([920.0, 680.0])
                .with_min_inner_size([640.0, 560.0]),
            centered: true,
            ..Default::default()
        },
        Box::new(move |context| {
            configure(&context.egui_ctx);
            let mut app = App::new(ru);
            app.check(&context.egui_ctx, true);
            Ok(Box::new(app))
        }),
    )
    .map_err(|error| io::Error::other(error.to_string()))
}

fn configure(context: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(INK);
    visuals.panel_fill = BG;
    visuals.window_fill = PANEL;
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(31, 44, 48);
    visuals.widgets.inactive.weak_bg_fill = visuals.widgets.inactive.bg_fill;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, LINE);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(42, 58, 61);
    visuals.widgets.hovered.weak_bg_fill = visuals.widgets.hovered.bg_fill;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, MINT);
    visuals.widgets.active.bg_fill = Color32::from_rgb(49, 71, 71);
    visuals.selection.bg_fill = Color32::from_rgb(48, 83, 78);
    context.set_visuals(visuals);

    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    let folder = std::path::PathBuf::from(root).join("Fonts");
    let mut fonts = egui::FontDefinitions::default();
    for (file, name, family) in [
        ("segoeui.ttf", "Segoe UI", FontFamily::Proportional),
        ("seguisb.ttf", "Display", FontFamily::Name("Display".into())),
        ("consola.ttf", "Consolas", FontFamily::Monospace),
    ] {
        let fallback = fonts.families[&FontFamily::Proportional].clone();
        let assigned = fonts.families.entry(family).or_insert(fallback);
        if let Ok(file) = std::fs::read(folder.join(file)) {
            fonts
                .font_data
                .insert(name.into(), FontData::from_owned(file).into());
            assigned.insert(0, name.into());
        }
    }
    context.set_fonts(fonts);
    context.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(9.0, 8.0);
        style.spacing.button_padding = egui::vec2(11.0, 7.0);
    });
}

fn display(text: &str, size: f32) -> RichText {
    RichText::new(text).font(FontId::new(size, FontFamily::Name("Display".into())))
}

fn surface() -> Frame {
    Frame::new()
        .fill(PANEL)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(12)
        .inner_margin(20)
}

enum Event {
    Diagnosis(Diagnosis),
    Stage(Stage),
    Recovered { reboot_required: bool },
    Cancelled,
    Finished { code: u32, output: String },
    Failed(String),
    UpdateFailed(String),
}

#[derive(Clone, Copy)]
enum Action {
    Quick,
    Scan,
    Apply,
    Restart,
    VoiceCheck,
    VoiceFix,
    VoiceReset,
}

impl Action {
    fn flag(self) -> &'static str {
        match self {
            Self::Quick => "--check",
            Self::Scan => "--dry-run",
            Self::Apply => "--apply",
            Self::Restart => "--restart-client",
            Self::VoiceCheck => "--voice-check",
            Self::VoiceFix => "--voice-fix",
            Self::VoiceReset => "--voice-reset",
        }
    }

    fn admin(self) -> bool {
        matches!(self, Self::Apply | Self::VoiceFix | Self::VoiceReset)
    }
}

struct App {
    ru: bool,
    worker: Option<Receiver<Event>>,
    diagnosis: Option<Diagnosis>,
    stage: Option<Stage>,
    target: Option<String>,
    recovery: Option<Arc<AtomicU8>>,
    auto_retry: bool,
    next_check: Instant,
    cancelled: bool,
    reboot_required: Option<bool>,
    update_error: Option<String>,
    journal: String,
    action_caption: Option<&'static str>,
    journal_code: Option<u32>,
}

impl App {
    fn new(ru: bool) -> Self {
        Self {
            ru,
            worker: None,
            diagnosis: None,
            stage: None,
            target: None,
            recovery: None,
            auto_retry: true,
            next_check: Instant::now(),
            cancelled: false,
            reboot_required: None,
            update_error: None,
            journal: String::new(),
            action_caption: None,
            journal_code: None,
        }
    }

    fn t(&self, ru: &'static str, en: &'static str) -> &'static str {
        if self.ru { ru } else { en }
    }

    fn spawn(
        &mut self,
        context: &egui::Context,
        task: impl FnOnce(&dyn Fn(Event)) + Send + 'static,
    ) {
        let (sender, receiver) = mpsc::channel();
        self.worker = Some(receiver);
        let context = context.clone();
        std::thread::spawn(move || task(&|event| send(&sender, &context, event)));
    }

    fn check(&mut self, context: &egui::Context, repair: bool) {
        self.diagnosis = None;
        self.update_error = None;
        self.reboot_required = None;
        self.stage = None;
        self.target = None;
        self.cancelled = false;
        self.next_check = Instant::now() + Duration::from_secs(15);
        let control = Arc::new(AtomicU8::new(0));
        self.recovery = repair.then(|| Arc::clone(&control));
        let ru = self.ru;
        self.spawn(context, move |send| {
            crate::locale::set_worker_language(ru);
            let result = (|| -> io::Result<()> {
                let diagnosis = vanguard::diagnose()?;
                send(Event::Diagnosis(diagnosis));
                if repair && let Diagnosis::Stalled(version) = diagnosis {
                    let mut last_progress = None;
                    let reboot_required = vanguard::recover(version, |stage| {
                        if matches!(stage, Stage::Install)
                            && control
                                .compare_exchange(0, 2, Ordering::Relaxed, Ordering::Relaxed)
                                .is_err()
                        {
                            return false;
                        }
                        if control.load(Ordering::Relaxed) == 1 {
                            return false;
                        }
                        if let Stage::Download {
                            fraction, bytes, ..
                        } = stage
                        {
                            let progress = ((fraction * 1000.0) as u32, bytes / 1_048_576);
                            if last_progress == Some(progress) {
                                return true;
                            }
                            last_progress = Some(progress);
                        }
                        send(Event::Stage(stage));
                        true
                    })?;
                    send(Event::Recovered { reboot_required });
                }
                Ok(())
            })();
            if let Err(error) = result {
                if error.kind() == io::ErrorKind::Interrupted {
                    send(Event::Cancelled);
                } else {
                    send(Event::UpdateFailed(error.to_string()));
                }
            }
        });
    }

    fn tool(&mut self, context: &egui::Context, action: Action, caption: &'static str) {
        self.action_caption = Some(caption);
        self.journal.clear();
        self.journal_code = None;
        let language = self.t("ru", "en");
        self.spawn(context, move |send| {
            let result = (|| -> io::Result<Event> {
                let executable = std::env::current_exe()?;
                let args = [action.flag(), "--no-pause", "--lang", language];
                if action.admin() && !crate::windows::is_elevated()? {
                    let code = crate::windows::run_elevated(&executable, &args)?;
                    return Ok(Event::Finished {
                        code,
                        output: String::new(),
                    });
                }
                let result = Command::new(executable)
                    .args(args)
                    .stdin(Stdio::null())
                    .creation_flags(0x08000000)
                    .output()?;
                let mut output = String::from_utf8_lossy(&result.stdout).into_owned();
                output.push_str(&String::from_utf8_lossy(&result.stderr));
                Ok(Event::Finished {
                    code: result.status.code().unwrap_or(1) as u32,
                    output,
                })
            })();
            send(result.unwrap_or_else(|error| Event::Failed(error.to_string())));
        });
    }

    fn receive(&mut self) {
        while let Some(worker) = &self.worker {
            match worker.try_recv() {
                Ok(Event::Diagnosis(state)) => {
                    self.diagnosis = Some(state);
                    self.target = match state {
                        Diagnosis::Stalled(version)
                        | Diagnosis::Updating(version)
                        | Diagnosis::Current(version) => Some(vanguard::display_version(version)),
                        Diagnosis::NoEvidence => None,
                    };
                }
                Ok(Event::Stage(stage)) => self.stage = Some(stage),
                Ok(Event::Recovered { reboot_required }) => {
                    self.reboot_required = Some(reboot_required);
                    self.stage = None;
                }
                Ok(Event::Cancelled) => {
                    self.cancelled = true;
                    self.stage = None;
                }
                Ok(Event::Finished { code, output }) => {
                    self.journal = output;
                    self.journal_code = Some(code);
                }
                Ok(Event::UpdateFailed(error)) => {
                    self.update_error = Some(error);
                    self.stage = None;
                }
                Ok(Event::Failed(error)) => {
                    self.journal = error;
                    self.journal_code = Some(1);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.worker = None;
                    self.recovery = None;
                }
            }
        }
    }

    fn update_card(&mut self, ui: &mut egui::Ui) {
        surface().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("RIOT VANGUARD")
                        .monospace()
                        .color(MINT)
                        .size(12.0),
                );
                if let Some(target) = &self.target {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        Frame::new()
                            .fill(Color32::from_rgb(33, 52, 53))
                            .corner_radius(6)
                            .inner_margin(egui::Margin::symmetric(9, 4))
                            .show(ui, |ui| {
                                ui.label(RichText::new(target).monospace().color(MINT).size(12.0));
                            });
                    });
                }
            });
            let (title, detail, color) = self.update_text();
            ui.add_space(3.0);
            ui.label(display(title, 31.0));
            ui.label(RichText::new(detail).size(14.0).color(color));
            if let Some(error) = &self.update_error {
                ui.label(RichText::new(error).color(WARN));
            }
            ui.add_space(10.0);
            if matches!(
                self.diagnosis,
                Some(Diagnosis::Stalled(_) | Diagnosis::Updating(_))
            ) {
                self.track(ui);
            }
            ui.add_space(8.0);
            let ready = self.worker.is_none();
            let can_repair = matches!(self.diagnosis, Some(Diagnosis::Stalled(_)))
                && self.reboot_required.is_none();
            ui.horizontal_wrapped(|ui| {
                if can_repair {
                    let button = egui::Button::new(
                        display(self.t("Восстановить обновление", "Recover update"), 15.0)
                            .color(BG),
                    )
                    .fill(MINT)
                    .min_size(egui::vec2(200.0, 39.0))
                    .corner_radius(7);
                    if ui.add_enabled(ready, button).clicked() {
                        self.action_caption = None;
                        self.check(ui.ctx(), true);
                    }
                }
                if self
                    .recovery
                    .as_ref()
                    .is_some_and(|state| state.load(Ordering::Relaxed) == 0)
                    && matches!(self.stage, Some(Stage::Download { .. } | Stage::Verify))
                    && ui.button(self.t("Отменить", "Cancel")).clicked()
                    && let Some(state) = &self.recovery
                    && state
                        .compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed)
                        .is_ok()
                {
                    self.auto_retry = false;
                }
                let check = self.t("Проверить статус", "Check status");
                let button = if can_repair {
                    egui::Button::new(check)
                } else {
                    egui::Button::new(display(check, 14.0).color(BG)).fill(MINT)
                }
                .min_size(egui::vec2(144.0, 39.0))
                .corner_radius(7);
                if ui.add_enabled(ready, button).clicked() {
                    self.action_caption = None;
                    self.check(ui.ctx(), false);
                }
                if can_repair && ready {
                    ui.label(
                        RichText::new(self.t(
                            "Понадобятся права администратора",
                            "Requires administrator approval",
                        ))
                        .size(12.0)
                        .color(MUTED),
                    );
                }
            });
        });
    }

    fn update_text(&self) -> (&'static str, String, Color32) {
        let t = |ru, en| self.t(ru, en);
        if self.cancelled {
            return (
                t("Восстановление остановлено", "Recovery cancelled"),
                t(
                    "Запусти восстановление ещё раз, когда будешь готов.",
                    "Run recovery again when ready.",
                )
                .into(),
                MUTED,
            );
        }
        if self.update_error.is_some() {
            return (
                t(
                    "Проверка или восстановление не завершились",
                    "Update recovery did not finish",
                ),
                t(
                    "Устрани причину и проверь статус ещё раз.",
                    "Resolve the issue and check again.",
                )
                .into(),
                WARN,
            );
        }
        if let Some(reboot_required) = self.reboot_required {
            return (
                t("Vanguard обновлён", "Vanguard is updated"),
                if reboot_required {
                    t(
                        "Перезагрузи компьютер, чтобы завершить обновление.",
                        "Restart the computer to complete the update.",
                    )
                } else {
                    t(
                        "Версия подтверждена. Riot Client запущен заново.",
                        "Version verified. Riot Client was relaunched.",
                    )
                }
                .into(),
                MINT,
            );
        }
        if let Some(stage) = &self.stage {
            let (title, detail) = match stage {
                Stage::Download { bytes, total, .. } => {
                    let downloaded = bytes / 1_048_576;
                    let size = total / 1_048_576;
                    (
                        t("Скачиваем Vanguard", "Downloading Vanguard"),
                        if *total > 0 {
                            format!("{downloaded} / {size} {}", t("МБ", "MB"))
                        } else {
                            format!("{downloaded} {}", t("МБ", "MB"))
                        },
                    )
                }
                Stage::Verify => (
                    t("Проверяем установщик", "Verifying installer"),
                    t(
                        "Проверяем цифровую подпись Riot Games.",
                        "Checking the Riot Games digital signature.",
                    )
                    .into(),
                ),
                Stage::Install => (
                    t("Устанавливаем Vanguard", "Installing Vanguard"),
                    t(
                        "Подтверди запрос Windows, если он появится.",
                        "Confirm the Windows prompt if it appears.",
                    )
                    .into(),
                ),
                Stage::Restart => (
                    t("Открываем Riot Client", "Opening Riot Client"),
                    t(
                        "Перезапускаем клиент после установки.",
                        "Relaunching the client after installation.",
                    )
                    .into(),
                ),
            };
            return (title, detail, MINT);
        }
        let (title, detail, color) = match self.diagnosis {
            Some(Diagnosis::Current(_)) => (
                t("Vanguard актуален", "Vanguard is up to date"),
                t(
                    "В журнале Riot нет незавершённого обновления.",
                    "The Riot log has no pending update.",
                ),
                MINT,
            ),
            Some(Diagnosis::NoEvidence) => (
                t("Обновление не обнаружено", "No update detected"),
                t(
                    "Если VALORANT завис на обновлении, открой Riot Client и повтори проверку.",
                    "If VALORANT is stuck updating, open Riot Client and check again.",
                ),
                MUTED,
            ),
            Some(Diagnosis::Updating(_)) => (
                t(
                    "Riot Client обновляет Vanguard",
                    "Riot Client is updating Vanguard",
                ),
                t(
                    "Riot Fix проверит, движется ли загрузка.",
                    "Riot Fix will check whether the update is progressing.",
                ),
                MUTED,
            ),
            Some(Diagnosis::Stalled(_)) => (
                t("Обновление Vanguard зависло", "Vanguard update is stuck"),
                t(
                    "Riot Fix скачает нужную версию с серверов Riot и установит её.",
                    "Riot Fix can download the required version from Riot and install it.",
                ),
                WARN,
            ),
            None => (
                t("Проверяем обновление", "Checking for a stuck update"),
                t(
                    "Читаем последнее состояние Vanguard в журнале Riot.",
                    "Reading the last Vanguard status in the Riot log.",
                ),
                MUTED,
            ),
        };
        (title, detail.into(), color)
    }

    fn track(&self, ui: &mut egui::Ui) {
        let position = match &self.stage {
            Some(Stage::Download { fraction, .. }) => *fraction,
            Some(Stage::Verify) => 1.5,
            Some(Stage::Install) => 2.5,
            Some(Stage::Restart) => 3.5,
            None if self.reboot_required.is_some() => 4.0,
            None => 0.0,
        };
        ui.columns(4, |columns| {
            for (index, (column, caption)) in columns
                .iter_mut()
                .zip([
                    self.t("Загрузка", "Download"),
                    self.t("Подпись", "Signature"),
                    self.t("Установка", "Install"),
                    self.t("Запуск клиента", "Relaunch"),
                ])
                .enumerate()
            {
                let fraction = (position - index as f32).clamp(0.0, 1.0);
                column.spacing_mut().item_spacing.y = 4.0;
                column.add(egui::ProgressBar::new(fraction).desired_height(5.0).fill(
                    if fraction > 0.0 {
                        MINT
                    } else {
                        Color32::TRANSPARENT
                    },
                ));
                column.label(RichText::new(caption).size(11.5).color(if fraction > 0.0 {
                    MINT
                } else {
                    MUTED
                }));
            }
        });
    }

    fn tools(&mut self, ui: &mut egui::Ui) {
        let connection = [
            (
                Action::Quick,
                self.t("Проверить Riot", "Check Riot connection"),
            ),
            (
                Action::Scan,
                self.t("Диагностика сети и Zapret", "Check network and Zapret"),
            ),
            (
                Action::Apply,
                self.t("Применить сетевой фикс", "Apply connection fix"),
            ),
            (
                Action::Restart,
                self.t("Перезапустить Riot Client", "Restart Riot Client"),
            ),
        ];
        let voice = [
            (
                Action::VoiceCheck,
                self.t("Проверить голосовой маршрут", "Check voice route"),
            ),
            (
                Action::VoiceFix,
                self.t("Применить фикс голоса", "Apply voice fix"),
            ),
            (
                Action::VoiceReset,
                self.t("Отменить фикс голоса", "Undo voice fix"),
            ),
        ];
        ui.columns(2, |columns| {
            self.tool_card(
                &mut columns[0],
                self.t("Подключение", "Connection"),
                &connection,
            );
            self.tool_card(
                &mut columns[1],
                self.t("Голосовой чат", "Voice chat"),
                &voice,
            );
        });
    }

    fn tool_card(&mut self, ui: &mut egui::Ui, title: &str, actions: &[(Action, &'static str)]) {
        surface().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(199.0);
            ui.label(display(title, 20.0));
            ui.add_space(2.0);
            for &(action, caption) in actions {
                let button = egui::Button::new(RichText::new(caption).size(13.5))
                    .min_size(egui::vec2(ui.available_width(), 32.0));
                if ui.add_enabled(self.worker.is_none(), button).clicked() {
                    self.tool(ui.ctx(), action, caption);
                }
            }
        });
    }

    fn journal(&self, ui: &mut egui::Ui) {
        let Some(caption) = self.action_caption else {
            return;
        };
        surface().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(display(caption, 17.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if self.worker.is_some() {
                        ui.spinner();
                    }
                    if let Some(code) = self.journal_code {
                        let (caption, color) = match code {
                            0 => (self.t("Готово", "Done"), MINT),
                            2 => (self.t("Есть предупреждения", "Warnings"), WARN),
                            _ => (self.t("Ошибка", "Failed"), WARN),
                        };
                        ui.label(RichText::new(caption).color(color));
                    }
                    if !self.journal.is_empty() && ui.button(self.t("Копировать", "Copy")).clicked()
                    {
                        ui.ctx().copy_text(self.journal.clone());
                    }
                });
            });
            if !self.journal.is_empty() {
                egui::ScrollArea::vertical()
                    .max_height(175.0)
                    .id_salt("journal")
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(self.journal.trim()).monospace().color(MUTED),
                            )
                            .selectable(true),
                        );
                    });
            }
        });
    }
}

fn send(sender: &Sender<Event>, context: &egui::Context, event: Event) {
    let _ = sender.send(event);
    context.request_repaint();
}

impl eframe::App for App {
    fn update(&mut self, context: &egui::Context, _: &mut eframe::Frame) {
        self.receive();
        if self.worker.is_none()
            && self.auto_retry
            && matches!(self.diagnosis, Some(Diagnosis::Updating(_)))
        {
            if Instant::now() >= self.next_check {
                self.check(context, true);
            } else {
                context.request_repaint_after(
                    self.next_check.saturating_duration_since(Instant::now()),
                );
            }
        }
        if self.worker.is_some() {
            context.request_repaint_after(Duration::from_millis(150));
            if self.recovery.is_some() && context.input(|input| input.viewport().close_requested())
            {
                context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                if let Some(state) = &self.recovery {
                    let _ = state.compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed);
                    self.auto_retry = false;
                }
            }
        }
        egui::CentralPanel::default()
            .frame(Frame::new().fill(BG).inner_margin(26))
            .show(context, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (mark, _) = ui
                                .allocate_exact_size(egui::vec2(43.0, 43.0), egui::Sense::hover());
                            ui.painter()
                                .rect_filled(mark, 9, Color32::from_rgb(33, 52, 53));
                            let center = mark.center();
                            let trace = [
                                (-13.0, 2.0),
                                (-7.0, 2.0),
                                (-2.0, -8.0),
                                (4.0, 8.0),
                                (9.0, -2.0),
                                (14.0, -2.0),
                            ]
                            .map(|(x, y)| center + egui::vec2(x, y));
                            ui.painter()
                                .add(egui::Shape::line(trace.to_vec(), Stroke::new(2.5, MINT)));
                            ui.vertical(|ui| {
                                ui.label(display("Riot Fix", 31.0));
                                ui.label(
                                    RichText::new(self.t(
                                        "Обновления и подключение VALORANT",
                                        "VALORANT updates and connectivity",
                                    ))
                                    .color(MUTED),
                                );
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.add_enabled_ui(self.worker.is_none(), |ui| {
                                    egui::ComboBox::from_id_salt("language")
                                        .selected_text(self.t("Русский", "English"))
                                        .width(93.0)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut self.ru, true, "Русский");
                                            ui.selectable_value(&mut self.ru, false, "English");
                                        });
                                });
                            });
                        });
                        ui.add_space(12.0);
                        self.update_card(ui);
                        ui.add_space(8.0);
                        self.tools(ui);
                        ui.add_space(8.0);
                        self.journal(ui);
                    });
            });
    }
}
