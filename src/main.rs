use imgui::*;
use glium::glutin::event::{Event, WindowEvent};
use support::LoopSignal;

mod support;

fn main() {
  let system = support::init(file!());

  // let mut cooldown = 0;
  let event_cb = |event:&Event<()>| {
    let loop_signal = match event {
      Event::WindowEvent { event:win_event, .. } => {
        match win_event {
          WindowEvent::CloseRequested => LoopSignal::Quit,

          WindowEvent::RedrawRequested | WindowEvent::Resized { .. } | 
          WindowEvent::MouseInput { .. } | WindowEvent::MouseWheel { .. } | 
          WindowEvent::CursorMoved { .. } | WindowEvent::CursorEntered { .. } | WindowEvent::CursorLeft { .. } |
          WindowEvent::HiDpiFactorChanged { .. }
            => LoopSignal::Render,

          _ => LoopSignal::Wait
        }
      },
      _ => LoopSignal::Wait
    };

    // println!("{:?}", event);
    // if let LoopSignal::Render = loop_signal {
    //   if cooldown == 100 {
    //     cooldown = 0;
    //   } else {
    //     cooldown += 1;
    //     loop_signal = LoopSignal::Wait;
    //   }
    // }
    loop_signal
  };

  let ui_cb = |ui:&mut Ui| {
    ui.window(im_str!("Hello world"))
      .size([300.0, 100.0], Condition::FirstUseEver)
      .build(|| {
        ui.text(im_str!("Hello world!"));
        ui.text(im_str!("こんにちは世界！"));
        ui.text(im_str!("This...is...imgui-rs!"));
        ui.separator();
        let mouse_pos = ui.io().mouse_pos;
        ui.text(format!(
          "Mouse Position: ({:.1},{:.1})",
          mouse_pos[0], mouse_pos[1]
        ));
      });

    return true;
  };

  // system.main_loop(event_cb, ui_cb);
  system.main_loop(ui_cb);
}
