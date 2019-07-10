use glium::glutin::{self, ContextBuilder};
use glium::glutin::window::{WindowBuilder};
use glium::glutin::event_loop::{EventLoop, ControlFlow};
use glium::glutin::event::Event;
use glium::glutin::event::WindowEvent;
use glium::Display;
use imgui::{Context, FontConfig, FontGlyphRanges, FontSource, Ui, DrawData};
use imgui_glium_renderer::GliumRenderer;
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use std::time::Instant;

pub struct Framework {
  pub display: Display,
  pub platform: WinitPlatform,
  pub imgui: Context,
  pub renderer: GliumRenderer,
}

pub fn init(title: &str) -> (EventLoop<()>, Framework) {
  let event_loop = EventLoop::new();
  let context = ContextBuilder::new().with_vsync(true);
  let builder = WindowBuilder::new()
    .with_title(title.to_owned())
    .with_inner_size(glutin::dpi::LogicalSize::new(1024f64, 768f64));
  let display =
    Display::new(builder, context, &event_loop).expect("Failed to initialize display");

  let mut imgui = Context::create();
  imgui.set_ini_filename(None);

  let mut platform = WinitPlatform::init(&mut imgui);
  {
    let gl_window = display.gl_window();
    let window = gl_window.window();
    platform.attach_window(imgui.io_mut(), window, HiDpiMode::Rounded);
  }

  let hidpi_factor = platform.hidpi_factor();
  let font_size = (13.0 * hidpi_factor) as f32;
  imgui.fonts().add_font(&[
    FontSource::DefaultFontData {
      config: Some(FontConfig {
        size_pixels: font_size,
        ..FontConfig::default()
      }),
    },
    FontSource::TtfData {
      data: include_bytes!("../../resources/mplus-1p-regular.ttf"),
      size_pixels: font_size,
      config: Some(FontConfig {
        rasterizer_multiply: 1.75,
        glyph_ranges: FontGlyphRanges::japanese(),
        ..FontConfig::default()
      }),
    },
  ]);

  imgui.io_mut().font_global_scale = (1.0 / hidpi_factor) as f32;

  let renderer =
    GliumRenderer::init(&mut imgui, &display).expect("Failed to initialize renderer");

  let framework = Framework {
    display,
    platform,
    imgui,
    renderer
  };

  (event_loop, framework)
}

pub fn begin_frame<'ui>(imgui:&'ui mut Context, platform:&WinitPlatform, display:&Display)->Ui<'ui> {
  let io = imgui.io_mut();
  let gl_window = display.gl_window();
  let window = gl_window.window();
  platform
    .prepare_frame(io, window)
    .expect("Failed to start frame");
  
  imgui.frame()
}

pub fn end_frame<'ui>(ui:Ui<'ui>, platform:&WinitPlatform, display:&Display)->&'ui DrawData {
  let gl_window = display.gl_window();
  let window = gl_window.window();
  platform.prepare_render(&ui, window);
  ui.render()
}

pub trait Program {
  fn on_event(&mut self, event:&Event<()>)->bool;
  fn on_draw(&mut self, framework:&mut Framework);
}

pub fn run<P:'static + Program>(event_loop:EventLoop<()>, mut framework: Framework, mut program: P) {
  let mut last_frame = Instant::now();
  let mut first_redraw = false;

  event_loop.run(move |event, _, control_flow| {

    internal_handle_event(&mut framework.imgui, &mut framework.platform, &framework.display, &event);

    let should_exit = program.on_event(&event);
    if should_exit {
      *control_flow = ControlFlow::Exit;
      return
    }

    *control_flow = ControlFlow::Wait;

    match event {
      Event::WindowEvent{event:win_event, .. } => {
        match win_event {
          WindowEvent::RedrawRequested => {
            let io = framework.imgui.io_mut();
            last_frame = io.update_delta_time(last_frame);

            program.on_draw(&mut framework);
              // imgui doesn't react to some events on the same frame they arrive at, but rather one frame late
              // E.g. if a mouse release arrives, the first frame rendered after that won't see its effects, only the second
              // So for every event that arrives, we actually do two redraws, to be sure those events take effect
              // Doing this through two requests is crucial for framerate, if we just did draw_ui twice here every frame would effectively be twice as long
            if first_redraw {
              first_redraw = false;
              let gl_window = framework.display.gl_window();
              let window = gl_window.window();
              window.request_redraw();
            }
          },
          WindowEvent::Resized { .. } | WindowEvent::Focused { .. } | WindowEvent::HiDpiFactorChanged { .. } |
          WindowEvent::KeyboardInput { .. } | 
          WindowEvent::CursorMoved { .. } | WindowEvent::CursorEntered { .. } | WindowEvent::CursorLeft { .. } |
          WindowEvent::MouseWheel { .. } | WindowEvent::MouseInput { .. } => {
            let gl_window = framework.display.gl_window();
            let window = gl_window.window();
            window.request_redraw();
            first_redraw = true;
          },
          _ => {}
        }
      },
      _ => {}
    }
  });
}

fn internal_handle_event(imgui:&mut Context, platform:&mut WinitPlatform, display:&Display, event:&Event<()>) {
  let gl_window = display.gl_window();
  let window = gl_window.window();
  platform.handle_event(imgui.io_mut(), window, event);
}

