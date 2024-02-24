use bevy::prelude::*;
use bevy::window::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin};
use bevy::winit::WinitWindows;
use winit::window::Icon;

struct Images {
    bevy_icon: Handle<Image>,
    bevy_icon_inverted: Handle<Image>,
}

impl FromWorld for Images {
    fn from_world(world: &mut World) -> Self {
        let asset_server = world.get_resource_mut::<AssetServer>().unwrap();
        Self {
            bevy_icon: asset_server.load("icon.png"),
            bevy_icon_inverted: asset_server.load("icon_inverted.png"),
        }
    }
}

const CAMERA_TARGET: Vec3 = Vec3::ZERO;

#[derive(Resource, Deref, DerefMut)]
struct OriginalCameraTransform(Transform);

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::rgb(0.0, 0.0, 0.0)))
        .insert_resource(Msaa::Sample4)
        .init_resource::<UiState>()
        .add_plugins(DefaultPlugins.set(WindowPlugin{
            primary_window: Some(Window {
                title: "geese edit".into(),
                resizable: true,
                decorations: true,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin)
        .add_systems(Startup, set_window_icon)
        .add_systems(Startup, configure_visuals_system)
        .add_systems(Startup, configure_ui_state_system)
        .add_systems(Startup, setup_system)
        .add_systems(Update, ui_example_system)
        .run();
}

#[derive(Default, Resource)]
struct UiState {
    pub label: String,
    pub value1: f32,
    pub painting: Painting,
    pub inverted: bool,
    pub egui_texture_handle: Option<egui::TextureHandle>,
    pub is_window_open: bool,
}

fn configure_visuals_system(mut contexts: EguiContexts, mut windows: Query<&mut Window>) {
    contexts.ctx_mut().set_visuals(egui::Visuals {
        window_rounding: 0.0.into(),
        ..Default::default()
    });

    let mut window = windows.single_mut();
    window.set_maximized(true);
}

fn configure_ui_state_system(mut ui_state: ResMut<UiState>) {
    ui_state.is_window_open = true;
}

pub fn set_window_icon(
    main_window: Query<Entity, With<PrimaryWindow>>,
    windows: NonSend<WinitWindows>,
) {
    let Some(primary) = windows.get_window(main_window.single()) else {return};

    let (icon_rgba, icon_width, icon_height) = {
        let image = image::open("./assets/icon.png")
            .expect("Failed to open icon path")
            .into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };

    let icon = Icon::from_rgba(icon_rgba, icon_width, icon_height).unwrap();
    primary.set_window_icon(Some(icon));
}

fn setup_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn(PbrBundle {
        mesh: meshes.add(Plane3d::default().mesh().size(5.0, 5.0)),
        material: materials.add(Color::rgb(0.3, 0.5, 0.3)),
        ..Default::default()
    });
    commands.spawn(PbrBundle {
        mesh: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        material: materials.add(Color::rgb(0.8, 0.7, 0.6)),
        transform: Transform::from_xyz(0.0, 0.5, 0.0),
        ..Default::default()
    });
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 1500.0,
            shadows_enabled: true,
            ..Default::default()
        },
        transform: Transform::from_xyz(4.0, 8.0, 4.0),
        ..Default::default()
    });

    let camera_pos = Vec3::new(-2.0, 2.5, 5.0);
    let camera_transform =
        Transform::from_translation(camera_pos).looking_at(CAMERA_TARGET, Vec3::Y);
    commands.insert_resource(OriginalCameraTransform(camera_transform));

    commands.spawn(Camera3dBundle {
        transform: camera_transform,
        ..Default::default()
    });
}

fn ui_example_system(
    mut ui_state: ResMut<UiState>,
    // You are not required to store Egui texture ids in systems. We store this one here just to
    // demonstrate that rendering by using a texture id of a removed image is handled without
    // making bevy_egui panic.
    mut rendered_texture_id: Local<egui::TextureId>,
    mut is_initialized: Local<bool>,
    // If you need to access the ids from multiple systems, you can also initialize the `Images`
    // resource while building the app and use `Res<Images>` instead.
    images: Local<Images>,
    mut contexts: EguiContexts,
) {
    let egui_texture_handle = ui_state
        .egui_texture_handle
        .get_or_insert_with(|| {
            contexts.ctx_mut().load_texture(
                "example-image",
                egui::ColorImage::example(),
                Default::default(),
            )
        })
        .clone();

    let mut load = false;
    let mut remove = false;
    let mut invert = false;

    if !*is_initialized {
        *is_initialized = true;
        *rendered_texture_id = contexts.add_image(images.bevy_icon.clone_weak());
    }

    let ctx = contexts.ctx_mut();

    egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
        // The top panel is often a good place for a menu bar:
        egui::menu::bar(ui, |ui| {
            egui::menu::menu_button(ui, "File", |ui| {
                if ui.button("Quit").clicked() {
                    std::process::exit(0);
                }
            });
            egui::menu::menu_button(ui, "Edit", |ui| {
                if ui.button("Quit").clicked() {
                    std::process::exit(0);
                }
            });
            egui::menu::menu_button(ui, "Window", |ui| {
                if ui.button("Quit").clicked() {
                    std::process::exit(0);
                }
            });
            egui::menu::menu_button(ui, "Tools", |ui| {
                if ui.button("Quit").clicked() {
                    std::process::exit(0);
                }
            });
            egui::menu::menu_button(ui, "Build", |ui| {
                if ui.button("Quit").clicked() {
                    std::process::exit(0);
                }
            });
            egui::menu::menu_button(ui, "Select", |ui| {
                if ui.button("Quit").clicked() {
                    std::process::exit(0);
                }
            });
            egui::menu::menu_button(ui, "Entity", |ui| {
                if ui.button("Quit").clicked() {
                    std::process::exit(0);
                }
            });
            egui::menu::menu_button(ui, "Help", |ui| {
                if ui.button("Quit").clicked() {
                    std::process::exit(0);
                }
            });
        });
    });

    egui::SidePanel::right("side_panel_right")
        .resizable(true)
        .default_width(200.0)
        .show(ctx, |ui| {
            ui.heading("Side Panel Right");
            ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover());
        });
        
    egui::TopBottomPanel::bottom("project")
        .resizable(true)
        .default_height(500.0)
        .show(ctx, |ui| {
            ui.heading("Bottom Panel");
            ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover());
        });

    egui::SidePanel::left("side_panel")
        .resizable(true)
        .default_width(200.0)
        .show(ctx, |ui| {
            ui.heading("Side Panel Left");
            ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover());
        });

    if invert {
        ui_state.inverted = !ui_state.inverted;
    }
    if load || invert {
        // If an image is already added to the context, it'll return an existing texture id.
        if ui_state.inverted {
            *rendered_texture_id = contexts.add_image(images.bevy_icon_inverted.clone_weak());
        } else {
            *rendered_texture_id = contexts.add_image(images.bevy_icon.clone_weak());
        };
    }
    if remove {
        contexts.remove_image(&images.bevy_icon);
        contexts.remove_image(&images.bevy_icon_inverted);
    }
}