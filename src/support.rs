use glium::glutin::ContextBuilder;
use glium::glutin::window::{WindowBuilder};
use glium::glutin::event_loop::{EventLoop, ControlFlow};
use glium::glutin::event::{Event, WindowEvent};
use glium::glutin::dpi::LogicalSize;
use glium::Display;
use imgui::{Context, FontConfig, FontSource, Ui, DrawData};
use imgui_glium_renderer::GliumRenderer;
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use std::time::Instant;

pub struct Framework {
  pub display: Display,
  pub platform: WinitPlatform,
  pub renderer: GliumRenderer,
}

pub fn init<T>(title: &str, window_size: &LogicalSize) -> (EventLoop<T>, Context, Framework) {
  let event_loop = EventLoop::<T>::new_user_event();
  let context = ContextBuilder::new().with_vsync(true);
  let builder = WindowBuilder::new()
    .with_title(title.to_owned())
    .with_inner_size(window_size.clone());
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
    }
  ]);

  imgui.io_mut().font_global_scale = (1.0 / hidpi_factor) as f32;

  let renderer =
    GliumRenderer::init(&mut imgui, &display).expect("Failed to initialize renderer");

  let framework = Framework {
    display,
    platform,
    renderer
  };

  (event_loop, imgui, framework)
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
  type UserEvent;

  fn framework(&self)->&Framework;
  fn framework_mut(&mut self)->&mut Framework;
  fn on_event(&mut self, event: &Event<Self::UserEvent>)->LoopSignal;
  fn on_frame(&mut self, imgui: &mut Context)->LoopSignal;
  fn on_shutdown(&mut self);
}

  // The ordering determines "strength", lower signals are stronger and override weaker (higher up) signals
  // I.e. Exit > Redraw > Wait
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum LoopSignal {
  Wait,
  RequestRedraw,
  ImmediateRedraw,
  Exit
}

pub fn run<P:'static + Program>(event_loop: EventLoop<P::UserEvent>, mut imgui: Context, mut program: P)->! {
  let mut last_frame = Instant::now();
  let mut first_redraw = false;

  event_loop.run(move |event, _, control_flow| {
    {
      let framework = program.framework_mut();
      internal_handle_event(&mut imgui, &mut framework.platform, &framework.display, &event);
    }

    let mut loop_signal = program.on_event(&event);
    
    let mut redraw_event = false;
    match event {
      Event::WindowEvent{event:win_event, .. } => {
        match win_event {
          WindowEvent::RedrawRequested => {
            redraw_event = true;
          }
          _ => {}
        }
      },
      Event::LoopDestroyed => {
        program.on_shutdown();
      }
      _ => {}
    };

    if redraw_event || loop_signal == LoopSignal::ImmediateRedraw {
      {
        let io = imgui.io_mut();
        last_frame = io.update_delta_time(last_frame);
      }
      
      let frame_loop_signal = program.on_frame(&mut imgui);
      loop_signal = loop_signal.max(frame_loop_signal);

        // imgui doesn't react to some events on the same frame they arrive at, but rather one frame late
        // E.g. if a mouse release arrives, the first frame rendered after that won't see its effects, only the second
        // So for every event that arrives, we actually do two redraws, to be sure those events take effect
        // Doing this through two requests is crucial for framerate, if we just did draw_ui twice here every frame would effectively be twice as long
      if redraw_event && first_redraw {
        let framework = program.framework();
        first_redraw = false;
        let gl_window = framework.display.gl_window();
        let window = gl_window.window();
        window.request_redraw();
      }
    }

    *control_flow = match loop_signal {
      LoopSignal::Wait => ControlFlow::Wait,
      LoopSignal::RequestRedraw => {
        let framework = program.framework();
        let gl_window = framework.display.gl_window();
        let window = gl_window.window();
        window.request_redraw();
        first_redraw = true;
        ControlFlow::Wait
      },
      LoopSignal::ImmediateRedraw => ControlFlow::Wait,
      LoopSignal::Exit => ControlFlow::Exit
    };
  });
}

fn internal_handle_event<T>(imgui:&mut Context, platform:&mut WinitPlatform, display:&Display, event:&Event<T>) {
  let gl_window = display.gl_window();
  let window = gl_window.window();
  platform.handle_event(imgui.io_mut(), window, event);
}