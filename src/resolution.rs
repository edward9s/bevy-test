//! Keep the scene and UI at the display's native resolution.
use crate::{Capture, OrbitCamera, reflection::Reflection, scene::Forest};
use bevy::{
    camera::RenderTarget,
    image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureFormat, TextureUsages},
    ui::IsDefaultUiCamera,
    window::PrimaryWindow,
};

#[derive(Resource)]
pub struct Resolution {
    pub image: Handle<Image>,
    pub size: UVec2,
    pub output: UVec2,
    pub ready: bool,
}

impl Resolution {
    fn new(output: UVec2, image: Handle<Image>) -> Self {
        assert!(output.min_element() > 0, "Empty display resolution");
        Self {
            image,
            size: output,
            output,
            ready: false,
        }
    }
}

pub fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut capture: ResMut<Capture>,
) {
    // Android may report 0x0 before the native surface is ready. Bootstrap with
    // the requested window size; update() adopts the real size once nonzero.
    let output = UVec2::new(1280, 720);
    let mut resolution = Resolution::new(output, Handle::default());
    let mut image = Image::new_target_texture(
        resolution.size.x,
        resolution.size.y,
        TextureFormat::Rgba8UnormSrgb,
        None,
    );
    image.data = None;
    // Every frame clears and redraws the scene. Keeping the previous GPU pixels
    // would require COPY_SRC and adds an unnecessary copy on each size change.
    image.copy_on_resize = false;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    resolution.image = images.add(image);
    let target = if capture.path.is_some() {
        let mut output_image =
            Image::new_target_texture(output.x, output.y, TextureFormat::Rgba8UnormSrgb, None);
        output_image.data = None;
        output_image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        let handle = images.add(output_image);
        capture.target = Some(handle.clone());
        RenderTarget::Image(handle.into())
    } else {
        RenderTarget::default()
    };
    commands.spawn((
        Name::new("Presentation"),
        Camera2d,
        target,
        Msaa::Off,
        IsDefaultUiCamera,
        // UI is assigned through UiTargetCamera, independently of world layers.
        // Keep this presentation view out of forest visibility/preprocessing.
        bevy::camera::visibility::RenderLayers::none(),
    ));
    commands.spawn((
        Name::new("Scene image"),
        ImageNode {
            image: resolution.image.clone(),
            // Apply Godot's ACES white normalization after the tone curve.
            // The existing composition pass handles this without another pass.
            color: Color::linear_rgb(
                1. / crate::ACES_WHITE,
                1. / crate::ACES_WHITE,
                1. / crate::ACES_WHITE,
            ),
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            top: px(0),
            width: percent(100),
            height: percent(100),
            ..default()
        },
        GlobalZIndex(-1),
    ));
    commands.insert_resource(resolution);
}

pub fn sync_projection(
    resolution: Res<Resolution>,
    mut projection: Single<&mut Projection, With<OrbitCamera>>,
    mut previous: Local<UVec2>,
) {
    if *previous != resolution.size {
        // Bevy 0.20 updates cameras BEFORE publishing Image asset events.
        // Force same-frame target/viewport refresh instead of waiting one frame
        // while the GPU already has the resized texture.
        projection.set_changed();
        *previous = resolution.size;
    }
}

