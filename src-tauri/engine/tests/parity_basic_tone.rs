//! `rr_basic_tone` against the app's tone sliders.
//!
//! The app's side is its own shaders, run on the GPU: `apply_tonal_adjustments`
//! and `apply_highlights_adjustment` in `shaders/shader.wgsl`, and the guided
//! filter's passes in `shaders/guided.wgsl`, in the order and at the sizes
//! `build_guided_coeffs` (`gpu_processing.rs`) runs them, restated below.
//!
//! - The sliders, given the same pixel and the same detail: every channel
//!   within 2 parts in 10^5 of the app's, or 10^-6 absolute near zero. Both
//!   chain several `pow`s; the GPU's differ from Rust's in the last bits.
//! - The guided filter's coefficients: within 5 x 10^-4 of the app's. Both
//!   sides take the variance as E[l^2] - E[l]^2 in f32, which cancels where
//!   the light is even, and sum in different orders.
//! - What the tone sliders read from them, the detail at each pixel, as the
//!   shader's `main` reads it with `sample_gf_tex`: within 10^-4 stops.
//!   Measured, 6 x 10^-6: the two coefficients' differences cancel along the
//!   fitted line.

mod common;

use std::sync::Arc;

use argentum_engine::Gpu;
use argentum_engine::Rgb;
use argentum_engine::tools::rr_basic_tone::{self, Amounts, Coefficients, log_luma};
use common::app_shader::{AppShader, Job};
use wgpu::util::DeviceExt;

const SLIDERS: [(f32, f32, f32, f32, f32); 9] = [
    // contrast, highlights, shadows, whites, blacks, as the app's sliders
    (40.0, 0.0, 0.0, 0.0, 0.0),
    (-60.0, 0.0, 0.0, 0.0, 0.0),
    (0.0, -80.0, 0.0, 0.0, 0.0),
    (0.0, 70.0, 0.0, 0.0, 0.0),
    (0.0, 0.0, 90.0, 0.0, 0.0),
    (0.0, 0.0, -50.0, 0.0, 0.0),
    (0.0, 0.0, 0.0, 60.0, -40.0),
    (0.0, 0.0, 0.0, -70.0, 55.0),
    (25.0, -45.0, 35.0, 20.0, -15.0),
];

fn amounts((contrast, highlights, shadows, whites, blacks): (f32, f32, f32, f32, f32)) -> Amounts {
    Amounts::from(&rr_basic_tone::Settings {
        contrast,
        highlights,
        shadows,
        whites,
        blacks,
    })
}

#[test]
fn the_sliders_are_the_apps_shader() {
    let Some(gpu) = common::gpu("the_sliders_are_the_apps_shader") else {
        return;
    };
    let shader = AppShader::new(
        gpu,
        r#"
        let s = ag_test_params[0];
        let d = ag_test_params[2u + i];
        let toned = apply_tonal_adjustments(v.xyz, d.x, d.y, 1u, s.x, s.y, s.z, s.w);
        return vec4<f32>(apply_highlights_adjustment(toned, d.x, d.y, 1u, ag_test_params[1].x), 0.0);
        "#,
    );
    let rgb = common::colours();
    let inputs: Vec<[f32; 4]> = rgb.pixels.iter().map(|p| [p[0], p[1], p[2], 0.0]).collect();
    // Detail from flat to a stop and a half either way, local and mid-scale.
    let details: Vec<[f32; 2]> = (0..rgb.pixels.len())
        .map(|i| {
            let t = (i * 7919 % 1000) as f32 / 1000.0;
            let u = (i * 104_729 % 997) as f32 / 997.0;
            if i % 5 == 0 {
                [0.0, 0.0]
            } else {
                [3.0 * t - 1.5, 2.0 * u - 1.0]
            }
        })
        .collect();
    let mut worst = 0.0f32;
    for sliders in SLIDERS {
        let a = amounts(sliders);
        let mut params = vec![
            [a.contrast, a.shadows, a.whites, a.blacks],
            [a.highlights, 0.0, 0.0, 0.0],
        ];
        params.extend(details.iter().map(|d| [d[0], d[1], 0.0, 0.0]));
        let theirs = shader.run(&Job {
            inputs: &inputs,
            params: &params,
            ..Default::default()
        });
        for ((p, d), b) in rgb.pixels.iter().zip(&details).zip(&theirs) {
            let ours = rr_basic_tone::basic_tone(*p, d[0], d[1], &a);
            for c in 0..3 {
                let miss = (ours[c] - b[c]).abs();
                let allowed = b[c].abs() * 2e-5 + 1e-6;
                worst = worst.max(miss / allowed);
                assert!(
                    miss <= allowed,
                    "{sliders:?}, {p:?}, detail {d:?}: ours {ours:?}, the app's {b:?}"
                );
            }
        }
    }
    eprintln!("\n  worst miss: {worst:.2} of the tolerance");
}

