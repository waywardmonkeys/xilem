// Copyright 2026 the Xilem Authors
// SPDX-License-Identifier: Apache-2.0

#[cfg(feature = "imaging_vello")]
use std::collections::HashMap;

use masonry_core::app::PaintResult;
use masonry_core::dpi::PhysicalSize;
use masonry_core::imaging::PaintSink;
#[cfg(all(feature = "imaging_vello_hybrid", not(feature = "imaging_vello")))]
use masonry_core::imaging::Painter;
use masonry_core::imaging::record::{Scene as ImagingScene, replay_transformed};
use masonry_core::kurbo::{Affine, Rect};
use masonry_core::peniko::Color;
use masonry_core::vello::wgpu;

#[cfg(all(feature = "imaging_vello", feature = "imaging_vello_hybrid"))]
use imaging_vello_hybrid as _;

#[cfg(feature = "imaging_vello")]
use imaging_vello::VelloSceneSink;
#[cfg(feature = "imaging_vello")]
use masonry_core::vello::{AaConfig, AaSupport, RenderParams, RendererOptions};

#[cfg(all(feature = "imaging_vello_hybrid", not(feature = "imaging_vello")))]
use imaging_vello_hybrid::VelloHybridRenderer;

#[cfg(feature = "imaging_vello")]
pub(crate) type Renderer = masonry_core::vello::Renderer;
#[cfg(feature = "imaging_vello")]
pub(crate) type Scene = masonry_core::vello::Scene;

#[cfg(all(feature = "imaging_vello_hybrid", not(feature = "imaging_vello")))]
pub(crate) struct Renderer {
    inner: VelloHybridRenderer,
    width: u16,
    height: u16,
}
#[cfg(all(feature = "imaging_vello_hybrid", not(feature = "imaging_vello")))]
pub(crate) type Scene = ImagingScene;

#[cfg(feature = "imaging_vello")]
#[derive(Debug)]
pub(crate) struct ImageOverrideState {
    pub(crate) image: masonry_core::peniko::ImageData,
    pub(crate) texture: wgpu::Texture,
    pub(crate) applied: bool,
    pub(crate) prev: Option<wgpu::TexelCopyTextureInfoBase<wgpu::Texture>>,
}

fn replay_scaled_paint_result<S>(paint_result: &PaintResult, sink: &mut S, scale_factor: f64)
where
    S: PaintSink + ?Sized,
{
    let scene_transform = Affine::scale(scale_factor);
    replay_transformed(&paint_result.base, sink, scene_transform);
    for layer in &paint_result.overlays {
        replay_transformed(&layer.scene, sink, scene_transform * layer.transform);
    }
}

pub(crate) fn build_scene(
    paint_result: &PaintResult,
    size: PhysicalSize<u32>,
    scale_factor: f64,
    base_color: Color,
) -> Result<Scene, &'static str> {
    #[cfg(feature = "imaging_vello")]
    {
        let mut scene = Scene::new();
        let bounds = Rect::new(0.0, 0.0, f64::from(size.width), f64::from(size.height));
        let mut sink = VelloSceneSink::new(&mut scene, bounds);
        replay_scaled_paint_result(paint_result, &mut sink, scale_factor);
        sink.finish()
            .map_err(|_| "couldn't translate retained imaging scene for Vello")?;
        Ok(scene)
    }

    #[cfg(all(feature = "imaging_vello_hybrid", not(feature = "imaging_vello")))]
    {
        let mut scene = Scene::new();
        Painter::new(&mut scene).fill_rect(
            Rect::new(0.0, 0.0, f64::from(size.width), f64::from(size.height)),
            base_color,
        );
        replay_scaled_paint_result(paint_result, &mut scene, scale_factor);
        Ok(scene)
    }
}

pub(crate) fn backend_label() -> &'static str {
    #[cfg(feature = "imaging_vello")]
    {
        "Rendering using Vello"
    }

    #[cfg(all(feature = "imaging_vello_hybrid", not(feature = "imaging_vello")))]
    {
        "Rendering using Vello Hybrid"
    }
}

pub(crate) fn backend_name() -> &'static str {
    #[cfg(feature = "imaging_vello")]
    {
        "imaging_vello"
    }

    #[cfg(all(feature = "imaging_vello_hybrid", not(feature = "imaging_vello")))]
    {
        "imaging_vello_hybrid"
    }
}

