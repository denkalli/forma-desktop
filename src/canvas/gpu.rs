//! The canvas texture: libforma renders the page with Vello into a texture that egui shows.
//!
//! Both use the GPU device eframe created, so the image never leaves the GPU.

use eframe::egui::TextureId;
use eframe::egui_wgpu::RenderState;
use libforma::geom::Affine;
use libforma::model::AssetStore;
use libforma::paint::Color;
use libforma::render::{Antialiasing, RenderTarget, Renderer, TARGET_FORMAT};
use libforma::scene::DisplayList;

/// Inputs that decide whether the canvas must be drawn again.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FrameKey {
    pub revision: u64,
    pub size: [u32; 2],
    pub transform: [f64; 6],
}

pub struct CanvasGpu {
    renderer: Renderer,
    texture: Option<(wgpu::Texture, wgpu::TextureView, TextureId)>,
    last: Option<FrameKey>,
}

impl CanvasGpu {
    pub fn new(state: &RenderState) -> libforma::Result<Self> {
        Ok(Self {
            renderer: Renderer::new(&state.device)?,
            texture: None,
            last: None,
        })
    }

    /// Whether a frame with `key` is already in the texture.
    pub fn is_current(&self, key: &FrameKey) -> bool {
        self.last.as_ref() == Some(key)
    }

    /// Draws the display list (if anything changed) and returns the texture to show.
    pub fn render(
        &mut self,
        state: &RenderState,
        key: FrameKey,
        list: &DisplayList,
        assets: &AssetStore,
    ) -> libforma::Result<TextureId> {
        let [width, height] = key.size;
        self.ensure_texture(state, width, height);
        let (_, storage_view, id) = self.texture.as_ref().expect("texture exists");
        self.renderer.render(
            &state.device,
            &state.queue,
            list,
            assets,
            &RenderTarget {
                view: storage_view,
                width,
                height,
                transform: Affine::new(key.transform),
                // egui draws the canvas background and grid under the texture.
                clear_color: Color::TRANSPARENT,
                antialiasing: Antialiasing::Fast,
            },
        )?;
        self.last = Some(key);
        Ok(*id)
    }

    /// The texture from the last render.
    pub fn texture_id(&self) -> Option<TextureId> {
        self.texture.as_ref().map(|(_, _, id)| *id)
    }

    fn ensure_texture(&mut self, state: &RenderState, width: u32, height: u32) {
        if let Some((texture, ..)) = &self.texture
            && texture.width() == width
            && texture.height() == height
        {
            return;
        }
        let texture = state.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("forma canvas"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let storage_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        // egui samples the texture as is: Vello already writes sRGB-encoded values.
        let display_view = texture.create_view(&wgpu::TextureViewDescriptor {
            usage: Some(wgpu::TextureUsages::TEXTURE_BINDING),
            ..Default::default()
        });
        let mut egui_renderer = state.renderer.write();
        let id = match self.texture.take() {
            Some((_, _, id)) => {
                egui_renderer.update_egui_texture_from_wgpu_texture(
                    &state.device,
                    &display_view,
                    wgpu::FilterMode::Nearest,
                    id,
                );
                id
            }
            None => egui_renderer.register_native_texture(
                &state.device,
                &display_view,
                wgpu::FilterMode::Nearest,
            ),
        };
        self.texture = Some((texture, storage_view, id));
        self.last = None;
    }
}
