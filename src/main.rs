use std::error::Error;
use std::path::Path;
use std::process::Command;
use imgui::*;
use glium::{
  Surface,
  backend::Facade,
};
use glium::glutin::event_loop::{EventLoop, EventLoopProxy, EventLoopClosed};
use glium::glutin::event::{Event, WindowEvent, VirtualKeyCode};
use support::{init, Program, Framework, LoopSignal, run, begin_frame, end_frame};
use loaded_dir::LoadedDir;
use image_display::ImageDisplay;
use worker_pool::{WorkerPool, Worker};
use std::sync::mpsc::Sender;

mod support;
mod image;
mod loaded_dir;
mod image_display;
mod worker_pool;

struct LoadWorker {
  id: usize,
  event_loop_proxy: EventLoopProxy<<Fotoleine as Program>::UserEvent>,
}

impl Worker for LoadWorker {
  type Input = String;
  type Output = usize;

  fn execute(&mut self, input: Self::Input, output: Sender<Self::Output>) {
    match output.send(input.len()) {
      Ok(_) => println!("Worker {}: channel send succeeded", self.id),
      Err(error) => println!("Worker {}: channel send failed, {}", self.id, error)
    };

    match self.event_loop_proxy.send_event(self.id) {
      Ok(()) => println!("Worker {}: Send succeeded", self.id),
      Err(EventLoopClosed) => println!("Worker {}: Event loop closed", self.id)
    };
  }
}

struct Fotoleine {
  framework: Framework,
  view_area_size: [f32; 2],
  loaded_dir: Option<LoadedDir>,
  image_display: ImageDisplay,
  worker_pool: WorkerPool<LoadWorker>
}

impl Program for Fotoleine {
  type UserEvent = usize;

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
      Event::UserEvent(num) => {
        println!("User event received, value {}", num);
        LoopSignal::Wait
      },
      _ => LoopSignal::Wait
    };

    match event {
      Event::WindowEvent{event:win_event, .. } => {
        match win_event {
          WindowEvent::DroppedFile(path) => {
            self.load_path(path);
            if let Some(ref mut loaded_dir) = self.loaded_dir {
              loaded_dir.shown_image.place_to_fit(self.view_area_size, 20.0);
            }
          }
          _ => {}
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

    if let Some(ref mut loaded_dir) = self.loaded_dir {
      let gl_ctx = self.framework.display.get_context();
      let change_image_res = 
        if ui.is_key_pressed(VirtualKeyCode::A as _) {
          loaded_dir.change_image(-1, gl_ctx)
        } else if ui.is_key_pressed(VirtualKeyCode::D as _) {
          loaded_dir.change_image( 1, gl_ctx)
        } else {
          Ok(())
        };

      match change_image_res {
        Ok(_) => {
          loaded_dir.shown_image.place_to_fit(self.view_area_size, 20.0); // :todo: unify with path load
        },
        Err(error) => {
          println!("Error changing image: {}", error);
        }
      }

      if ui.is_key_pressed(VirtualKeyCode::O as _) {
        let mut path = loaded_dir.entries[loaded_dir.shown_idx].path();
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

    if let Some(ref loaded_dir) = self.loaded_dir {
      self.image_display.draw_image(&loaded_dir.shown_image, &mut target);
    }

    self.framework.renderer
      .render(&mut target, draw_data)
      .expect("Rendering failed");
    target.finish().expect("Failed to swap buffers");

    loop_signal
  }

  fn on_shutdown(&mut self) {
    // if let Some(thread_handle) = self.thread_handle.take() {
    //   thread_handle.join().expect("Couldn't join on other thread");
    // }
    println!("Shutting down");
  }
}

impl Fotoleine {
  fn init(framework: Framework, display_size:&[f32; 2], event_loop: &EventLoop<<Self as Program>::UserEvent>)->Result<Fotoleine, Box<dyn Error>> {
    let image_display = ImageDisplay::new(&framework.display, &display_size)?;
    let worker_pool = WorkerPool::new(4, |id| {
      LoadWorker {
        id: id,
        event_loop_proxy: event_loop.create_proxy()
      }
    }, String::from("hello"));

    for output in worker_pool.output.iter() {
      println!("{:?}", output);
    };

    // let join_handle = thread::spawn(move || {
    //   for i in 0..10 {
    //     println!("From thread: {}", i);
    //     match event_loop_proxy.send_event(i) {
    //       Ok(()) => println!("Send succeeded"),
    //       Err(EventLoopClosed) => println!("Event loop closed")
    //     };
    //     thread::sleep(Duration::from_millis(1000))
    //   }
    // });


    Ok(Fotoleine {
      framework: framework,
      view_area_size: [display_size[0], display_size[1]],
      loaded_dir: None,
      image_display: image_display,
      worker_pool: worker_pool
    })
  }

  fn load_path(&mut self, path:&Path) {
    let gl_ctx = self.framework.display.get_context();
    self.loaded_dir = LoadedDir::new(&path, gl_ctx).ok();
  }

  fn build_ui(&mut self, ui:&mut Ui) {
  }
}

fn main() {
  let display_size = [1280, 720];
  let (event_loop, imgui, framework) = init("fotoleine", display_size);
  let fotoleine = Fotoleine::init(framework, &[display_size[0] as f32, display_size[1] as f32], &event_loop).expect("Couldn't initialize Fotoleine.");

  run(event_loop, imgui, fotoleine);
}
