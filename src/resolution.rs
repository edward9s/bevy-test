//! Native UI with either fixed native or 65-100% adaptive scene resolution.
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
    pub fixed: bool,
    level: u32,
    wait: f32,
    elapsed: f32,
    frames: u32,
}

impl Resolution {
    fn new(output: UVec2, image: Handle<Image>, fixed: bool) -> Self {
        assert!(output.min_element() > 0, "Empty display resolution");
        Self {
            image,
            size: output,
            output,
            ready: false,
            fixed,
            level: 20,
            wait: 2.,
            elapsed: 0.,
            frames: 0,
        }
    }

    fn sample(&mut self, delta: f32) -> bool {
        assert!(delta.is_finite() && delta >= 0., "Invalid frame interval");
        if self.fixed || !self.ready {
            self.wait = 2.;
            self.elapsed = 0.;
            self.frames = 0;
            return false;
        }
        self.wait -= delta;
        if self.wait > 0. {
            return false;
        }
        self.elapsed += delta;
        self.frames += 1;
        if self.elapsed < 0.5 || self.frames < 8 {
            return false;
        }
        // Whole-frame time includes CPU and presentation waits. This is not a
        // GPU timer; automatic mode intentionally requires no GPU profiling.
        let frame_ms = self.elapsed / self.frames as f32 * 1000.;
        self.elapsed = 0.;
        self.frames = 0;
        let next = if frame_ms > 18. {
            self.level.saturating_sub(1).max(13)
        } else if frame_ms < 12.5 {
            (self.level + 1).min(20)
        } else {
            self.level
        };
        if next == self.level {
            return false;
        }
        self.level = next;
        self.size = (self.output.as_vec2() * (next as f32 / 20.))
            .round()
            .as_uvec2()
            .max(UVec2::ONE);
        self.wait = 1.;
        true
    }
}

fn fixed_resolution() -> bool {
    match option_env!("BEVY_FIXED_RESOLUTION") {
        None | Some("1") => true,
        Some("0") => false,
        Some(_) => panic!("BEVY_FIXED_RESOLUTION must be 0 or 1"),
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
    // Reference captures always use the native baseline in either build mode.
    let mut resolution = Resolution::new(
        output,
        Handle::default(),
        capture.path.is_some() || fixed_resolution(),
    );
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
    time: Res<Time<Real>>,
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
        let fixed = resolution.fixed;
        *resolution = Resolution::new(output, image, fixed);
        resolution.ready = ready;
    }
    let scaled = resolution.sample(time.delta_secs());
    if resized || scaled {
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
            "Scene resolution: {}x{} for {}x{} display ({}, {}%)",
            size.x,
            size.y,
            output.x,
            output.y,
            if resolution.fixed {
                "Native"
            } else {
                "Auto whole-frame budget"
            },
            resolution.level * 5,
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
            let mut resolution = Resolution::new(UVec2::new(1280, 720), Handle::default(), true);
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
            let resolution = Resolution::new(output, Handle::default(), true);
            assert_eq!(resolution.size, output);
        }
    }

    #[test]
    fn fixed_mode_keeps_native_detail_under_load() {
        let output = UVec2::new(2560, 1600);
        let mut resolution = Resolution::new(output, Handle::default(), true);
        resolution.ready = true;
        for _ in 0..1000 {
            assert!(!resolution.sample(0.1));
        }
        assert_eq!(resolution.size, output);
    }

    #[test]
    fn adaptive_mode_respects_bounds_hysteresis_and_recovery() {
        for output in [
            UVec2::new(2560, 1600),
            UVec2::new(1600, 2560),
            UVec2::new(800, 480),
        ] {
            let mut resolution = Resolution::new(output, Handle::default(), false);
            // Loading and one-time reflection capture must not lower quality.
            for _ in 0..1000 {
                assert!(!resolution.sample(0.1));
            }
            assert_eq!(resolution.size, output);
            resolution.ready = true;
            for _ in 0..10 {
                assert!(!resolution.sample(0.1));
            }
            for _ in 0..1000 {
                resolution.sample(0.04);
                assert!((13..=20).contains(&resolution.level));
            }
            assert_eq!(resolution.level, 13);
            assert_eq!(
                resolution.size,
                (output.as_vec2() * 0.65).round().as_uvec2()
            );
            for _ in 0..1000 {
                assert!(!resolution.sample(0.016));
            }
            for _ in 0..3000 {
                resolution.sample(0.008);
            }
            assert_eq!(resolution.level, 20);
            assert_eq!(resolution.size, output);
        }
    }
}
