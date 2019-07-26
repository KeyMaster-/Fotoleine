use std::error::Error;
use std::process::Command;
use imgui::*;
use glium::{
  Surface,
  backend::Facade,
};
use glium::glutin::event_loop::EventLoop;
use glium::glutin::event::{Event, WindowEvent, VirtualKeyCode};
use support::{init, Program, Framework, LoopSignal, run, begin_frame, end_frame};
use image_display::ImageDisplay;
use image_handling::{ImageHandling, loader_pool::LoadNotification};

mod support;
mod image;
mod image_handling;
mod image_display;
mod worker_pool;

struct Fotoleine {
  framework: Framework,
  image_handling: ImageHandling,
  image_display: ImageDisplay,
  view_area_size: [f32; 2],
}

impl Fotoleine {
  fn init(framework: Framework, display_size:&[f32; 2], event_loop: &EventLoop<LoadNotification>)->Result<Fotoleine, Box<dyn Error>> {
    let image_display = ImageDisplay::new(&framework.display, &display_size)?;
    let image_handling = ImageHandling::new(10, 4, &event_loop);

    Ok(Fotoleine {
      framework,
      image_handling,
      image_display,
      view_area_size: [display_size[0], display_size[1]],
    })
  }

  fn build_ui(&mut self, ui:&mut Ui) {
  }
}

impl Program for Fotoleine {
  type UserEvent = LoadNotification;

  fn framework(&self)->&Framework {
    return &self.framework;
  }

  fn framework_mut(&mut self)->&mut Framework {
    return &mut self.framework;
  }

  fn on_event(&mut self, event:&Event<Self::UserEvent>)->LoopSignal {
    let loop_signal = match event {
      Event::WindowEvent{event:win_event, .. } => {
        match win_event {
          WindowEvent::CloseRequested 
            => LoopSignal::Exit,
          WindowEvent::Resized { .. } | WindowEvent::Focused { .. } | WindowEvent::HiDpiFactorChanged { .. } |
          WindowEvent::KeyboardInput { .. } | 
          WindowEvent::CursorMoved { .. } | WindowEvent::CursorEntered { .. } | WindowEvent::CursorLeft { .. } |
          WindowEvent::MouseWheel { .. } | WindowEvent::MouseInput { .. } 
            => LoopSignal::Redraw,
          _ => LoopSignal::Wait
        }
      },
      Event::UserEvent(_) => LoopSignal::Redraw,
      _ => LoopSignal::Wait
    };

    match event {
      Event::WindowEvent{event:win_event, .. } => {
        match win_event {
          WindowEvent::DroppedFile(path) => {
            let load_res = self.image_handling.load_path(&path);
            if let Err(load_error) = load_res {
              println!("Couldn't load path {}: {}", path.to_string_lossy(), load_error);
            } else {
              if let Some(ref mut loaded_dir) = self.image_handling.loaded_dir {
                loaded_dir.set_shown(0, &self.image_handling.services);
              }
            }
          }
          _ => {}
        }
      },
      Event::UserEvent(notification) => {
        println!("User event received, value {:?}", notification);
        match notification {
          LoadNotification::ImageLoaded => {
            if let Some(ref mut loaded_dir) = self.image_handling.loaded_dir {
              let gl_ctx = self.framework.display.get_context();
              let load_res = loaded_dir.receive_image(&self.image_handling.services, gl_ctx);
              if let Err(error) = load_res {
                println!("Error receiving image: {}", error);
              }
            } else {
                //:todo: this could happen if an invalid path was loaded while a load was pending
                // it's fine to discard the image in that case though
              println!("Received load result, but loaded_dir does not exist!");
            }
            // let load_output_res = self.loader_pool.output.recv();
            // if let Ok(load_output) = load_output_res {

            //   if let Some(ref mut loaded_dir) = self.loaded_dir {

            //     let loaded_idx = load_output.1;

            //     let gl_ctx = self.framework.display.get_context();
            //     let process_res = loaded_dir.process_loaded_image(load_output, gl_ctx);
            //     if let Err(error) = process_res {
            //       println!("Couldn't process loaded image: {}", error);
            //     }

            //     if let Some(ref mut placed_image) = loaded_dir.image_at_mut(loaded_idx) {
            //       placed_image.place_to_fit(self.view_area_size, 20.0);
            //     }

            //   } else {
            //     println!("Got load result despite loaded dir not existing?");
            //   }
            // } else {
            //   println!("Error reaceiving loaded image data");
            // }
          },
          LoadNotification::LoadFailed => {
            println!("Image loading failed!");
            // :todo: set a flag and show this in the ui
            // also, maybe send image id along with notification to see whether the failed load was on the image we showed,
            // also to make decisions in loaded dir about re-requesting maybe
          }
        }
      },
      _ => {}
    };

    loop_signal
  }

  fn on_frame(&mut self, imgui: &mut Context)->LoopSignal {
    let mut loop_signal = LoopSignal::Wait;

    let mut ui = begin_frame(imgui, &self.framework.platform, &self.framework.display);

    if ui.is_key_pressed(VirtualKeyCode::Q as _) && ui.io().key_super {
      loop_signal = LoopSignal::Exit;
    }

    if let Some(ref mut loaded_dir) = self.image_handling.loaded_dir {
      if ui.is_key_pressed(VirtualKeyCode::A as _) {
        loaded_dir.set_shown(loaded_dir.offset_idx(-1), &self.image_handling.services);
      } else if ui.is_key_pressed(VirtualKeyCode::D as _) {
        loaded_dir.set_shown(loaded_dir.offset_idx( 1), &self.image_handling.services);
      }

      if let Some(ref mut placed_image) = loaded_dir.image_at_mut(loaded_dir.shown_idx()) {
        placed_image.place_to_fit(self.view_area_size, 20.0);
      };

      if ui.is_key_pressed(VirtualKeyCode::O as _) {
        let mut path = loaded_dir.path_at(loaded_dir.shown_idx());
        path.set_extension("cr2");

        let open_res = Command::new("open")
          .arg(path.as_os_str())
          .output();

        if let Err(err) = open_res {
          println!("Couldn't open file {}, error {}", path.to_string_lossy(), err);
        }
      }
    }

    self.build_ui(&mut ui);

    let draw_data = end_frame(ui, &self.framework.platform, &self.framework.display);

    let mut target = self.framework.display.draw();
    target.clear_color_srgb(0.1, 0.1, 0.1, 1.0);

    if let Some(ref loaded_dir) = self.image_handling.loaded_dir {
      if let Some(ref placed_image) = loaded_dir.image_at(loaded_dir.shown_idx()) {
        self.image_display.draw_image(placed_image, &mut target);
      }
    }

    self.framework.renderer
      .render(&mut target, draw_data)
      .expect("Rendering failed");
    target.finish().expect("Failed to swap buffers");

    loop_signal
  }

  fn on_shutdown(&mut self) {
    println!("Shutting down");
  }
}

fn main() {

  let display_size = [1280, 720];
  let (event_loop, imgui, framework) = init("fotoleine", display_size);
  let fotoleine = Fotoleine::init(framework, &[display_size[0] as f32, display_size[1] as f32], &event_loop).expect("Couldn't initialize Fotoleine.");

  run(event_loop, imgui, fotoleine);
}
