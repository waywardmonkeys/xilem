// Copyright 2026 the Xilem Authors
// SPDX-License-Identifier: Apache-2.0

use image::RgbaImage;
use masonry_core::app::PaintResult;
use masonry_core::dpi::PhysicalSize;
use masonry_core::kurbo::{Affine, Rect};
use masonry_core::peniko::Color;

#[cfg(feature = "imaging_vello")]
use std::num::NonZeroUsize;

#[cfg(all(feature = "imaging_vello", feature = "imaging_vello_hybrid"))]
use imaging_vello_hybrid as _;
#[cfg(all(feature = "imaging_vello", feature = "imaging_vello_hybrid"))]
use vello_hybrid as _;

#[cfg(feature = "imaging_vello")]
use imaging_vello::VelloSceneSink;
#[cfg(feature = "imaging_vello")]
use masonry_core::peniko::Fill;
#[cfg(feature = "imaging_vello")]
use masonry_core::vello::util::{RenderContext, block_on_wgpu};
#[cfg(feature = "imaging_vello")]
use masonry_core::vello::wgpu::{
    BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, MapMode,
    TexelCopyBufferInfo, TexelCopyBufferLayout, TextureDescriptor, TextureDimension, TextureFormat,
    TextureUsages, TextureViewDescriptor,
};
#[cfg(feature = "imaging_vello")]
use masonry_core::vello::{self, Scene};

#[cfg(all(feature = "imaging_vello_hybrid", not(feature = "imaging_vello")))]
use imaging_vello_hybrid::VelloHybridRenderer;
#[cfg(all(feature = "imaging_vello_hybrid", not(feature = "imaging_vello")))]
use masonry_core::imaging::Painter;
#[cfg(all(feature = "imaging_vello_hybrid", not(feature = "imaging_vello")))]
use masonry_core::imaging::record::{Scene, replay_transformed};

#[cfg(feature = "imaging_vello")]
#[derive(Default)]
pub(crate) struct RenderState {
    render_context: Option<RenderContext>,
    renderer: Option<vello::Renderer>,
}

#[cfg(all(feature = "imaging_vello_hybrid", not(feature = "imaging_vello")))]
#[derive(Default)]
pub(crate) struct RenderState {
    renderer: Option<VelloHybridRenderer>,
    renderer_size: Option<(u16, u16)>,
}

#[cfg(feature = "imaging_vello")]
pub(crate) fn render(
    state: &mut RenderState,
    paint_result: &PaintResult,
    window_size: PhysicalSize<u32>,
    root_padding: u32,
    background_color: Color,
) -> RgbaImage {
    let mut context = state
        .render_context
        .take()
        .unwrap_or_else(RenderContext::new);

    let device_id = pollster::block_on(context.device(None)).expect("No compatible device found");
    let device_handle = &mut context.devices[device_id];
    let device = &device_handle.device;
    let queue = &device_handle.queue;

    let mut contents_scene = Scene::new();
    let bounds = Rect::new(
        0.0,
        0.0,
        f64::from(window_size.width),
        f64::from(window_size.height),
    );
    let mut sink = VelloSceneSink::new(&mut contents_scene, bounds);
    paint_result.replay_into(&mut sink);
    sink.finish()
        .expect("translate retained imaging scene for Vello");

    let mut renderer = state.renderer.take().unwrap_or_else(|| {
        vello::Renderer::new(
            device,
            vello::RendererOptions {
                // TODO - Examine this value
                use_cpu: true,
                num_init_threads: NonZeroUsize::new(1),
                // TODO - Examine this value
                antialiasing_support: vello::AaSupport::area_only(),
                ..Default::default()
            },
        )
        .expect("Got non-Send/Sync error from creating renderer")
    });

    let (width, height) = padded_dimensions(window_size, root_padding);
    let render_params = vello::RenderParams {
        base_color: background_color,
        width,
        height,
        antialiasing_method: vello::AaConfig::Area,
    };

    let size = Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let target = device.create_texture(&TextureDescriptor {
        label: Some("Target texture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::STORAGE_BINDING | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&TextureViewDescriptor::default());

    let scene = if root_padding != 0 {
        let mut scene = Scene::new();
        // 25% opacity of 50% grey provides a border of where the actual widget content is.
        let padding_color = Color::from_rgba8(127, 127, 127, 64);
        for [x0, y0, x1, y1] in padding_rects(width, height, root_padding) {
            scene.fill(
                Fill::EvenOdd,
                Affine::IDENTITY,
                padding_color,
                None,
                &Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64),
            );
        }
        scene.append(
            &contents_scene,
            Some(Affine::translate((
                root_padding as f64,
                root_padding as f64,
            ))),
        );
        scene
    } else {
        contents_scene
    };

    renderer
        .render_to_texture(device, queue, &scene, &view, &render_params)
        .expect("Got non-Send/Sync error from rendering");
    let padded_byte_width = (width * 4).next_multiple_of(256);
    let buffer_size = padded_byte_width as u64 * height as u64;
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("val"),
        size: buffer_size,
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("Copy out buffer"),
    });
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        TexelCopyBufferInfo {
            buffer: &buffer,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_byte_width),
                rows_per_image: None,
            },
        },
        size,
    );

    queue.submit([encoder.finish()]);
    let buf_slice = buffer.slice(..);

    let (sender, receiver) = futures_intrusive::channel::shared::oneshot_channel();
    buf_slice.map_async(MapMode::Read, move |v| sender.send(v).unwrap());
    let recv_result = block_on_wgpu(device, receiver.receive()).expect("channel was closed");
    recv_result.expect("failed to map buffer");

    let data = buf_slice.get_mapped_range();
    let mut result = Vec::<u8>::with_capacity((width * height * 4).try_into().unwrap());
    for row in 0..height {
        let start = (row * padded_byte_width).try_into().unwrap();
        result.extend(&data[start..start + (width * 4) as usize]);
    }

    state.render_context = Some(context);
    state.renderer = Some(renderer);

    RgbaImage::from_vec(width, height, result).expect("failed to create image")
}