/// A picture with light, shade, edges and texture, its values ones a half
/// float holds exactly, as the app's input texture does.
fn scene(w: usize, h: usize) -> Rgb {
    let pixels = (0..w * h)
        .map(|i| {
            let (x, y) = ((i % w) as f32 / w as f32, (i / w) as f32 / h as f32);
            let mut v = 0.02 + 0.5 * x * x + 0.1 * y;
            if ((x * 9.0) as i32 + (y * 7.0) as i32) % 2 == 0 {
                v *= 2.5;
            }
            v *= 1.0 + 0.15 * ((i * 7919 % 1013) as f32 / 1013.0 - 0.5);
            let c = [v * 1.1, v, v * 0.8 + 0.05 * y];
            c.map(|v| half::f16::from_f32(v).to_f32())
        })
        .collect();
    Rgb::new(w, h, pixels)
}

#[test]
fn the_guided_filter_is_the_apps_shader() {
    let Some(gpu) = common::gpu("the_guided_filter_is_the_apps_shader") else {
        return;
    };
    eprintln!();
    // Below 1080 on the short side, and above it, where the filter works on a
    // smaller copy.
    for (w, h) in [(640, 400), (1500, 1200)] {
        let rgb = scene(w, h);
        let ours = Coefficients::of(&rgb);
        let theirs = AppGuided::new(gpu.clone()).run(&rgb);
        for (name, a, b) in [
            ("tonal", &ours.tonal, &theirs.0),
            ("coarse", &ours.coarse, &theirs.1),
        ] {
            assert_eq!((a.width, a.height), (b.0, b.1), "{name} size");
            let worst = |k: usize| {
                a.texels
                    .iter()
                    .zip(&b.2)
                    .map(|(x, y)| (x[k] - y[k]).abs())
                    .fold(0.0f32, f32::max)
            };
            eprintln!(
                "  {w}x{h} {name} {}x{}: worst slope {:.2e}, offset {:.2e}",
                a.width,
                a.height,
                worst(0),
                worst(1)
            );
            assert!(worst(0) <= 5e-4 && worst(1) <= 5e-4, "{w}x{h} {name}");
        }

        // Read back at full size as the shader's main does.
        let shader = AppShader::new(
            gpu.clone(),
            "return sample_gf_tex(ag_test_tex, vec2<u32>(u32(v.x), u32(v.y)));",
        );
        let probes: Vec<[f32; 4]> = (0..997)
            .map(|k| [((k * 389) % w) as f32, ((k * 211) % h) as f32, 0.0, 0.0])
            .collect();
        let sampled = |plane: &(usize, usize, Vec<[f32; 2]>)| {
            let texels: Vec<[f32; 4]> = plane.2.iter().map(|t| [t[0], t[1], 0.0, 0.0]).collect();
            shader.run(&Job {
                inputs: &probes,
                input_size: (w as u32, h as u32),
                texture: Some((plane.0 as u32, plane.1 as u32, &texels)),
                ..Default::default()
            })
        };
        let (tonal, coarse) = (sampled(&theirs.0), sampled(&theirs.1));
        let mut worst_detail = 0.0f32;
        for (k, probe) in probes.iter().enumerate() {
            let (x, y) = (probe[0] as usize, probe[1] as usize);
            let gf_i = log_luma(rgb.pixels[y * w + x]);
            // `tonal_log_detail` and `tonal_mid_detail` in their main().
            let smooth = tonal[k][0] * gf_i + tonal[k][1];
            let app = (gf_i - smooth, smooth - (coarse[k][0] * gf_i + coarse[k][1]));
            let ours = ours.detail(x, y, gf_i);
            worst_detail = worst_detail
                .max((ours.0 - app.0).abs())
                .max((ours.1 - app.1).abs());
        }
        eprintln!("  {w}x{h} detail: worst {worst_detail:.2e} stops");
        assert!(worst_detail <= 1e-4, "{w}x{h}: detail {worst_detail}");
    }
}

/// The app's guided filter passes, `guided.wgsl` as it is, run in the order
/// and at the sizes `build_guided_coeffs` in `gpu_processing.rs` @ c786c146
/// runs them: the luminance half's tonal and coarse coefficients.
struct AppGuided {
    gpu: Arc<Gpu>,
    module: wgpu::ShaderModule,
    layout: wgpu::BindGroupLayout,
}

type Plane = (usize, usize, Vec<[f32; 2]>);

