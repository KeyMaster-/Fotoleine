use imgui::*;
use glium::Surface;
use glium::glutin::event::{Event, WindowEvent};
use support::{init, Program, Framework, run, begin_frame, end_frame};

mod support;

struct Fotoleine {
  test_val: i32
}

impl Program for Fotoleine {
  fn on_event(&mut self, event:&Event<()>)->bool {
    match event {
      Event::WindowEvent{event:win_event, .. } => {
        match win_event {
          WindowEvent::CloseRequested => {
            return true;
          },
          _ => {}
        }
      },
      _ => {}
    }
    return false;
  }

  fn on_draw(&mut self, framework:&mut Framework) {
    let Framework {
      ref display,
      ref platform,
      ref mut imgui,
      ref mut renderer
    } = framework;

    let mut ui = begin_frame(imgui, platform, display);
    ui_cb(&mut ui);
    let draw_data = end_frame(ui, platform, display);

    let mut target = display.draw();
    target.clear_color_srgb(0.1, 0.1, 0.1, 1.0);

    renderer
      .render(&mut target, draw_data)
      .expect("Rendering failed");
    target.finish().expect("Failed to swap buffers");
  }
}

fn ui_cb(ui:&mut Ui) {
  ui.window(im_str!("Hello world"))
    .size([300.0, 100.0], Condition::FirstUseEver)
    .build(|| {
      ui.text(im_str!("Hello world!"));
      ui.text(im_str!("こんにちは世界！"));
      ui.text(im_str!("This...is...imgui-rs!"));
      ui.separator();
      // ui.input_text()
      let mouse_pos = ui.io().mouse_pos;
      ui.text(format!(
        "Mouse Position: ({:.1},{:.1})",
        mouse_pos[0], mouse_pos[1]
      ));
    });
}

fn main() {
  let (event_loop, framework) = init("fotoleine");
  let fotoleine = Fotoleine {
    test_val: 5
  };

  run(event_loop, framework, fotoleine);
}