pub fn update(
    mut resolution: ResMut<Resolution>,
    mut images: ResMut<Assets<Image>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    forest: Res<Forest>,
    reflection: Res<Reflection>,
    capture: Res<Capture>,
) {
    resolution.ready = forest.ready && reflection.is_ready();
    let output = if capture.path.is_some() {
        if !capture.check_resize {
            return;
        }
        // Exercise GPU-resident targets across tablet startup, rotation,
        // shrinking and growing, then return to the reference capture size.
        match capture.resize_step {
            0 => UVec2::new(2560, 1600),
            1 => UVec2::new(1600, 2560),
            2 => UVec2::new(800, 480),
            _ => UVec2::new(1280, 720),
        }
    } else {
        let Some(window) = window else {
            return;
        };
        window.resolution.physical_size()
    };
    if output.min_element() == 0 {
        return;
    }
    let resized = output != resolution.output;
    if resized {
        let image = resolution.image.clone();
        let ready = resolution.ready;
        *resolution = Resolution::new(output, image);
        resolution.ready = ready;
    }
    if resized {
        let size = resolution.size;
        images
            .get_mut(&resolution.image)
            .expect("Scene render target missing")
            .resize(Extent3d {
                width: size.x,
                height: size.y,
                depth_or_array_layers: 1,
            });
        info!(
            "Scene resolution: {}x{} for {}x{} display",
            size.x, size.y, output.x, output.y
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        asset::AssetEvent,
        camera::CameraUpdateSystems,
        render::{camera::camera_system, texture::ManualTextureViews},
        window::{WindowCreated, WindowResized, WindowScaleFactorChanged},
    };

    #[test]
    fn startup_accepts_a_surface_that_is_not_ready_yet() {
        let mut app = App::new();
        app.init_resource::<Assets<Image>>()
            .insert_resource(Capture {
                path: None,
                check_resize: false,
                resize_step: 0,
                seconds: 0.,
                captured: false,
                target: None,
            })
            .add_systems(Startup, setup);
        app.world_mut().spawn((
            Window {
                resolution: bevy::window::WindowResolution::new(0, 0),
                ..default()
            },
            PrimaryWindow,
        ));
        app.update();
        let resolution = app.world().resource::<Resolution>();
        assert_eq!(resolution.size, UVec2::new(1280, 720));
        let images = app.world().resource::<Assets<Image>>();
        let image = images.get(&resolution.image).unwrap();
        assert!(image.data.is_none());
        assert!(!image.copy_on_resize);
    }

    #[test]
    fn resized_target_and_camera_agree_before_image_events_arrive() {
        for synchronize in [false, true] {
            let mut images = Assets::<Image>::default();
            let mut resolution = Resolution::new(UVec2::new(1280, 720), Handle::default());
            let original = resolution.size;
            let image = images.add(Image::new_target_texture(
                original.x,
                original.y,
                TextureFormat::Rgba8UnormSrgb,
                None,
            ));
            resolution.image = image.clone();
            let mut app = App::new();
            app.insert_resource(images)
                .insert_resource(resolution)
                .init_resource::<ManualTextureViews>()
                .add_message::<WindowCreated>()
                .add_message::<WindowResized>()
                .add_message::<WindowScaleFactorChanged>()
                .add_message::<AssetEvent<Image>>()
                .add_systems(PostUpdate, camera_system.in_set(CameraUpdateSystems));
            if synchronize {
                app.add_systems(PostUpdate, sync_projection.before(CameraUpdateSystems));
            }
            let camera = app
                .world_mut()
                .spawn((
                    Camera3d::default(),
                    OrbitCamera,
                    RenderTarget::Image(image.clone().into()),
                ))
                .id();
            app.update();
            let changed = UVec2::new(540, 960);
            app.world_mut()
                .resource_mut::<Assets<Image>>()
                .get_mut(&image)
                .unwrap()
                .resize(Extent3d {
                    width: changed.x,
                    height: changed.y,
                    depth_or_array_layers: 1,
                });
            app.world_mut().resource_mut::<Resolution>().size = changed;
            app.update();
            let reported = app
                .world()
                .get::<Camera>(camera)
                .unwrap()
                .physical_target_size();
            // The negative control reproduces the stale viewport. The production
            // synchronizer refreshes it in the same frame, without asset events.
            assert_eq!(reported, Some(if synchronize { changed } else { original }));
        }
    }

    #[test]
    fn retains_native_detail_on_tablets_and_small_displays() {
        for output in [
            UVec2::new(2560, 1600),
            UVec2::new(1600, 2560),
            UVec2::new(800, 480),
        ] {
            let resolution = Resolution::new(output, Handle::default());
            assert_eq!(resolution.size, output);
        }
    }
}
