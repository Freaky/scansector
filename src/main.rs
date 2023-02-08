use std::path::{Path, PathBuf};
use std::thread::JoinHandle;

use eframe::egui;
use egui::plot::{Text, PlotPoint};
use egui::{ComboBox, Ui};
use roxmltree::Node;

#[derive(Debug, Clone, Copy)]
struct Position {
    x: f64,
    y: f64,
}

#[derive(Clone, Debug)]
struct Object {
    name: String,
    planet: bool,
    pos: Position,
    mission: bool,
}

#[derive(Clone, Debug)]
struct System {
    name: String,
    objects: Vec<Object>,
}

fn load_save(path: &Path) -> std::io::Result<Vec<System>> {
    let save = std::fs::read_to_string(path)?;
    let doc = roxmltree::Document::parse(&save).unwrap();
    let mut systems = vec![];

    for sys in doc.descendants().filter(|n| n.tag_name().name() == "Sstm") {
        let Some(name) = sys.attribute("bN") else { continue };
        let mut system = System {
            name: name.to_string(),
            objects: vec![],
        };

        for planet in sys.descendants().filter(|n| n.tag_name().name() == "Plnt") {
            let Some(mut planet) = extract_object(&planet) else { continue };
            planet.planet = true;
            system.objects.push(planet);
        }

        for ent in sys.descendants().filter(|n| n.tag_name().name() == "CCEnt") {
            let Some(ent) = extract_object(&ent) else { continue };
            system.objects.push(ent);
        }
        systems.push(system);
    }

    systems.sort_unstable_by_key(|s| s.name.clone());

    Ok(systems)
}

fn parse_vector(v: &str) -> Option<Position> {
    let mut split = v.split(|b| b == '|');
    let x = split.next()?.parse().ok()?;
    let y = split.next()?.parse().ok()?;
    Some(Position { x, y })
}

fn extract_object(node: &Node) -> Option<Object> {
    let loc = node.descendants().find(|n| n.tag_name().name() == "loc")?;
    let loc = parse_vector(loc.text()?)?;

    let mission = node.descendants().any(|n| n.tag_name().name() == "MReq");

    let what = node.descendants().find(|n| n.tag_name().name() == "j0")?;
    let what = json::parse(what.text()?).ok()?;

    Some(Object {
        name: what
            .entries()
            .find(|e| e.0 == "f0")?
            .1
            .as_str()?
            .to_string(),
        planet: false,
        pos: loc,
        mission,
    })
}

#[derive(Debug, Default)]
struct ScanSectorUi {
    pick_file: Option<JoinHandle<Option<PathBuf>>>,
    message: Option<String>,
    save: Option<PathBuf>,
    systems: Vec<System>,
    filter: String,
    selected: usize,
}

impl ScanSectorUi {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }
}

impl eframe::App for ScanSectorUi {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::gui_zoom::zoom_with_keyboard_shortcuts(ctx, frame.info().native_pixels_per_point);

        egui::TopBottomPanel::top("footer").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.menu_button("View", |ui| {
                    egui::gui_zoom::zoom_menu_buttons(ui, frame.info().native_pixels_per_point);

                    egui::widgets::global_dark_light_mode_buttons(ui);
                });

                if self
                    .pick_file
                    .as_ref()
                    .map(|t| t.is_finished())
                    .unwrap_or(false)
                {
                    let jh = self.pick_file.take().unwrap();
                    self.save = jh.join().unwrap();

                    if let Some(path) = &self.save {
                        match load_save(path) {
                            Ok(systems) => {
                                self.systems = systems;
                                self.message = None;
                            }
                            Err(e) => {
                                self.message = Some(e.to_string());
                            }
                        }
                    }
                }

                ui.add_enabled_ui(self.pick_file.is_none(), |ui| {
                    if ui.button("Pick Save").clicked() {
                        self.pick_file = Some(std::thread::spawn(move || {
                            rfd::FileDialog::new()
                                .add_filter("XML", &["xml"])
                                .pick_file()
                        }));
                    }
                });

                if let Some(path) = &self.save {
                    ui.heading(path.to_string_lossy());
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(message) = &self.message {
                ui.label(message.clone());
            }

            if !self.systems.is_empty() {
                ui.group(|ui| {
                    ui.heading("Select a System");
                    ui.horizontal(|ui| {
                        ui.label("Filter");
                        ui.text_edit_singleline(&mut self.filter);

                        ComboBox::from_id_source("_star_system_select")
                            .width(ui.available_width())
                            .selected_text(self.systems[self.selected].name.clone())
                            .show_ui(ui, |ui| {
                                for (index, system) in self.systems.iter().enumerate() {
                                    if system
                                        .name
                                        .to_lowercase()
                                        .contains(&self.filter.to_lowercase())
                                    {
                                        ui.selectable_value(
                                            &mut self.selected,
                                            index,
                                            &system.name,
                                        );
                                    }
                                }
                            });
                    });
                });

                render_system(ui, &self.systems[self.selected]);
            }
        });
    }
}

fn render_system(ui: &mut Ui, system: &System) {
    ui.heading(format!("Current System: {}", system.name));

    if system.objects.is_empty() {
        ui.label("Spooky empty system");
        return;
    }

    let bounds_x = system
        .objects
        .iter()
        .map(|s| s.pos.x.abs())
        .reduce(f64::max)
        .unwrap()
        + 2000.0;
    let bounds_y = system
        .objects
        .iter()
        .map(|s| s.pos.y.abs())
        .reduce(f64::max)
        .unwrap()
        + 2000.0;

    use eframe::egui::plot::{Legend, MarkerShape, Plot, Points};
    let plot = Plot::new("system_display")
        .data_aspect(1.0)
        .include_x(bounds_x)
        .include_x(-bounds_x)
        .include_y(bounds_y)
        .include_y(-bounds_y)
        .auto_bounds_x()
        .auto_bounds_y()
        .legend(Legend::default());

    plot.show(ui, |ui| {
        for object in &system.objects {
            let points = Points::new(vec![[object.pos.x, object.pos.y]])
                .name(object.name.to_string())
                .filled(true)
                .radius(10.0)
                .shape(if object.planet {
                    MarkerShape::Circle
                } else if object.mission {
                    MarkerShape::Asterisk
                } else {
                    MarkerShape::Cross
                });

            ui.points(points);
            ui.text(
                Text::new(PlotPoint::new(object.pos.x, object.pos.y), &object.name)
                    .name(object.name.clone()),
            );
        }
    });
}

fn main() {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "Scansector - Starsector System Scanner",
        native_options,
        Box::new(|cc| Box::new(ScanSectorUi::new(cc))),
    ).unwrap();
}
