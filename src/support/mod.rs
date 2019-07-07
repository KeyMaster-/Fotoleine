use glium::glutin::{self, ContextBuilder};
use glium::glutin::window::{WindowBuilder};
use glium::glutin::event_loop::{EventLoop, ControlFlow};
use glium::glutin::event::Event;
use glium::glutin::event::WindowEvent;
use glium::{Display, Surface};
use imgui::{Context, FontConfig, FontGlyphRanges, FontSource, Ui};
use imgui_glium_renderer::GliumRenderer;
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use std::time::Instant;

#[derive(Debug)]
pub enum LoopSignal {
  Wait, // don't render, wait for next event
  Quit, // exit loop
  Render, // render the imgui ui
}

pub struct System {
  pub events_loop: EventLoop<()>,
  pub display: Display,
  pub imgui: Context,
  pub platform: WinitPlatform,
  pub renderer: GliumRenderer,
  pub font_size: f32,
}

pub fn init(title: &str) -> System {
  let title = match title.rfind('/') {
    Some(idx) => title.split_at(idx + 1).1,
    None => title,
  };
  let events_loop = EventLoop::new();
  let context = ContextBuilder::new().with_vsync(true);
  let builder = WindowBuilder::new()
    .with_title(title.to_owned())
    .with_inner_size(glutin::dpi::LogicalSize::new(1024f64, 768f64));
  let display =
    Display::new(builder, context, &events_loop).expect("Failed to initialize display");

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

  System {
    events_loop,
    display,
    imgui,
    platform,
    renderer,
    font_size,
  }
}

impl System {
  // fn test() {
  //     println!("hi");
  // }

  pub fn main_loop<
    // EventF: FnMut(&Event<()>)->LoopSignal,
    // UiF: 'static + FnMut(&mut Ui)->bool>(self, mut process_events:EventF, mut run_ui: UiF) {

    UiF: 'static + FnMut(&mut Ui)->bool>(self, mut run_ui: UiF) {
    let System {
      events_loop,
      display,
      mut imgui,
      mut platform,
      mut renderer,
      ..
    } = self;
    let mut last_frame = Instant::now();

    events_loop.run(move |event, _, control_flow| {
      let gl_window = display.gl_window();
      let window = gl_window.window();

      platform.handle_event(imgui.io_mut(), &window, &event);

      *control_flow = ControlFlow::Wait;

      match event {
        Event::WindowEvent{event:win_event, .. } => {
          match win_event {
            WindowEvent::CloseRequested => {
              *control_flow = ControlFlow::Exit
            },
            WindowEvent::RedrawRequested => {
              println!("redrawing");
              let io = imgui.io_mut();
              platform
                .prepare_frame(io, &window)
                .expect("Failed to start frame");
              last_frame = io.update_delta_time(last_frame);
              let mut ui = imgui.frame();
              run_ui(&mut ui);

              let mut target = display.draw();
              target.clear_color_srgb(0.1, 0.1, 0.1, 1.0);
              platform.prepare_render(&ui, &window);
              let draw_data = ui.render();
              renderer
                .render(&mut target, draw_data)
                .expect("Rendering failed");
              target.finish().expect("Failed to swap buffers");
            },
            WindowEvent::Resized { .. } | WindowEvent::Focused { .. } | WindowEvent::HiDpiFactorChanged { .. } |
            WindowEvent::KeyboardInput { .. } | 
            WindowEvent::CursorMoved { .. } | WindowEvent::CursorEntered { .. } | WindowEvent::CursorLeft { .. } |
            WindowEvent::MouseWheel { .. } | WindowEvent::MouseInput { .. } => {
              window.request_redraw();
              println!("redraw requested");
            },
            _ => {}
          }
        },
        _ => {}
      }

      // let mut loop_signal = process_events(&event);
      // match loop_signal {
      //   LoopSignal::Wait => {
      //     if render_decay != 0 {
      //       loop_signal = LoopSignal::Render;
      //       render_decay -= 1;
      //     }
      //   },
      //   LoopSignal::Render => {
      //     render_decay = RENDER_DECAY_TIME;
      //   },
      //   LoopSignal::Quit => {}
      // }

      // match loop_signal {
      //   LoopSignal::Wait => ControlFlow::Continue,
      //   LoopSignal::Quit => ControlFlow::Break,
      //   LoopSignal::Render => {
      //     let io = imgui.io_mut();
      //     platform
      //       .prepare_frame(io, &window)
      //       .expect("Failed to start frame");
      //     last_frame = io.update_delta_time(last_frame);
      //     let mut ui = imgui.frame();
      //     let keep_running = run_ui(&mut ui);

      //     let mut target = display.draw();
      //     target.clear_color_srgb(0.1, 0.1, 0.1, 1.0);
      //     platform.prepare_render(&ui, &window);
      //     let draw_data = ui.render();
      //     renderer
      //       .render(&mut target, draw_data)
      //       .expect("Rendering failed");
      //     target.finish().expect("Failed to swap buffers");

      //     if keep_running {
      //       ControlFlow::Continue
      //     } else {
      //       ControlFlow::Break
      //     }
      //   }
      // }
    });

    // while run {
      

    //   let io = imgui.io_mut();
    //   platform
    //     .prepare_frame(io, &window)
    //     .expect("Failed to start frame");
    //   last_frame = io.update_delta_time(last_frame);
    //   let mut ui = imgui.frame();
    //   run_ui(&mut run, &mut ui);

    //   let mut target = display.draw();
    //   target.clear_color_srgb(0.1, 0.1, 0.1, 1.0);
    //   platform.prepare_render(&ui, &window);
    //   let draw_data = ui.render();
    //   renderer
    //     .render(&mut target, draw_data)
    //     .expect("Rendering failed");
    //   target.finish().expect("Failed to swap buffers");
    // }
  }
}