#[cfg(all(feature = "imaging_vello_hybrid", not(feature = "imaging_vello")))]
pub(crate) fn render(
    state: &mut RenderState,
    paint_result: &PaintResult,
    window_size: PhysicalSize<u32>,
    root_padding: u32,
    background_color: Color,
) -> RgbaImage {
    let (width, height) = padded_dimensions(window_size, root_padding);
    let width_u16 = u16::try_from(width).expect("screenshot width exceeds hybrid backend limit");
    let height_u16 = u16::try_from(height).expect("screenshot height exceeds hybrid backend limit");

    if state.renderer_size != Some((width_u16, height_u16)) {
        state.renderer = Some(VelloHybridRenderer::new(width_u16, height_u16));
        state.renderer_size = Some((width_u16, height_u16));
    }

    let renderer = state.renderer.as_mut().unwrap();
    let mut scene = Scene::new();
    {
        let mut painter = Painter::new(&mut scene);
        painter.fill_rect(
            Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
            background_color,
        );

        if root_padding != 0 {
            let padding_color = Color::from_rgba8(127, 127, 127, 64);
            for [x0, y0, x1, y1] in padding_rects(width, height, root_padding) {
                painter.fill_rect(
                    Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64),
                    padding_color,
                );
            }
        }
    }

    let padding_transform = Affine::translate((root_padding as f64, root_padding as f64));
    replay_transformed(&paint_result.base, &mut scene, padding_transform);
    for layer in &paint_result.overlays {
        replay_transformed(
            &layer.scene,
            &mut scene,
            padding_transform * layer.transform,
        );
    }

    let rgba = renderer
        .render_scene_rgba8(&scene)
        .expect("render retained imaging scene for Vello Hybrid");
    RgbaImage::from_vec(width, height, rgba).expect("failed to create image")
}

fn padded_dimensions(window_size: PhysicalSize<u32>, root_padding: u32) -> (u32, u32) {
    let width = window_size.width.max(1) + root_padding * 2;
    let height = window_size.height.max(1) + root_padding * 2;
    (width, height)
}

fn padding_rects(width: u32, height: u32, padding: u32) -> [[u32; 4]; 4] {
    [
        [0, 0, padding, height],
        [width - padding, 0, width, height],
        [padding, 0, width - padding, padding],
        [padding, height - padding, width - padding, height],
    ]
}