impl AppGuided {
    fn new(gpu: Arc<Gpu>) -> Self {
        let device = &gpu.device;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("the app's guided.wgsl"),
            source: wgpu::ShaderSource::Wgsl(common::app_shader::read("guided.wgsl").into()),
        });
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                texture(0),
                texture(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        AppGuided {
            gpu,
            module,
            layout,
        }
    }

    fn texture(&self, w: usize, h: usize, texels: Option<&[[f32; 4]]>) -> wgpu::Texture {
        let desc = wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: w as u32,
                height: h as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        };
        let device = &self.gpu.device;
        match texels {
            Some(t) => device.create_texture_with_data(
                &self.gpu.queue,
                &desc,
                wgpu::util::TextureDataOrder::LayerMajor,
                bytemuck::cast_slice(t),
            ),
            None => device.create_texture(&desc),
        }
    }

    /// The tonal and the coarse coefficients, luminance half.
    fn run(&self, rgb: &Rgb) -> (Plane, Plane) {
        let device = &self.gpu.device;
        let (width, height) = (rgb.width as u32, rgb.height as u32);
        // build_guided_coeffs, its sizes and radii.
        let short = width.min(height).max(1);
        let k = (1080.0 / short as f32).min(1.0);
        let lw = ((width as f32 * k).round() as u32).max(1);
        let lh = ((height as f32 * k).round() as u32).max(1);
        let low_short = lw.min(lh) as f32;
        let cw = lw.div_ceil(4).max(1);
        let ch = lh.div_ceil(4).max(1);
        let params = |base_radius: f32, eps: f32, div: f32| {
            let radius = (base_radius * low_short / 1080.0 / div).ceil().max(1.0) as u32;
            let mut bytes = Vec::new();
            bytes.extend(radius.to_le_bytes());
            bytes.extend(1u32.to_le_bytes());
            bytes.extend(eps.to_le_bytes());
            bytes.extend(0f32.to_le_bytes());
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: &bytes,
                usage: wgpu::BufferUsages::UNIFORM,
            })
        };
        let input: Vec<[f32; 4]> = rgb.pixels.iter().map(|p| [p[0], p[1], p[2], 1.0]).collect();
        let input = self.texture(rgb.width, rgb.height, Some(&input));
        let fine = |_| self.texture(lw as usize, lh as usize, None);
        let coarse = |_| self.texture(cw as usize, ch as usize, None);
        let (moments, tmp, blurred, ab) = (fine(0), fine(1), fine(2), fine(3));
        let (c_moments, c_tmp, c_blurred, c_ab) = (coarse(0), coarse(1), coarse(2), coarse(3));
        let params_tonal = params(10.0, 0.25, 1.0);
        let params_coarse = params(40.0, 0.5, 4.0);

        let mut encoder = device.create_command_encoder(&Default::default());
        let mut pass = |entry: &str,
                        src: &wgpu::Texture,
                        dst: &wgpu::Texture,
                        params: &wgpu::Buffer,
                        (w, h): (u32, u32)| {
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&self.layout)],
                immediate_size: 0,
            });
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &self.module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            });
            let src = src.create_view(&Default::default());
            let dst = dst.create_view(&Default::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&dst),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: params.as_entire_binding(),
                    },
                ],
            });
            let mut cpass = encoder.begin_compute_pass(&Default::default());
            cpass.set_pipeline(&pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
        };
        let f = (lw, lh);
        let c = (cw, ch);
        // The order of build_guided_coeffs, less the clarity scale.
        pass("gf_downsample", &input, &moments, &params_tonal, f);
        pass("gf_box_down", &moments, &c_moments, &params_coarse, c);
        pass("gf_blur_h", &c_moments, &c_tmp, &params_coarse, c);
        pass("gf_blur_v", &c_tmp, &c_blurred, &params_coarse, c);
        pass("gf_coeffs", &c_blurred, &c_ab, &params_coarse, c);
        pass("gf_blur_h", &c_ab, &c_tmp, &params_coarse, c);
        pass("gf_blur_v", &c_tmp, &c_moments, &params_coarse, c);
        pass("gf_blur_h", &moments, &tmp, &params_tonal, f);
        pass("gf_blur_v", &tmp, &blurred, &params_tonal, f);
        pass("gf_coeffs", &blurred, &ab, &params_tonal, f);
        pass("gf_blur_h", &ab, &tmp, &params_tonal, f);
        pass("gf_blur_v", &tmp, &blurred, &params_tonal, f);
        self.gpu.queue.submit(Some(encoder.finish()));
        (self.read(&blurred, f), self.read(&c_moments, c))
    }

    /// The `xy` of each texel: the luminance line's slope and offset.
    fn read(&self, texture: &wgpu::Texture, (w, h): (u32, u32)) -> Plane {
        let device = &self.gpu.device;
        let row = (w * 16).div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.gpu.queue.submit(Some(encoder.finish()));
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |r| r.expect("mapped"));
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(120)),
            })
            .expect("the GPU finished");
        let bytes = slice.get_mapped_range();
        let mut texels = Vec::with_capacity((w * h) as usize);
        for y in 0..h as usize {
            let line: &[[f32; 4]] =
                bytemuck::cast_slice(&bytes[y * row as usize..y * row as usize + w as usize * 16]);
            texels.extend(line.iter().map(|t| [t[0], t[1]]));
        }
        (w as usize, h as usize, texels)
    }
}
