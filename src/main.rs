use eframe::egui::{self, Color32, FontFamily, FontId, RichText, Sense, Stroke, StrokeKind, Vec2};
use launchshelf::{Project, Registry, SharedJob, Store};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, mpsc},
    time::Duration,
};

const YELLOW: Color32 = Color32::from_rgb(244, 207, 38);
const INK: Color32 = Color32::from_rgb(20, 22, 27);
const BG: Color32 = Color32::from_rgb(29, 31, 37);
const PANEL: Color32 = Color32::from_rgb(20, 22, 27);
const CARD: Color32 = Color32::from_rgb(38, 41, 49);
const EDGE: Color32 = Color32::from_rgb(66, 70, 82);
const TEXT: Color32 = Color32::from_rgb(244, 242, 232);
const MUTED: Color32 = Color32::from_rgb(171, 175, 186);
const GOOD: Color32 = Color32::from_rgb(120, 222, 166);
const ALERT: Color32 = Color32::from_rgb(255, 158, 102);
enum Event {
    Candidate(launchshelf::Result<Project>),
    Git(HashMap<String, Option<launchshelf::GitInfo>>),
}
#[derive(Clone)]
enum Action {
    Launch(Project, bool),
    Edit(Project),
    Favorite(String),
    Folder(PathBuf),
    Docs(PathBuf),
    Register(Project),
    Log(String),
}
struct Workshop {
    store: Store,
    registry: Registry,
    load_error: Option<String>,
    search: String,
    view: String,
    editor: Option<Project>,
    editor_new: bool,
    confirm_remove: Option<Project>,
    message: Option<(String, bool)>,
    jobs: HashMap<String, SharedJob>,
    git: HashMap<String, Option<launchshelf::GitInfo>>,
    git_loading: bool,
    sender: mpsc::Sender<Event>,
    receiver: mpsc::Receiver<Event>,
    log_project: Option<String>,
    demo: bool,
    last_git: f64,
}
impl Workshop {
    fn new(cc: &eframe::CreationContext<'_>, store: Store, demo: bool) -> Self {
        install_theme(&cc.egui_ctx);
        let (registry, load_error) = match store.load() {
            Ok(s) => (s, None),
            Err(e) => (Registry::default(), Some(e)),
        };
        let (sender, receiver) = mpsc::channel();
        let mut app = Self {
            store,
            registry,
            load_error,
            search: String::new(),
            view: "All projects".into(),
            editor: None,
            editor_new: false,
            confirm_remove: None,
            message: None,
            jobs: HashMap::new(),
            git: HashMap::new(),
            git_loading: false,
            sender,
            receiver,
            log_project: None,
            demo,
            last_git: 0.,
        };
        if demo {
            app.registry = demo_registry();
        }
        app.refresh_git();
        app
    }
    fn persist(&mut self, next: Registry) -> bool {
        if self.demo {
            self.message = Some(("Demo mode does not change your library.".into(), false));
            return false;
        }
        if self.load_error.is_some() {
            self.message = Some((
                "Fix or reload the existing library before making changes.".into(),
                true,
            ));
            return false;
        }
        let mut next = next;
        match self.store.save(&mut next) {
            Ok(()) => {
                self.registry = next;
                true
            }
            Err(e) => {
                self.message = Some((e, true));
                false
            }
        }
    }
    fn refresh_git(&mut self) {
        if self.git_loading {
            return;
        }
        self.git_loading = true;
        let projects = self.registry.projects.clone();
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let statuses = projects
                .into_iter()
                .map(|p| {
                    let info = launchshelf::git_info(&p.folder);
                    (p.id, info)
                })
                .collect();
            let _ = sender.send(Event::Git(statuses));
        });
    }
    fn picker(&self, desktop: bool) {
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let result = if desktop {
                rfd::FileDialog::new()
                    .set_title("Import an installed application")
                    .set_directory(
                        std::env::var_os("HOME")
                            .map(PathBuf::from)
                            .unwrap_or_default()
                            .join(".local/share/applications"),
                    )
                    .add_filter("Desktop entry", &["desktop"])
                    .pick_file()
                    .map(|p| launchshelf::desktop_import(&p))
            } else {
                rfd::FileDialog::new()
                    .set_title("Choose a source project folder")
                    .pick_folder()
                    .map(|p| launchshelf::detect(&p))
            };
            if let Some(result) = result {
                let _ = sender.send(Event::Candidate(result));
            }
        });
    }
    fn handle(&mut self, action: Action) {
        match action {
            Action::Edit(p) => {
                self.editor = Some(p);
                self.editor_new = false;
            }
            Action::Launch(p, build) => {
                if self.demo {
                    self.message=Some(("Demo projects illustrate the interface. Add your own project to launch it.".into(),false));
                    return;
                }
                if self
                    .jobs
                    .get(&p.id)
                    .is_some_and(|j| !j.lock().unwrap().done)
                {
                    self.message =
                        Some(("This project's command is already running.".into(), false));
                    return;
                }
                match launchshelf::run(&self.store, &p, build) {
                    Ok(job) => {
                        self.jobs.insert(p.id.clone(), job);
                        self.message = None;
                    }
                    Err(e) => self.message = Some((e, true)),
                }
            }
            Action::Favorite(id) => {
                let mut next = self.registry.clone();
                if let Some(p) = next.projects.iter_mut().find(|p| p.id == id) {
                    p.favorite = !p.favorite;
                }
                self.persist(next);
            }
            Action::Folder(path) | Action::Docs(path) => {
                if let Err(e) = std::process::Command::new("xdg-open").arg(path).spawn() {
                    self.message = Some((format!("Could not open: {e}"), true));
                }
            }
            Action::Register(p) => {
                if self.demo {
                    return;
                }
                match std::env::current_exe()
                    .map_err(|e| e.to_string())
                    .and_then(|exe| launchshelf::register_desktop(&exe, &p))
                {
                    Ok(path) => {
                        self.message =
                            Some((format!("Launcher registered: {}", path.display()), false))
                    }
                    Err(e) => self.message = Some((e, true)),
                }
            }
            Action::Log(id) => self.log_project = Some(id),
        }
    }
    fn card(&self, ui: &mut egui::Ui, p: &Project) -> Option<Action> {
        let mut action = None;
        let accent = kind_color(&p.kind);
        let job = self.jobs.get(&p.id).map(|j| j.lock().unwrap().clone());
        let running = job.as_ref().is_some_and(|j| !j.done);
        let width = ui.available_width();
        ui.spacing_mut().item_spacing.y = 0.;
        let (cover, _) = ui.allocate_exact_size(Vec2::new(width, 88.), Sense::hover());
        let time = ui.input(|i| i.time);
        paint_cover(ui.painter(), cover, p, accent, running.then_some(time));
        let star =
            egui::Rect::from_min_size(cover.right_top() + Vec2::new(-42., 8.), Vec2::splat(34.));
        let response = ui
            .interact(star, ui.id().with(("favorite", &p.id)), Sense::click())
            .on_hover_text(if p.favorite {
                "Remove from favorites"
            } else {
                "Add to favorites"
            });
        ui.painter().text(
            star.center(),
            egui::Align2::CENTER_CENTER,
            if p.favorite { "★" } else { "☆" },
            FontId::proportional(24.),
            if response.hovered() {
                Color32::WHITE
            } else {
                INK
            },
        );
        if response.clicked() {
            action = Some(Action::Favorite(p.id.clone()));
        }
        egui::Frame::new()
            .fill(CARD)
            .inner_margin(egui::Margin::symmetric(16, 14))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(138.);
                ui.spacing_mut().item_spacing.y = 6.;
                ui.label(heavy(&p.name, 22.));
                ui.label(
                    RichText::new(shorten(&tilde(&p.folder), 46))
                        .size(12.5)
                        .color(MUTED),
                )
                .on_hover_text(p.folder.display().to_string());
                ui.add_space(4.);
                let (status, color) = match &job {
                    Some(j) if !j.done => ("Running now".to_string(), YELLOW),
                    Some(j) if j.success => ("Finished fine".to_string(), GOOD),
                    Some(_) => ("Something went wrong. Check the log.".to_string(), ALERT),
                    None if !p.folder.exists() => (
                        "Folder is missing. Configure its new location.".into(),
                        ALERT,
                    ),
                    None => match self.git.get(&p.id) {
                        Some(Some(g)) if g.changes == 0 => {
                            (format!("{}  ·  clean", g.branch), GOOD)
                        }
                        Some(Some(g)) => (
                            format!(
                                "{}  ·  {} {}",
                                g.branch,
                                g.changes,
                                if g.changes == 1 { "change" } else { "changes" }
                            ),
                            ALERT,
                        ),
                        _ if self.git_loading => ("Checking Git…".into(), MUTED),
                        _ => ("No Git repository".into(), MUTED),
                    },
                };
                ui.label(RichText::new(status).size(13.).color(color));
                ui.add_space(8.);
                let launch = ui.add_enabled(
                    !running,
                    egui::Button::new(
                        heavy(if running { "Running…" } else { "Launch" }, 17.).color(INK),
                    )
                    .fill(YELLOW)
                    .stroke(Stroke::NONE)
                    .min_size(Vec2::new(ui.available_width(), 40.)),
                );
                if launch.clicked() {
                    action = Some(Action::Launch(p.clone(), false));
                }
                ui.add_space(4.);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.;
                    if !p.build.is_empty() && !running && text_link(ui, "Build").clicked() {
                        action = Some(Action::Launch(p.clone(), true));
                    }
                    if text_link(ui, "Folder").clicked() {
                        action = Some(Action::Folder(p.folder.clone()));
                    }
                    if let Some(path) = &p.docs
                        && text_link(ui, "Notes").clicked()
                    {
                        action = Some(Action::Docs(path.clone()));
                    }
                    if text_link(ui, "Log").clicked() {
                        action = Some(Action::Log(p.id.clone()));
                    }
                    if text_link(ui, "Settings").clicked() {
                        action = Some(Action::Edit(p.clone()));
                    }
                });
            });
        action
    }
    fn editor(&mut self, ctx: &egui::Context) {
        let Some(mut project) = self.editor.clone() else {
            return;
        };
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        let mut register = None;
        egui::Window::new(if self.editor_new{"Add to your shelf"}else{"Project settings"}).id(egui::Id::new("project-editor")).open(&mut open).collapsible(false).resizable(false).default_width(560.).anchor(egui::Align2::CENTER_CENTER,[0.,0.]).show(ctx,|ui|{
            eyebrow(ui,"NOTHING RUNS UNTIL YOU PRESS LAUNCH");ui.label(heavy(if self.editor_new{"Does this look right?"}else{"Settings"},26.));ui.add_space(10.);
            let wide = |ui: &mut egui::Ui, text: &mut String| { ui.add(egui::TextEdit::singleline(text).desired_width(f32::INFINITY).margin(egui::Margin::symmetric(8, 6))); };
            ui.label("Name");wide(ui,&mut project.name);
            ui.add_space(4.);
            ui.label("What runs when you press Launch");
            ui.add(egui::TextEdit::singleline(&mut project.launch).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY).margin(egui::Margin::symmetric(8, 6)));
            ui.add_space(4.);
            ui.checkbox(&mut project.favorite,"Favorite");
            egui::CollapsingHeader::new("More settings").id_salt("more-settings").show(ui,|ui|{
                let mut folder=project.folder.to_string_lossy().into_owned();ui.label("Folder");wide(ui,&mut folder);project.folder=PathBuf::from(folder);
                ui.label("Type");wide(ui,&mut project.kind);
                ui.label("Build command (optional)");ui.add(egui::TextEdit::singleline(&mut project.build).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY).margin(egui::Margin::symmetric(8, 6)));
                ui.label("Notes file (optional)");let mut docs=project.docs.as_ref().map(|p|p.to_string_lossy().into_owned()).unwrap_or_default();if ui.add(egui::TextEdit::singleline(&mut docs).desired_width(f32::INFINITY).margin(egui::Margin::symmetric(8, 6))).changed(){project.docs=if docs.trim().is_empty(){None}else{Some(PathBuf::from(docs))}}
                ui.checkbox(&mut project.terminal,"Open in a terminal");
                ui.label(RichText::new("Arguments respect quotes. There's no automatic shell expansion, so use bash -lc '…' for pipelines or &&.").size(12.5).color(MUTED));
                if !self.editor_new {
                    ui.add_space(4.);
                    if text_link(ui,"Add to my app launcher").clicked(){register=Some(project.clone());}
                }
            });
            ui.add_space(12.);
            ui.horizontal(|ui|{
                save=ui.add(primary_button(if self.editor_new{"Put it on the shelf"}else{"Save"})).clicked();
                cancel=ui.add(ghost_button("Cancel")).clicked();
                if !self.editor_new{ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if text_link(ui,"Remove from shelf").clicked(){self.confirm_remove=Some(project.clone());cancel=true;}});}
            });
        });
        if let Some(p) = register {
            self.handle(Action::Register(p));
        }
        self.editor = if open && !cancel {
            Some(project.clone())
        } else {
            None
        };
        if save {
            match project.validate() {
                Err(e) => self.message = Some((e, true)),
                Ok(()) => {
                    let mut next = self.registry.clone();
                    if self.editor_new {
                        next.projects.push(project)
                    } else if let Some(old) = next.projects.iter_mut().find(|p| p.id == project.id)
                    {
                        *old = project;
                    }
                    if self.persist(next) {
                        self.editor = None;
                        self.message =
                            Some(("Saved. Press Launch when you're ready.".into(), false));
                        self.refresh_git();
                    }
                }
            }
        }
    }
}
impl eframe::App for Workshop {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.request_repaint_after(Duration::from_millis(500));
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::Candidate(Ok(mut p)) => {
                    p.name = friendly_name(&p.name);
                    self.editor = Some(p);
                    self.editor_new = true;
                }
                Event::Candidate(Err(e)) => self.message = Some((e, true)),
                Event::Git(g) => {
                    self.git = g;
                    self.git_loading = false;
                }
            }
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            {
                let path = file.path().to_path_buf();
                let result = if path.is_dir() {
                    launchshelf::detect(&path)
                } else {
                    launchshelf::desktop_import(&path)
                };
                match result {
                    Ok(mut p) => {
                        p.name = friendly_name(&p.name);
                        self.editor = Some(p);
                        self.editor_new = true;
                    }
                    Err(e) => self.message = Some((e, true)),
                }
            }
        }
        let now = ctx.input(|i| i.time);
        if now - self.last_git > 30. {
            self.last_git = now;
            self.refresh_git();
        }
        let any_running = self.jobs.values().any(|j| !j.lock().unwrap().done);
        if any_running {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
        egui::Panel::top("top")
            .frame(
                egui::Frame::new()
                    .fill(YELLOW)
                    .inner_margin(egui::Margin::symmetric(26, 16)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    shelf_mark(ui);
                    ui.add_space(4.);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.;
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.;
                            ui.label(heavy("LAUNCH", 30.).color(INK));
                            ui.label(RichText::new("SHELF").size(30.).color(INK));
                        });
                        ui.label(
                            RichText::new("A home for the apps you build.")
                                .size(14.)
                                .color(INK),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(ink_button("+ Add project")).clicked() {
                            self.picker(false)
                        }
                        ui.add_space(4.);
                        ui.scope(|ui| {
                            let visuals = ui.visuals_mut();
                            visuals.extreme_bg_color = Color32::from_rgb(255, 250, 225);
                            visuals.override_text_color = Some(INK);
                            visuals.widgets.inactive.bg_stroke = Stroke::new(1., INK);
                            visuals.widgets.hovered.bg_stroke = Stroke::new(1., INK);
                            visuals.selection.stroke = Stroke::new(2., INK);
                            ui.add(
                                egui::TextEdit::singleline(&mut self.search)
                                    .hint_text(
                                        RichText::new("Search your shelf…")
                                            .color(Color32::from_rgb(90, 90, 80)),
                                    )
                                    .margin(egui::Margin::symmetric(10, 8))
                                    .desired_width(300.0_f32.min(ui.available_width() * 0.5)),
                            );
                        });
                    });
                });
            });
        egui::Panel::left("side")
            .exact_size(214.)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(16, 20)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    eyebrow(ui, "YOUR SHELF");
                    ui.add_space(6.);
                    let mut names = vec!["All projects".to_string()];
                    if self.registry.projects.iter().any(|p| p.favorite) {
                        names.push("Favorites".into());
                    }
                    let mut kinds: Vec<String> = self
                        .registry
                        .projects
                        .iter()
                        .map(|p| p.kind.clone())
                        .collect();
                    kinds.sort();
                    kinds.dedup();
                    names.extend(kinds);
                    if !names.contains(&self.view) {
                        self.view = "All projects".into();
                    }
                    for name in &names {
                        let count = self
                            .registry
                            .projects
                            .iter()
                            .filter(|p| {
                                name == "All projects"
                                    || (name == "Favorites" && p.favorite)
                                    || &p.kind == name
                            })
                            .count();
                        if side_item(ui, name, count, &self.view == name) {
                            self.view = name.clone();
                        }
                    }
                    ui.add_space(22.);
                    ui.label(
                        RichText::new("Drop a project folder anywhere in this window to add it.")
                            .size(14.)
                            .color(TEXT),
                    );
                    ui.add_space(8.);
                    if text_link(ui, "Add an installed app instead").clicked() {
                        self.picker(true);
                    }
                    ui.add_space(22.);
                    ui.label(
                        RichText::new(
                            "Your files stay where they are. Nothing runs until you press Launch.",
                        )
                        .size(13.)
                        .color(MUTED),
                    );
                    if self.demo {
                        ui.add_space(14.);
                        ui.label(heavy("DEMO SHELF", 13.).color(YELLOW));
                    }
                });
            });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin::symmetric(26, 20))).show(ui,|ui|{
            if let Some(error)=&self.load_error{ui.colored_label(ALERT,error);}
            let filter=self.search.to_lowercase();
            let projects:Vec<_>=self.registry.projects.iter().filter(|p|(self.view=="All projects"||(self.view=="Favorites"&&p.favorite)||p.kind==self.view)&&format!("{} {} {}",p.name,p.folder.display(),p.kind).to_lowercase().contains(&filter)).cloned().collect();
            ui.label(heavy(if self.view=="All projects"{"Your ideas, within reach."}else{&self.view},30.));
            ui.add_space(10.);
            let mut actions=Vec::new();
            egui::ScrollArea::vertical().auto_shrink([false,false]).show(ui,|ui|{
                if projects.is_empty(){
                    egui::Frame::new().fill(CARD).inner_margin(34).show(ui,|ui|{
                        ui.set_width(ui.available_width());
                        ui.label(heavy(if self.registry.projects.is_empty(){"Every good app starts somewhere."}else{"Nothing on this shelf yet."},28.));
                        ui.add_space(10.);
                        ui.label(RichText::new("Add a source folder or an installed app. LaunchShelf suggests the settings, and you decide what runs.").color(MUTED));
                        ui.add_space(16.);
                        if self.registry.projects.is_empty() && ui.add(primary_button("Choose a project folder")).clicked(){self.picker(false)}
                    });
                }
                let width=ui.available_width();
                let columns=if width>960.{3}else if width>620.{2}else{1};
                for group in projects.chunks(columns){
                    ui.columns(columns,|cols|{
                        for(i,p)in group.iter().enumerate(){
                            if let Some(action)=self.card(&mut cols[i],p){actions.push(action)}
                        }
                    });
                    paint_plank(ui);
                }
                ui.add_space(6.);
                ui.label(RichText::new(format!("{} on your shelf  ·  source stays local  ·  commands are yours to review",plural(self.registry.projects.len()))).size(13.).color(MUTED));
                ui.add_space(10.);
            });
            for action in actions{self.handle(action)}
        });
        self.editor(&ctx);
        if let Some((message, error)) = self.message.clone() {
            egui::Window::new(if error {
                "Needs your attention"
            } else {
                "LaunchShelf"
            })
            .id(egui::Id::new("notice"))
            .anchor(egui::Align2::CENTER_BOTTOM, [0., -18.])
            .collapsible(false)
            .resizable(false)
            .show(&ctx, |ui| {
                ui.set_max_width(700.);
                ui.label(RichText::new(message).color(if error { ALERT } else { TEXT }));
                if text_link(ui, "Dismiss").clicked() {
                    self.message = None;
                }
            });
        }
        if let Some(p) = self.confirm_remove.clone() {
            let mut open = true;
            egui::Window::new("Remove from shelf?")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
                .show(&ctx, |ui| {
                    ui.label(format!("Take {} off the shelf?", p.name));
                    ui.label(
                        RichText::new("Your files and logs stay right where they are.")
                            .color(MUTED),
                    );
                    ui.horizontal(|ui| {
                        if ui.add(ghost_button("Remove from shelf")).clicked() {
                            let mut next = self.registry.clone();
                            next.projects.retain(|x| x.id != p.id);
                            if self.persist(next) {
                                if let Err(e) = launchshelf::unregister_desktop(&p) {
                                    self.message = Some((e, true));
                                }
                                self.confirm_remove = None;
                            }
                        }
                        if ui.add(primary_button("Keep project")).clicked() {
                            self.confirm_remove = None;
                        }
                    });
                });
            if !open {
                self.confirm_remove = None;
            }
        }
        if let Some(id) = self.log_project.clone() {
            let mut open = true;
            let text = if let Some(job) = self.jobs.get(&id) {
                launchshelf::read_log(&job.lock().unwrap().log)
            } else {
                let dir = self.store.root.join("logs").join(&id);
                let path = std::fs::read_dir(dir)
                    .ok()
                    .and_then(|files| files.filter_map(|f| f.ok()).map(|f| f.path()).max());
                path.map(|p| launchshelf::read_log(&p)).unwrap_or_else(|| {
                    "No runs yet. Launch or build this project to create a log.".into()
                })
            };
            let name = self
                .registry
                .projects
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.name.clone())
                .unwrap_or_default();
            egui::Window::new(format!("Log: {name}"))
                .id(egui::Id::new("project-log"))
                .open(&mut open)
                .collapsible(false)
                .default_width(760.)
                .default_height(380.)
                .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
                .show(&ctx, |ui| {
                    egui::ScrollArea::both().show(ui, |ui| {
                        ui.label(RichText::new(text).monospace().size(12.));
                    });
                });
            if !open {
                self.log_project = None;
            }
        }
    }
}
fn install_theme(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "atkinson".into(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/AtkinsonHyperlegible-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "atkinson-bold".into(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/AtkinsonHyperlegible-Bold.ttf"
        ))),
    );
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .insert(0, "atkinson".into());
    let mut bold = vec!["atkinson-bold".to_string()];
    bold.extend(
        fonts.families[&FontFamily::Proportional]
            .iter()
            .skip(1)
            .cloned(),
    );
    fonts.families.insert(FontFamily::Name("bold".into()), bold);
    ctx.set_fonts(fonts);
    let mut style = (*ctx.global_style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = Color32::from_rgb(33, 35, 42);
    style.visuals.window_stroke = Stroke::new(2., YELLOW);
    style.visuals.window_corner_radius = egui::CornerRadius::ZERO;
    style.visuals.window_shadow = egui::Shadow {
        offset: [8, 8],
        blur: 0,
        spread: 0,
        color: Color32::from_black_alpha(150),
    };
    style.visuals.popup_shadow = egui::Shadow {
        offset: [4, 4],
        blur: 0,
        spread: 0,
        color: Color32::from_black_alpha(150),
    };
    style.visuals.menu_corner_radius = egui::CornerRadius::ZERO;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.extreme_bg_color = INK;
    for widgets in [
        &mut style.visuals.widgets.noninteractive,
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widgets.corner_radius = egui::CornerRadius::ZERO;
    }
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1., EDGE);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1., YELLOW);
    style.visuals.widgets.active.bg_stroke = Stroke::new(2., YELLOW);
    style.visuals.selection.bg_fill = Color32::from_rgb(92, 79, 24);
    style.visuals.selection.stroke = Stroke::new(2., YELLOW);
    style.spacing.item_spacing = Vec2::new(10., 8.);
    style.spacing.button_padding = Vec2::new(13., 8.);
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(15.5));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(14.5));
    ctx.set_global_style(style);
}
fn heavy(text: &str, size: f32) -> RichText {
    RichText::new(text)
        .size(size)
        .family(FontFamily::Name("bold".into()))
}
fn eyebrow(ui: &mut egui::Ui, text: &str) {
    ui.label(heavy(text, 11.5).color(YELLOW));
}
fn primary_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(heavy(text, 15.).color(INK))
        .fill(YELLOW)
        .stroke(Stroke::NONE)
}
fn ink_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(heavy(text, 15.).color(YELLOW))
        .fill(INK)
        .stroke(Stroke::NONE)
}
fn ghost_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text).size(14.5).color(TEXT))
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::new(1., EDGE))
}
fn text_link(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(13.5), MUTED);
    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Link, true, text));
    let color = if response.hovered() { YELLOW } else { MUTED };
    ui.painter().text(
        rect.left_top(),
        egui::Align2::LEFT_TOP,
        text,
        FontId::proportional(13.5),
        color,
    );
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        Stroke::new(1., color.gamma_multiply(0.6)),
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}
fn side_item(ui: &mut egui::Ui, name: &str, count: usize, selected: bool) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.), Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, name));
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 0, YELLOW);
    } else if response.hovered() {
        painter.rect_filled(rect, 0, CARD);
    }
    let color = if selected { INK } else { TEXT };
    let font = if selected {
        FontId::new(15., FontFamily::Name("bold".into()))
    } else {
        FontId::proportional(15.)
    };
    painter.text(
        rect.left_center() + Vec2::new(10., 0.),
        egui::Align2::LEFT_CENTER,
        name,
        font,
        color,
    );
    painter.text(
        rect.right_center() - Vec2::new(10., 0.),
        egui::Align2::RIGHT_CENTER,
        count.to_string(),
        FontId::proportional(13.5),
        if selected { INK } else { MUTED },
    );
    response.clicked()
}
fn kind_color(kind: &str) -> Color32 {
    match kind {
        "Rust" => Color32::from_rgb(255, 138, 76),
        "Python" => Color32::from_rgb(92, 137, 240),
        "JavaScript" => YELLOW,
        "Desktop app" => Color32::from_rgb(238, 118, 170),
        _ => Color32::from_rgb(82, 205, 190),
    }
}
fn seed_of(text: &str) -> u32 {
    text.bytes()
        .fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619))
}
/// A little poster for each project: its own pattern from its name, its color from its kind.
fn paint_cover(
    painter: &egui::Painter,
    rect: egui::Rect,
    p: &Project,
    accent: Color32,
    running: Option<f64>,
) {
    let painter = painter.with_clip_rect(rect);
    painter.rect_filled(rect, 0, accent);
    let seed = seed_of(&p.name);
    let line = Stroke::new(2.5, Color32::from_black_alpha(46));
    let center = rect.right_center() + Vec2::new(-90., ((seed >> 8) % 50) as f32 - 25.);
    match seed % 4 {
        0 => {
            for i in 1..9 {
                painter.circle_stroke(center, i as f32 * 21., line);
            }
        }
        1 => {
            let mut x = rect.left() - rect.height();
            while x < rect.right() {
                painter.line_segment(
                    [
                        egui::pos2(x, rect.bottom()),
                        egui::pos2(x + rect.height(), rect.top()),
                    ],
                    line,
                );
                x += 20.;
            }
        }
        2 => {
            for row in 0..7 {
                for col in 0..40 {
                    let dot = rect.left_top()
                        + Vec2::new(
                            col as f32 * 18. + if row % 2 == 0 { 0. } else { 9. },
                            row as f32 * 17. + 4.,
                        );
                    painter.circle_filled(dot, 2.6, Color32::from_black_alpha(52));
                }
            }
        }
        _ => {
            for i in 0..24 {
                let angle = i as f32 / 24. * std::f32::consts::TAU;
                painter.line_segment([center, center + Vec2::angled(angle) * 400.], line);
            }
        }
    }
    if let Some(time) = running {
        // Running: a band of light sweeps the cover.
        let x = rect.left() + ((time * 0.8).fract() as f32) * (rect.width() + 160.) - 80.;
        painter.rect_filled(
            egui::Rect::from_center_size(
                egui::pos2(x, rect.center().y),
                Vec2::new(80., rect.height()),
            ),
            0,
            Color32::from_white_alpha(70),
        );
    }
    let letter = p
        .name
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    painter.text(
        rect.left_center() + Vec2::new(14., 0.),
        egui::Align2::LEFT_CENTER,
        letter,
        FontId::new(60., FontFamily::Name("bold".into())),
        INK,
    );
    let tag_font = FontId::new(12., FontFamily::Name("bold".into()));
    let tag = p.kind.to_uppercase();
    let galley = painter.layout_no_wrap(tag.clone(), tag_font.clone(), YELLOW);
    let tag_rect =
        egui::Rect::from_min_size(rect.left_bottom() + Vec2::new(0., -22.), Vec2::new(0., 22.));
    let tag_rect = egui::Rect::from_min_size(
        tag_rect.min + Vec2::new(62., -6.),
        galley.size() + Vec2::new(14., 8.),
    );
    painter.rect_filled(tag_rect, 0, INK);
    painter.text(
        tag_rect.center(),
        egui::Align2::CENTER_CENTER,
        tag,
        tag_font,
        if accent == YELLOW { TEXT } else { accent },
    );
}
/// The plank each row of projects sits on.
fn paint_plank(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.), Sense::hover());
    let painter = ui.painter();
    let plank = egui::Rect::from_min_size(rect.left_top(), Vec2::new(rect.width(), 8.));
    painter.rect_filled(plank, 0, YELLOW);
    painter.rect_filled(
        egui::Rect::from_min_size(plank.left_bottom(), Vec2::new(rect.width(), 3.)),
        0,
        Color32::from_rgb(150, 126, 22),
    );
    painter.rect_filled(
        egui::Rect::from_min_size(
            rect.left_top() + Vec2::new(0., 11.),
            Vec2::new(rect.width(), 7.),
        ),
        0,
        Color32::from_black_alpha(70),
    );
}
/// The app's mark: three boxes on a shelf, one going up.
fn shelf_mark(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(54.), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 0, INK);
    let base = rect.left_bottom() + Vec2::new(8., -12.);
    painter.rect_filled(
        egui::Rect::from_min_size(base, Vec2::new(38., 4.)),
        0,
        YELLOW,
    );
    painter.rect_filled(
        egui::Rect::from_min_size(base + Vec2::new(2., -13.), Vec2::new(10., 13.)),
        0,
        Color32::from_rgb(92, 137, 240),
    );
    painter.rect_filled(
        egui::Rect::from_min_size(base + Vec2::new(26., -17.), Vec2::new(10., 17.)),
        0,
        Color32::from_rgb(255, 138, 76),
    );
    painter.rect_filled(
        egui::Rect::from_min_size(base + Vec2::new(14., -30.), Vec2::new(10., 20.)),
        0,
        YELLOW,
    );
    painter.rect_stroke(rect, 0, Stroke::new(1., INK), StrokeKind::Inside);
}
/// `tide-tables` becomes `Tide Tables`. Names someone chose with capitals stay as they are.
fn friendly_name(name: &str) -> String {
    if name.chars().any(|c| c.is_uppercase()) {
        return name.to_string();
    }
    name.split(['-', '_', ' '])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut letters = word.chars();
            letters
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + letters.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}
fn tilde(path: &std::path::Path) -> String {
    match std::env::var_os("HOME").and_then(|home| {
        path.strip_prefix(home)
            .ok()
            .map(|rest| rest.display().to_string())
    }) {
        Some(rest) => format!("~/{rest}"),
        None => path.display().to_string(),
    }
}
fn plural(count: usize) -> String {
    if count == 1 {
        "1 project".into()
    } else {
        format!("{count} projects")
    }
}
fn shorten(text: &str, length: usize) -> String {
    if text.chars().count() > length {
        format!("{}…", text.chars().take(length - 1).collect::<String>())
    } else {
        text.into()
    }
}
fn demo_registry() -> Registry {
    let mut registry = Registry::default();
    let demo_root = std::env::temp_dir().join("launchshelf-demo-projects");
    for (i, (name, kind)) in [
        ("Orbit Notes", "Rust"),
        ("Copper Canvas", "Python"),
        ("Signal Deck", "JavaScript"),
        ("Pocket CLI", "Rust"),
        ("Tide Tables", "Python"),
        ("Sketch Viewer", "Desktop app"),
    ]
    .into_iter()
    .enumerate()
    {
        let folder = demo_root.join(name.to_lowercase().replace(' ', "-"));
        let _ = std::fs::create_dir_all(&folder);
        registry.projects.push(Project {
            id: format!("{i:024x}"),
            name: name.into(),
            folder,
            kind: kind.into(),
            launch: "printf demo".into(),
            build: if kind == "Rust" {
                "cargo build --release".into()
            } else {
                String::new()
            },
            docs: None,
            terminal: false,
            favorite: i < 2,
            created: 0,
        })
    }
    registry
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let store = Store::default_location().map_err(std::io::Error::other)?;
    if args.get(1).is_some_and(|s| s == "--detect") {
        let path = args.get(2).ok_or("Supply a project folder")?;
        let p = launchshelf::detect(std::path::Path::new(path)).map_err(std::io::Error::other)?;
        println!("{}", serde_json::to_string_pretty(&p)?);
        return Ok(());
    }
    if args.get(1).is_some_and(|s| s == "--list") {
        let state = store.load().map_err(std::io::Error::other)?;
        for p in state.projects {
            println!("{}  {}  {}", p.id, p.name, p.folder.display())
        }
        return Ok(());
    }
    if args
        .get(1)
        .is_some_and(|s| s == "--launch" || s == "--build")
    {
        let id = args.get(2).ok_or("Supply a saved project ID")?;
        let state = store.load().map_err(std::io::Error::other)?;
        let project = state
            .projects
            .iter()
            .find(|p| &p.id == id)
            .ok_or("Project is not registered")?;
        let job = launchshelf::run(&store, project, args[1] == "--build")
            .map_err(std::io::Error::other)?;
        loop {
            let j = job.lock().unwrap().clone();
            if j.done {
                println!("{}\nLog: {}", j.status, j.log.display());
                if !j.success {
                    std::process::exit(1)
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        return Ok(());
    }
    if args.get(1).is_some_and(|s| s == "--help") {
        println!(
            "LaunchShelf — a home for the apps you build for the apps you build\n\nlaunchshelf                 Open the native desktop app\nlaunchshelf --detect DIR    Preview suggested project settings\nlaunchshelf --list          List saved projects\nlaunchshelf --launch ID     Run a saved launch command\nlaunchshelf --build ID      Run a saved build command\nlaunchshelf --demo          Show synthetic projects without saving\n\nLAUNCHSHELF_DATA overrides the library directory."
        );
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1240., 850.])
            .with_min_inner_size([880., 620.])
            .with_app_id("launchshelf"),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    let demo = args.iter().any(|s| s == "--demo");
    eframe::run_native(
        "LaunchShelf",
        options,
        Box::new(move |cc| Ok(Box::new(Workshop::new(cc, store, demo)))),
    )?;
    Ok(())
}

/// Pictures and the demo video for the website: the real app, drawn off screen, with made-up projects.
/// `cargo test --bin launchshelf capture -- --ignored --test-threads=1`, with CAPTURE_DIR set.
#[cfg(test)]
mod capture {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    use std::{
        io::Write,
        path::Path,
        process::{Command, Stdio},
    };

    const SIZE: [u32; 2] = [1280, 800];

    fn git(folder: &Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(folder)
            .args(["-c", "user.name=Demo", "-c", "user.email=demo@example.com"])
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
    }

    /// A small real project: a Git repo with a script that really runs.
    fn make_project(home: &Path, slug: &str, lines: &[&str], dirty: bool) -> PathBuf {
        let folder = home.join("Projects").join(slug);
        std::fs::create_dir_all(&folder).unwrap();
        let body: String = lines
            .iter()
            .map(|l| format!("print({l:?}, flush=True); time.sleep(4)\n"))
            .collect();
        std::fs::write(folder.join("app.py"), format!("import time\n{body}")).unwrap();
        git(&folder, &["init", "-q", "-b", "main"]);
        git(&folder, &["add", "."]);
        git(&folder, &["commit", "-qm", "First"]);
        if dirty {
            std::fs::write(folder.join("todo.txt"), "more\n").unwrap();
        }
        folder
    }

    fn shelf(root: &Path) -> (Store, PathBuf) {
        let home = root.join("sam");
        // SAFETY: capture tests run alone, on one thread, before the app starts.
        unsafe {
            std::env::set_var("HOME", &home);
            std::env::set_var("LAUNCHSHELF_DATA", root.join("library"));
        }
        let store = Store::default_location().unwrap();
        let mut registry = Registry::default();
        for (i, (name, slug, kind, lines, dirty, favorite)) in [
            (
                "Orbit Notes",
                "orbit-notes",
                "Rust",
                vec!["Opening your notes", "Loading 42 notes", "Ready"],
                false,
                true,
            ),
            (
                "Copper Canvas",
                "copper-canvas",
                "Python",
                vec!["Warming up the canvas", "Ready"],
                true,
                true,
            ),
            (
                "Signal Deck",
                "signal-deck",
                "JavaScript",
                vec!["Starting on port 5173", "Ready"],
                false,
                false,
            ),
            (
                "Pocket CLI",
                "pocket-cli",
                "Rust",
                vec!["Pocket CLI 0.3", "Done"],
                false,
                false,
            ),
            (
                "Sketch Viewer",
                "sketch-viewer",
                "Desktop app",
                vec!["Opening sketches", "Ready"],
                true,
                false,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let folder = make_project(&home, slug, &lines, dirty);
            registry.projects.push(Project {
                id: format!("{i:024x}"),
                name: name.into(),
                folder,
                kind: kind.into(),
                launch: "python3 app.py".into(),
                build: if kind == "Rust" {
                    "cargo build --release".into()
                } else {
                    String::new()
                },
                docs: None,
                terminal: false,
                favorite,
                created: 0,
            });
        }
        store.save(&mut registry).unwrap();
        (store, home)
    }

    #[derive(Debug)]
    struct DroppedFolder(PathBuf);
    impl egui::DroppedFile for DroppedFolder {
        fn path(&self) -> &Path {
            &self.0
        }
        fn bytes(&self) -> Result<Vec<u8>, String> {
            Err("A folder has no bytes.".into())
        }
    }

    struct Film<'a> {
        harness: Harness<'a, Workshop>,
        cursor: egui::Pos2,
        ffmpeg: Option<std::process::Child>,
    }
    impl Film<'_> {
        fn frame(&mut self) {
            self.harness.hover_at(self.cursor);
            self.harness.step();
            let mut image = self.harness.render().unwrap();
            draw_cursor(&mut image, self.cursor);
            if let Some(ffmpeg) = &mut self.ffmpeg {
                ffmpeg
                    .stdin
                    .as_mut()
                    .unwrap()
                    .write_all(image.as_raw())
                    .unwrap();
            }
        }
        fn hold(&mut self, seconds: f32) {
            for _ in 0..(seconds * 30.) as usize {
                self.frame();
            }
        }
        fn glide(&mut self, to: egui::Pos2) {
            let from = self.cursor;
            for i in 1..=18 {
                let t = i as f32 / 18.;
                self.cursor = from + (to - from) * (t * t * (3. - 2. * t));
                self.frame();
            }
        }
        fn press(&mut self) {
            for pressed in [true, false] {
                self.harness.event(egui::Event::PointerButton {
                    pos: self.cursor,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                });
                self.frame();
            }
        }
        fn click_label(&mut self, label: &str, nth: usize) {
            let target = {
                let nodes: Vec<_> = self.harness.get_all_by_label(label).collect();
                nodes[nth].rect().center()
            };
            self.glide(target);
            self.hold(0.15);
            self.press();
        }
        fn shot(&mut self, path: PathBuf) {
            self.harness.remove_cursor();
            self.harness.step();
            self.harness.step();
            self.harness.render().unwrap().save(path).unwrap();
        }
    }

    fn draw_cursor(image: &mut image::RgbaImage, at: egui::Pos2) {
        let shape = [
            (0., 0.),
            (0., 21.),
            (5., 16.),
            (9., 25.),
            (13., 23.),
            (9., 15.),
            (16., 15.),
        ];
        let inside = |x: f32, y: f32, grow: f32| {
            let mut hit = false;
            for i in 0..shape.len() {
                let (ax, ay) = shape[i];
                let (bx, by) = shape[(i + 1) % shape.len()];
                if (ay > y) != (by > y) && x < (bx - ax) * (y - ay) / (by - ay) + ax {
                    hit = !hit;
                }
            }
            hit || (grow > 0.
                && [(-1.5, 0.), (1.5, 0.), (0., -1.5), (0., 1.5)].iter().any(
                    |(dx, dy): &(f32, f32)| {
                        let mut h = false;
                        for i in 0..shape.len() {
                            let (ax, ay) = shape[i];
                            let (bx, by) = shape[(i + 1) % shape.len()];
                            if (ay > y + dy) != (by > y + dy)
                                && x + dx < (bx - ax) * (y + dy - ay) / (by - ay) + ax
                            {
                                h = !h;
                            }
                        }
                        h
                    },
                ))
        };
        for y in 0..30 {
            for x in 0..22 {
                let (px, py) = (at.x as i32 + x - 2, at.y as i32 + y - 2);
                if px < 0 || py < 0 || px as u32 >= image.width() || py as u32 >= image.height() {
                    continue;
                }
                let (fx, fy) = ((x - 2) as f32, (y - 2) as f32);
                if inside(fx, fy, 0.) {
                    image.get_pixel_mut(px as u32, py as u32).0 = [255, 255, 255, 255];
                } else if inside(fx, fy, 1.) {
                    image.get_pixel_mut(px as u32, py as u32).0 = [0, 0, 0, 255];
                }
            }
        }
    }

    fn film<'a>(store: Store, video: Option<&Path>) -> Film<'a> {
        let harness = Harness::builder()
            .with_size(Vec2::new(SIZE[0] as f32, SIZE[1] as f32))
            .with_step_dt(1. / 30.)
            .build_eframe(move |cc| Workshop::new(cc, store, false));
        let ffmpeg = video.map(|path| {
            Command::new("ffmpeg")
                .args([
                    "-y",
                    "-loglevel",
                    "error",
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "rgba",
                    "-s",
                    &format!("{}x{}", SIZE[0], SIZE[1]),
                    "-r",
                    "30",
                    "-i",
                    "-",
                    "-c:v",
                    "libx264",
                    "-pix_fmt",
                    "yuv420p",
                    "-crf",
                    "20",
                    "-movflags",
                    "+faststart",
                ])
                .arg(path)
                .stdin(Stdio::piped())
                .spawn()
                .unwrap()
        });
        Film {
            harness,
            cursor: egui::pos2(900., 300.),
            ffmpeg,
        }
    }

    #[test]
    fn friendly_names() {
        assert_eq!(friendly_name("tide-tables"), "Tide Tables");
        assert_eq!(friendly_name("pocket_cli"), "Pocket Cli");
        assert_eq!(friendly_name("FormForge"), "FormForge");
    }

    #[test]
    #[ignore]
    fn capture() {
        let out = PathBuf::from(std::env::var("CAPTURE_DIR").expect("set CAPTURE_DIR"));
        std::fs::create_dir_all(&out).unwrap();
        // A short neutral path, because the log window shows it.
        let root = PathBuf::from("/tmp/home");
        let _ = std::fs::remove_dir_all(&root);
        let (store, home) = shelf(&root);
        let new_project = home.join("Projects/tide-tables");
        std::fs::create_dir_all(&new_project).unwrap();
        std::fs::write(new_project.join("main.py"), "import time\nprint('Tide tables for today', flush=True)\ntime.sleep(9)\nprint('High tide 6:42 AM', flush=True)\n").unwrap();

        let mut f = film(store, Some(&out.join("launchshelf-demo.mp4")));
        f.harness.step();
        std::thread::sleep(Duration::from_millis(500));
        f.hold(2.0);
        f.shot(out.join("shelf.png"));
        // Launch a project for real.
        f.click_label("Launch", 0);
        f.hold(0.5);
        f.harness.step();
        let mut image = f.harness.render().unwrap();
        draw_cursor(&mut image, f.cursor);
        image.save(out.join("running.png")).unwrap();
        while f
            .harness
            .state()
            .jobs
            .values()
            .any(|j| !j.lock().unwrap().done)
        {
            f.frame();
        }
        f.hold(1.0);
        // Read what it printed.
        f.click_label("Log", 0);
        f.hold(2.2);
        f.shot(out.join("log.png"));
        f.harness.state_mut().log_project = None;
        f.hold(0.3);
        // Drop a new folder in.
        f.glide(egui::pos2(760., 420.));
        f.harness
            .input_mut()
            .dropped_files
            .push(Arc::new(DroppedFolder(new_project.clone())));
        f.hold(2.4);
        f.shot(out.join("add.png"));
        f.click_label("Put it on the shelf", 0);
        f.hold(1.6);
        f.harness.state_mut().message = None;
        f.harness.state_mut().refresh_git();
        f.hold(1.0);
        // Find it by typing.
        let search = f
            .harness
            .get_by_role(egui::accesskit::Role::TextInput)
            .rect()
            .center();
        f.glide(search);
        f.press();
        for letter in "tide".chars() {
            f.harness.event(egui::Event::Text(letter.to_string()));
            f.hold(0.18);
        }
        f.hold(1.2);
        f.click_label("Launch", 0);
        f.hold(0.3);
        while f
            .harness
            .state()
            .jobs
            .values()
            .any(|j| !j.lock().unwrap().done)
        {
            f.frame();
        }
        f.hold(1.6);
        f.harness.state_mut().search.clear();
        f.hold(2.0);
        f.shot(out.join("final.png"));
        let mut ffmpeg = f.ffmpeg.take().unwrap();
        drop(ffmpeg.stdin.take());
        assert!(ffmpeg.wait().unwrap().success());
    }
}