#[cfg(feature = "imaging_vello")]
pub(crate) fn render_to_texture(
    renderer: &mut Option<Renderer>,
    image_overrides: &mut HashMap<u64, ImageOverrideState>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    _surface_format: wgpu::TextureFormat,
    target_view: &wgpu::TextureView,
    scene: &Scene,
    base_color: Color,
    size: PhysicalSize<u32>,
) {
    let renderer_options = RendererOptions {
        antialiasing_support: AaSupport::area_only(),
        ..Default::default()
    };
    let render_params = RenderParams {
        base_color,
        width: size.width,
        height: size.height,
        antialiasing_method: AaConfig::Area,
    };

    let renderer = renderer.get_or_insert_with(|| {
        #[cfg_attr(not(feature = "tracy"), expect(unused_mut, reason = "cfg"))]
        let mut renderer = Renderer::new(device, renderer_options).unwrap();
        #[cfg(feature = "tracy")]
        {
            let new_profiler = wgpu_profiler::GpuProfiler::new_with_tracy_client(
                wgpu_profiler::GpuProfilerSettings::default(),
                wgpu::Backend::Vulkan,
                device,
                queue,
            )
            .unwrap_or(renderer.profiler);
            renderer.profiler = new_profiler;
        }
        renderer
    });

    for override_state in image_overrides.values_mut() {
        if override_state.applied {
            continue;
        }
        override_state.prev = renderer.override_image(
            &override_state.image,
            Some(wgpu::TexelCopyTextureInfoBase {
                texture: override_state.texture.clone(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            }),
        );
        override_state.applied = true;
    }

    renderer
        .render_to_texture(device, queue, scene, target_view, &render_params)
        .expect("failed to render to surface");
}

#[cfg(all(feature = "imaging_vello_hybrid", not(feature = "imaging_vello")))]
pub(crate) fn render_to_texture(
    renderer: &mut Option<Renderer>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    _surface_format: wgpu::TextureFormat,
    target_texture: &wgpu::Texture,
    target_view: &wgpu::TextureView,
    scene: &Scene,
    _base_color: Color,
    size: PhysicalSize<u32>,
) {
    let width = match u16::try_from(size.width) {
        Ok(width) => width,
        Err(_) => {
            tracing::error!("window width exceeds hybrid backend limit");
            return;
        }
    };
    let height = match u16::try_from(size.height) {
        Ok(height) => height,
        Err(_) => {
            tracing::error!("window height exceeds hybrid backend limit");
            return;
        }
    };

    let renderer = renderer.get_or_insert_with(|| Renderer {
        inner: VelloHybridRenderer::new(width, height),
        width,
        height,
    });
    if renderer.width != width || renderer.height != height {
        *renderer = Renderer {
            inner: VelloHybridRenderer::new(width, height),
            width,
            height,
        };
    }

    let rgba = match renderer.inner.render_scene_rgba8(scene) {
        Ok(rgba) => rgba,
        Err(err) => {
            tracing::error!(
                ?err,
                "couldn't translate retained imaging scene for Vello Hybrid"
            );
            return;
        }
    };

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: target_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size.width * 4),
            rows_per_image: Some(size.height),
        },
        wgpu::Extent3d {
            width: size.width,
            height: size.height,
            depth_or_array_layers: 1,
        },
    );

    let _ = (device, target_view);
}

#[cfg(feature = "imaging_vello")]
pub(crate) fn set_image_override(
    renderer: &mut Option<Renderer>,
    image_overrides: &mut HashMap<u64, ImageOverrideState>,
    image: masonry_core::peniko::ImageData,
    texture: wgpu::Texture,
) {
    let image_id = image.data.id();

    if let Some(existing) = image_overrides.get_mut(&image_id) {
        existing.texture = texture;
        if existing.applied {
            if let Some(renderer) = renderer {
                renderer.override_image(
                    &existing.image,
                    Some(wgpu::TexelCopyTextureInfoBase {
                        texture: existing.texture.clone(),
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    }),
                );
            } else {
                existing.applied = false;
            }
        }
        return;
    }

    let mut state = ImageOverrideState {
        image,
        texture,
        applied: false,
        prev: None,
    };

    if let Some(renderer) = renderer {
        state.prev = renderer.override_image(
            &state.image,
            Some(wgpu::TexelCopyTextureInfoBase {
                texture: state.texture.clone(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            }),
        );
        state.applied = true;
    }

    image_overrides.insert(image_id, state);
}

#[cfg(feature = "imaging_vello")]
pub(crate) fn clear_image_override(
    renderer: &mut Option<Renderer>,
    image_overrides: &mut HashMap<u64, ImageOverrideState>,
    image: &masonry_core::peniko::ImageData,
) {
    let image_id = image.data.id();
    let Some(state) = image_overrides.remove(&image_id) else {
        return;
    };
    if state.applied
        && let Some(renderer) = renderer
    {
        renderer.override_image(&state.image, state.prev);
    }
}
