use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use launchshelf::{Project, Registry, SharedJob, Store};
use std::{collections::HashMap, path::PathBuf, sync::mpsc, time::Duration};

const BG: Color32 = Color32::from_rgb(12, 17, 25);
const PANEL: Color32 = Color32::from_rgb(19, 28, 39);
const CARD: Color32 = Color32::from_rgb(24, 35, 48);
const EDGE: Color32 = Color32::from_rgb(45, 61, 78);
const TEXT: Color32 = Color32::from_rgb(233, 239, 247);
const MUTED: Color32 = Color32::from_rgb(145, 163, 183);
const MINT: Color32 = Color32::from_rgb(110, 229, 197);
const COPPER: Color32 = Color32::from_rgb(241, 183, 139);
const COLORS: [Color32; 4] = [
    MINT,
    COPPER,
    Color32::from_rgb(151, 173, 252),
    Color32::from_rgb(235, 144, 178),
];
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
}
impl Workshop {
    fn new(cc: &eframe::CreationContext<'_>, store: Store, demo: bool) -> Self {
        let mut style = (*cc.egui_ctx.global_style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = BG;
        style.visuals.window_fill = PANEL;
        style.visuals.override_text_color = Some(TEXT);
        style.visuals.widgets.inactive.bg_fill = CARD;
        style.visuals.widgets.inactive.weak_bg_fill = CARD;
        style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(8);
        style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(8);
        style.visuals.widgets.inactive.bg_stroke = Stroke::new(1., EDGE);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(40, 59, 73);
        style.visuals.widgets.hovered.bg_stroke = Stroke::new(1., MINT);
        style.visuals.selection.bg_fill = Color32::from_rgb(40, 82, 74);
        style.visuals.selection.stroke = Stroke::new(1., MINT);
        style.spacing.item_spacing = Vec2::new(10., 10.);
        style.spacing.button_padding = Vec2::new(14., 9.);
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(15.));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.));
        cc.egui_ctx.set_global_style(style);
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
                        self.message = Some((
                            format!(
                                "{} started for {}",
                                if build { "Build" } else { "Launch" },
                                p.name
                            ),
                            false,
                        ));
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
    fn card(&self, ui: &mut egui::Ui, p: &Project, index: usize) -> Option<Action> {
        let mut action = None;
        let accent = COLORS[index % COLORS.len()];
        egui::Frame::new()
            .fill(CARD)
            .stroke(Stroke::new(1., EDGE))
            .corner_radius(14)
            .inner_margin(20)
            .show(ui, |ui| {
                ui.set_min_height(220.);
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    project_mark(ui, accent, &p.kind);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&p.name).size(21.).strong());
                        ui.label(RichText::new(&p.kind).size(12.).color(accent));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(if p.favorite { "★" } else { "☆" })
                            .on_hover_text("Favorite")
                            .clicked()
                        {
                            action = Some(Action::Favorite(p.id.clone()));
                        }
                    });
                });
                ui.add_space(10.);
                ui.label(
                    RichText::new(shorten(&p.folder.to_string_lossy(), 58))
                        .size(12.)
                        .color(MUTED),
                )
                .on_hover_text(p.folder.display().to_string());
                ui.horizontal(|ui| {
                    if let Some(Some(g)) = self.git.get(&p.id) {
                        ui.label(
                            RichText::new(format!(
                                "{}  ·  {}",
                                g.branch,
                                if g.changes == 0 {
                                    "clean".into()
                                } else {
                                    format!("{} changes", g.changes)
                                }
                            ))
                            .size(12.)
                            .color(if g.changes == 0 {
                                MINT
                            } else {
                                COPPER
                            }),
                        );
                    } else {
                        ui.label(
                            RichText::new(if self.git_loading {
                                "Checking Git…"
                            } else {
                                "No Git repository"
                            })
                            .size(12.)
                            .color(MUTED),
                        );
                    }
                });
                ui.add_space(10.);
                let running = self
                    .jobs
                    .get(&p.id)
                    .is_some_and(|j| !j.lock().unwrap().done);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !running,
                            egui::Button::new(RichText::new("Launch").strong().color(BG))
                                .fill(accent),
                        )
                        .clicked()
                    {
                        action = Some(Action::Launch(p.clone(), false));
                    }
                    if !p.build.is_empty()
                        && ui
                            .add_enabled(!running, egui::Button::new("Build"))
                            .clicked()
                    {
                        action = Some(Action::Launch(p.clone(), true));
                    }
                    if ui.button("Source").clicked() {
                        action = Some(Action::Folder(p.folder.clone()));
                    }
                    if let Some(path) = &p.docs
                        && ui.button("Docs").clicked()
                    {
                        action = Some(Action::Docs(path.clone()));
                    }
                });
                ui.horizontal(|ui| {
                    if ui.small_button("Configure").clicked() {
                        action = Some(Action::Edit(p.clone()));
                    }
                    if ui
                        .small_button("Add launcher")
                        .on_hover_text("Create a desktop entry for this saved project")
                        .clicked()
                    {
                        action = Some(Action::Register(p.clone()));
                    }
                    if ui.small_button("Log").clicked() {
                        action = Some(Action::Log(p.id.clone()));
                    }
                });
                if let Some(job) = self.jobs.get(&p.id) {
                    let job = job.lock().unwrap();
                    ui.label(RichText::new(shorten(&job.status, 65)).size(12.).color(
                        if job.done && !job.success {
                            COPPER
                        } else {
                            MINT
                        },
                    ));
                } else {
                    ui.label(
                        RichText::new(if p.folder.exists() {
                            "Configured locally · ready to run"
                        } else {
                            "Source folder is missing · configure its new location"
                        })
                        .size(12.)
                        .color(if p.folder.exists() {
                            MUTED
                        } else {
                            COPPER
                        }),
                    );
                }
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
        egui::Window::new(if self.editor_new{"Add your project"}else{"Configure project"}).id(egui::Id::new("project-editor")).open(&mut open).collapsible(false).resizable(false).default_width(580.).show(ctx,|ui|{
            eyebrow(ui,"REVIEW BEFORE RUNNING");ui.label(RichText::new("Your project. Your launch settings.").size(22.).strong());ui.add_space(12.);
            ui.label("Name");ui.text_edit_singleline(&mut project.name);
            let mut folder=project.folder.to_string_lossy().into_owned();ui.label("Source folder / working directory");if ui.text_edit_singleline(&mut folder).changed(){project.folder=PathBuf::from(folder)}
            ui.label("Project type");ui.text_edit_singleline(&mut project.kind);
            ui.label("Launch command");ui.add(egui::TextEdit::singleline(&mut project.launch).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY));
            ui.label("Build command (optional)");ui.add(egui::TextEdit::singleline(&mut project.build).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY));
            ui.label("Documentation file (optional)");let mut docs=project.docs.as_ref().map(|p|p.to_string_lossy().into_owned()).unwrap_or_default();if ui.text_edit_singleline(&mut docs).changed(){project.docs=if docs.trim().is_empty(){None}else{Some(PathBuf::from(docs))}}
            ui.checkbox(&mut project.terminal,"Launch in a terminal");ui.checkbox(&mut project.favorite,"Pin to favorites");
            ui.label(RichText::new("Arguments respect quotes. No automatic shell expansion. For pipelines or &&, explicitly use bash -lc '…'. Saving does not run the command.").size(12.).color(MUTED));
            ui.add_space(10.);ui.horizontal(|ui|{save=ui.add(egui::Button::new(RichText::new("Save project").color(BG).strong()).fill(MINT)).clicked();cancel=ui.button("Cancel").clicked();if !self.editor_new&&ui.button("Remove from library…").clicked(){self.confirm_remove=Some(project.clone());cancel=true;}});
        });
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
                            Some(("Project saved. Launch it when you're ready.".into(), false));
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
                Event::Candidate(Ok(p)) => {
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
                    Ok(p) => {
                        self.editor = Some(p);
                        self.editor_new = true;
                    }
                    Err(e) => self.message = Some((e, true)),
                }
            }
        }
        egui::Panel::top("top")
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(28, 20)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    project_mark(ui, MINT, "LaunchShelf");
                    ui.vertical(|ui| {
                        ui.label(RichText::new("LaunchShelf").size(28.).strong());
                        ui.label(
                            RichText::new("A home for the apps you build.")
                                .size(13.)
                                .color(MUTED),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(if self.demo {
                                "DEMO WORKSPACE"
                            } else {
                                "LOCAL WORKSPACE  /  0.1"
                            })
                            .size(11.)
                            .color(MINT),
                        );
                    });
                });
            });
        egui::Panel::left("side")
            .exact_size(205.)
            .resizable(false)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(22))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    eyebrow(ui, "YOUR WORKSHOP");
                    ui.add_space(16.);
                    for name in ["All projects", "Favorites", "Rust", "Python", "JavaScript", "Desktop app"] {
                        let count = self.registry.projects.iter().filter(|p| {
                            name == "All projects" || (name == "Favorites" && p.favorite) || p.kind == name
                        }).count();
                        if ui.add_sized([160., 38.], egui::Button::new(format!("{name}   {count}")).selected(self.view == name)).clicked() {
                            self.view = name.into();
                        }
                    }
                    ui.add_space(30.);
                    eyebrow(ui, "BRING SOMETHING IN");
                    ui.add_space(10.);
                    if ui.add_sized([160., 38.], egui::Button::new("+ Project folder")).clicked() { self.picker(false); }
                    if ui.add_sized([160., 38.], egui::Button::new("Import desktop app")).clicked() { self.picker(true); }
                    ui.add_space(12.);
                    ui.label(RichText::new("Drop a folder into this window. Review its settings. Make it yours.").size(13.).color(MUTED));
                    ui.add_space(30.);
                    if ui.button("Refresh Git status").clicked() { self.refresh_git(); }
                    if ui.button("Reload library").clicked() {
                        match self.store.load() {
                            Ok(r) => { self.registry = r; self.load_error = None; self.refresh_git(); }
                            Err(e) => self.message = Some((e, true)),
                        }
                    }
                    ui.add_space(12.);
                    ui.label(RichText::new("Your source files stay where they are. LaunchShelf stores references and logs.").size(12.).color(MUTED));
                });
            });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG).inner_margin(28)).show(ui,|ui|{
            if let Some(error)=&self.load_error{ui.colored_label(COPPER,error);}
            ui.horizontal(|ui|{ui.vertical(|ui|{eyebrow(ui,"MAKE / BUILD / LAUNCH");ui.label(RichText::new(if self.view=="All projects"{"Your ideas, within reach."}else{&self.view}).size(30.).strong());});ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if ui.add(egui::Button::new(RichText::new("+ Add project").strong().color(BG)).fill(MINT)).clicked(){self.picker(false)}});});
            ui.add_space(12.);ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search names, folders or project types…").desired_width(f32::INFINITY));ui.add_space(18.);
            let filter=self.search.to_lowercase();let projects:Vec<_>=self.registry.projects.iter().filter(|p|(self.view=="All projects"||(self.view=="Favorites"&&p.favorite)||p.kind==self.view)&&format!("{} {} {}",p.name,p.folder.display(),p.kind).to_lowercase().contains(&filter)).cloned().collect();
            let mut actions=Vec::new();egui::ScrollArea::vertical().show(ui,|ui|{
                if projects.is_empty(){egui::Frame::new().fill(CARD).corner_radius(16).inner_margin(36).show(ui,|ui|{ui.set_min_width(ui.available_width());ui.label(RichText::new(if self.registry.projects.is_empty(){"Every good app starts somewhere."}else{"No projects match this view."}).size(26.).strong());ui.add_space(12.);ui.label(RichText::new("Add a source folder or import an installed desktop app. We'll suggest the settings; you decide what runs.").color(MUTED));ui.add_space(18.);if ui.button("Choose a project folder").clicked(){self.picker(false)}});}
                let columns=if ui.available_width()>720.{2}else{1};
                for (row,group) in projects.chunks(columns).enumerate(){ui.columns(columns,|cols|{for(i,p)in group.iter().enumerate(){if let Some(action)=self.card(&mut cols[i],p,row*columns+i){actions.push(action)}}});ui.add_space(16.);}
                ui.add_space(8.);ui.label(RichText::new(format!("{} projects in your library  ·  source stays local  ·  commands are yours to review",self.registry.projects.len())).size(12.).color(MUTED));
            });for action in actions{self.handle(action)}
        });
        self.editor(&ctx);
        if let Some((message, error)) = self.message.clone() {
            egui::Window::new(if error {
                "Something needs attention"
            } else {
                "Workshop update"
            })
            .id(egui::Id::new("notice"))
            .anchor(egui::Align2::CENTER_BOTTOM, [0., -18.])
            .collapsible(false)
            .resizable(false)
            .show(&ctx, |ui| {
                ui.set_max_width(700.);
                ui.label(RichText::new(message).color(if error { COPPER } else { MINT }));
                if ui.small_button("Dismiss").clicked() {
                    self.message = None;
                }
            });
        }
        if let Some(p) = self.confirm_remove.clone() {
            let mut open = true;
            egui::Window::new("Remove project?")
                .open(&mut open)
                .collapsible(false)
                .show(&ctx, |ui| {
                    ui.label(format!(
                        "Remove {} from LaunchShelf and remove its managed desktop entry?",
                        p.name
                    ));
                    ui.label(
                        RichText::new("Source files and logs will be preserved.").color(MUTED),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Remove from library").clicked() {
                            let mut next = self.registry.clone();
                            next.projects.retain(|x| x.id != p.id);
                            if self.persist(next) {
                                if let Err(e) = launchshelf::unregister_desktop(&p) {
                                    self.message = Some((e, true));
                                }
                                self.confirm_remove = None;
                            }
                        }
                        if ui.button("Keep project").clicked() {
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
            egui::Window::new("Project process log")
                .open(&mut open)
                .default_width(820.)
                .default_height(440.)
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
fn eyebrow(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(11.).color(MINT).strong());
}
fn shorten(text: &str, length: usize) -> String {
    if text.chars().count() > length {
        format!("{}…", text.chars().take(length - 1).collect::<String>())
    } else {
        text.into()
    }
}
fn project_mark(ui: &mut egui::Ui, color: Color32, kind: &str) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(46.), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(r, 10, Color32::from_rgb(31, 48, 57));
    let c = r.center();
    let points = vec![
        c + Vec2::new(-12., -8.),
        c + Vec2::new(0., -14.),
        c + Vec2::new(12., -8.),
        c + Vec2::new(12., 8.),
        c + Vec2::new(0., 14.),
        c + Vec2::new(-12., 8.),
    ];
    painter.add(egui::Shape::closed_line(points, Stroke::new(2., color)));
    painter.text(
        c,
        egui::Align2::CENTER_CENTER,
        kind.chars().next().unwrap_or('L'),
        egui::FontId::proportional(15.),
        color,
    );
}
fn demo_registry() -> Registry {
    let mut registry = Registry::default();
    let demo_root = std::env::temp_dir().join("launchshelf-demo-projects");
    for (i, (name, kind)) in [
        ("Orbit Notes", "Rust"),
        ("Copper Canvas", "Python"),
        ("Signal Deck", "JavaScript"),
        ("Pocket CLI", "Rust"),
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
            "LaunchShelf — a local workshop for the apps you build\n\nlaunchshelf                 Open the native desktop app\nlaunchshelf --detect DIR    Preview suggested project settings\nlaunchshelf --list          List saved projects\nlaunchshelf --launch ID     Run a saved launch command\nlaunchshelf --build ID      Run a saved build command\nlaunchshelf --demo          Show synthetic projects without saving\n\nLAUNCHSHELF_DATA overrides the library directory."
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
