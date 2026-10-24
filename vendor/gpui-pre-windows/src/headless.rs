//! Test-only WARP rendering. No WindowsPlatform, HWND, swap chain,
//! DirectComposition target, presentation or event loop is constructed.
use super::*;

pub fn headless_text_system() -> Result<Arc<dyn PlatformTextSystem>> {
    Ok(Arc::new(DirectWriteTextSystem::new(
        &DirectXDevices::warp()?,
    )?))
}

pub struct DirectXHeadlessRenderer(DirectXRenderer);

impl DirectXHeadlessRenderer {
    pub fn new() -> Result<Self> {
        let devices = DirectXRendererDevices::new(&DirectXDevices::warp()?, true)?;
        let atlas = Arc::new(DirectXAtlas::new(&devices.device, &devices.device_context));
        let globals = DirectXGlobalElements::new(&devices.device)?;
        let pipelines = DirectXRenderPipelines::new(&devices.device)?;
        Ok(Self(DirectXRenderer {
            hwnd: HWND::default(),
            atlas,
            devices: Some(devices),
            resources: None,
            globals,
            pipelines,
            direct_composition: None,
            font_info: DirectXRenderer::get_font_info(),
            width: 0,
            height: 0,
            skip_draws: false,
        }))
    }

    fn resize(&mut self, size: Size<DevicePixels>) -> Result<()> {
        let width = size.width.0.max(1) as u32;
        let height = size.height.0.max(1) as u32;
        if self.0.width == width && self.0.height == height {
            return Ok(());
        }
        let devices = self.0.devices.as_ref().context("missing WARP device")?;
        unsafe { devices.device_context.OMSetRenderTargets(None, None) };
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: RENDER_TARGET_FORMAT,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
            ..Default::default()
        };
        let mut render_target = None;
        let mut render_target_view = None;
        unsafe {
            devices
                .device
                .CreateTexture2D(&desc, None, Some(&mut render_target))?;
            devices.device.CreateRenderTargetView(
                render_target
                    .as_ref()
                    .context("missing offscreen texture")?,
                None,
                Some(&mut render_target_view),
            )?;
        }
        let (path_intermediate_texture, path_intermediate_srv) =
            create_path_intermediate_texture(&devices.device, width, height)?;
        let (path_intermediate_msaa_texture, path_intermediate_msaa_view) =
            create_path_intermediate_msaa_texture_and_view(&devices.device, width, height)?;
        set_rasterizer_state(&devices.device, &devices.device_context)?;
        self.0.resources = Some(DirectXResources {
            swap_chain: None,
            render_target,
            render_target_view,
            path_intermediate_texture,
            path_intermediate_srv,
            path_intermediate_msaa_texture,
            path_intermediate_msaa_view,
            viewport: D3D11_VIEWPORT {
                Width: width as f32,
                Height: height as f32,
                MaxDepth: 1.,
                ..Default::default()
            },
        });
        self.0.width = width;
        self.0.height = height;
        Ok(())
    }
}

impl PlatformHeadlessRenderer for DirectXHeadlessRenderer {
    fn render_scene_to_image(
        &mut self,
        scene: &Scene,
        size: Size<DevicePixels>,
    ) -> Result<image::RgbaImage> {
        self.resize(size)?;
        self.0
            .render_to_image(scene, WindowBackgroundAppearance::Opaque)
    }

    fn render_scene(&mut self, scene: &Scene, size: Size<DevicePixels>) -> Result<()> {
        self.resize(size)?;
        self.0.render(scene, WindowBackgroundAppearance::Opaque)
    }

    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        self.0.sprite_atlas()
    }
}
