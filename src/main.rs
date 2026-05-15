mod canvas;
mod input;

use std::num::NonZeroU32;
use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

use canvas::{Canvas, Tool};
use input::InputState;

const INIT_W: u32 = 1280;
const INIT_H: u32 = 720;

const HELP: &str =
    "Draw2D  |  P: pen  E: eraser  \
     1-7: colors  [ ]: brush size  \
     scroll: brush size  \
     Ctrl+Z: undo  Ctrl+C / Esc: clear";

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.run_app(&mut App::default()).expect("run app");
}

// ---------------------------------------------------------------------------
// Application shell
// ---------------------------------------------------------------------------

#[derive(Default)]
struct App {
    state: Option<State>,
}

/// All live state lives here so it can be re-created on resume.
struct State {
    window: Arc<Window>,
    // Context must outlive Surface; declare surface first so it is dropped first.
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    _context: softbuffer::Context<Arc<Window>>,
    canvas: Canvas,
    input: InputState,
    modifiers: ModifiersState,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return; // already initialised (e.g. mobile resume)
        }

        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title(HELP)
                        .with_inner_size(PhysicalSize::new(INIT_W, INIT_H)),
                )
                .expect("create window"),
        );

        let context =
            softbuffer::Context::new(window.clone()).expect("create softbuffer context");
        let surface =
            softbuffer::Surface::new(&context, window.clone()).expect("create surface");

        self.state = Some(State {
            surface,
            _context: context,
            canvas: Canvas::new(INIT_W, INIT_H),
            input: InputState::default(),
            modifiers: ModifiersState::empty(),
            window,
        });
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // Belt-and-suspenders: covers Cmd+Q / dock-quit paths that bypass
        // CloseRequested.  Safe to call even if already None.
        self.state = None;
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        // Drop state eagerly on any terminal window event so that the
        // softbuffer surface/context are destroyed while the display
        // connection is still valid.  On Linux/Wayland the surface sends
        // wl_surface destroy messages on drop; if those happen after the
        // connection is torn down you get a segfault.  On X11 panicking
        // inside a C callback (from an unwrap failure) is UB.
        if matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed) {
            self.state = None;
            event_loop.exit();
            return;
        }

        let Some(state) = self.state.as_mut() else { return };

        match event {
            WindowEvent::Resized(size) => {
                state.canvas.resize(size.width.max(1), size.height.max(1));
                present(state);
            }

            WindowEvent::RedrawRequested => present(state),

            // ----------------------------------------------------------------
            // Keyboard
            // ----------------------------------------------------------------
            WindowEvent::ModifiersChanged(m) => {
                state.modifiers = m.state();
            }

            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: key,
                        state: key_state,
                        ..
                    },
                ..
            } => {
                if key_state == ElementState::Pressed {
                    let mods = state.modifiers;
                    handle_key(state, &key, mods);
                    present(state);
                }
            }

            // ----------------------------------------------------------------
            // Mouse movement — draw if button held
            // ----------------------------------------------------------------
            WindowEvent::CursorMoved { position, .. } => {
                let prev = state.input.cursor;
                let cur = (position.x as f32, position.y as f32);
                state.input.cursor = cur;

                if state.input.lmb_down {
                    state.canvas.stroke(
                        prev.0 as i32,
                        prev.1 as i32,
                        cur.0 as i32,
                        cur.1 as i32,
                    );
                    present(state);
                }
            }

            // ----------------------------------------------------------------
            // Mouse buttons
            // ----------------------------------------------------------------
            WindowEvent::MouseInput { button, state: btn_state, .. } => {
                let pressed = btn_state == ElementState::Pressed;
                match button {
                    MouseButton::Left => {
                        if pressed {
                            state.canvas.push_history();
                            // Stamp a single dot so clicking without dragging still draws.
                            let (cx, cy) = state.input.cursor;
                            state.canvas.stroke(cx as i32, cy as i32, cx as i32, cy as i32);
                            present(state);
                        }
                        state.input.lmb_down = pressed;
                    }
                    MouseButton::Right => {
                        state.input.rmb_down = pressed;
                    }
                    _ => {}
                }
            }

            // ----------------------------------------------------------------
            // Scroll wheel — adjust brush size
            // ----------------------------------------------------------------
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 20.0,
                };
                state.canvas.adjust_brush(dy);
                // Update title to show new brush size.
                update_title(state);
                present(state);
            }

            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Key dispatch
// ---------------------------------------------------------------------------

fn handle_key(state: &mut State, key: &Key, mods: ModifiersState) {
    match key {
        Key::Named(NamedKey::Escape) => state.canvas.clear(),

        Key::Character(c) => {
            let s = c.as_str();

            // Ctrl chords first.
            if mods.control_key() {
                match s {
                    "z" | "Z" => state.canvas.undo(),
                    "c" | "C" => state.canvas.clear(),
                    _ => {}
                }
                return;
            }

            match s {
                // Tools
                "p" | "P" => state.canvas.set_tool(Tool::Pen),
                "e" | "E" => state.canvas.set_tool(Tool::Eraser),

                // Color palette  (0x00RRGGBB)
                "1" => state.canvas.set_color(0x111111), // near-black
                "2" => state.canvas.set_color(0xCC0000), // red
                "3" => state.canvas.set_color(0x007700), // green
                "4" => state.canvas.set_color(0x0055CC), // blue
                "5" => state.canvas.set_color(0xFF8800), // orange
                "6" => state.canvas.set_color(0x8800CC), // purple
                "7" => state.canvas.set_color(0xFFFFFF), // white (paint-over)

                // Brush size
                "[" | "{" => state.canvas.adjust_brush(-1.0),
                "]" | "}" => state.canvas.adjust_brush(1.0),
                "-" | "_" => state.canvas.adjust_brush(-2.0),
                "=" | "+" => state.canvas.adjust_brush(2.0),

                _ => {}
            }
        }

        _ => {}
    }

    update_title(state);
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn present(state: &mut State) {
    let size = state.window.inner_size();
    let (w, h) = (size.width, size.height);
    if w == 0 || h == 0 {
        return;
    }

    // Use ? via a closure so panics never unwind through C frames (X11/Wayland).
    let mut render = || -> Option<()> {
        state
            .surface
            .resize(NonZeroU32::new(w)?, NonZeroU32::new(h)?)
            .ok()?;

        let mut buf = state.surface.buffer_mut().ok()?;
        let pixels = state.canvas.pixels();
        let len = (w * h) as usize;
        let copy = len.min(pixels.len());
        buf[..copy].copy_from_slice(&pixels[..copy]);
        if copy < len {
            buf[copy..len].fill(0x00FFFFFF);
        }
        buf.present().ok()
    };
    render();
}

// ---------------------------------------------------------------------------
// Title bar status
// ---------------------------------------------------------------------------

fn update_title(state: &mut State) {
    let tool = match state.canvas.tool {
        Tool::Pen => "pen",
        Tool::Eraser => "eraser",
    };
    let color = state.canvas.color;
    let brush = state.canvas.brush_size;
    let title = format!(
        "Draw2D  |  tool: {tool}  color: #{color:06X}  brush: {brush}px  \
         | P/E: tool  1-7: color  []: size  Ctrl+Z: undo  Esc: clear"
    );
    state.window.set_title(&title);
}
