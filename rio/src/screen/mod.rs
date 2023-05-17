mod bindings;
mod messenger;
mod state;
pub mod window;

use crate::clipboard::{Clipboard, ClipboardType};
use crate::crosswords::grid::Scroll;
use crate::crosswords::{Crosswords, Mode};
use crate::event::sync::FairMutex;
use crate::event::EventProxy;
use crate::ime::Ime;
use crate::layout::Layout;
use crate::performer::Machine;
use crate::screen::bindings::{Action as Act, BindingMode, Key};
use messenger::Messenger;
use state::State;
use std::borrow::Cow;
use std::error::Error;
use std::rc::Rc;
use std::sync::Arc;
use sugarloaf::Sugarloaf;
use teletypewriter::create_pty;

pub struct Screen {
    bindings: bindings::KeyBindings,
    clipboard: Clipboard,
    ignore_chars: bool,
    layout: Layout,
    pub ime: Ime,
    pub messenger: Messenger,
    state: State,
    sugarloaf: Sugarloaf,
    terminal: Arc<FairMutex<Crosswords<EventProxy>>>,
}

impl Screen {
    pub async fn new(
        winit_window: &winit::window::Window,
        config: &Rc<config::Config>,
        event_proxy: EventProxy,
    ) -> Result<Screen, Box<dyn Error>> {
        let shell = std::env::var("SHELL")?;
        let size = winit_window.inner_size();
        let scale = winit_window.scale_factor();

        let mut layout = Layout::new(
            size.width as f32,
            size.height as f32,
            scale as f32,
            config.style.font_size,
        );
        let (columns, rows) = layout.compute();
        let pty = create_pty(&Cow::Borrowed(&shell), columns as u16, rows as u16);

        let power_preference: wgpu::PowerPreference = match config.performance {
            config::Performance::High => wgpu::PowerPreference::HighPerformance,
            config::Performance::Low => wgpu::PowerPreference::LowPower,
        };

        let sugarloaf = Sugarloaf::new(
            winit_window,
            power_preference,
            config.style.font.to_string(),
        )
        .await?;

        let state = State::new(config);

        let event_proxy_clone = event_proxy.clone();
        let terminal: Arc<FairMutex<Crosswords<EventProxy>>> =
            Arc::new(FairMutex::new(Crosswords::new(columns, rows, event_proxy)));

        let machine = Machine::new(Arc::clone(&terminal), pty, event_proxy_clone)?;
        let channel = machine.channel();
        machine.spawn();
        let messenger = Messenger::new(channel);

        let clipboard = Clipboard::new();
        let bindings = bindings::default_key_bindings();
        let ime = Ime::new();

        Ok(Screen {
            ime,
            sugarloaf,
            terminal,
            layout,
            messenger,
            state,
            bindings,
            clipboard,
            ignore_chars: false,
        })
    }

    #[inline]
    pub fn propagate_modifiers_state(&mut self, state: winit::event::ModifiersState) {
        self.messenger.set_modifiers(state);
    }

    #[inline]
    pub fn clipboard_get(&mut self, clipboard_type: ClipboardType) -> String {
        self.clipboard.get(clipboard_type)
    }

    pub fn input_character(&mut self, character: char) {
        if self.ime.preedit().is_some() {
            return;
        }

        let ignore_chars = self.ignore_chars;
        // || self.ctx.terminal().mode().contains(TermMode::VI)
        if ignore_chars {
            return;
        }

        let utf8_len = character.len_utf8();
        let mut bytes = vec![0; utf8_len];
        character.encode_utf8(&mut bytes[..]);

        #[cfg(not(target_os = "macos"))]
        let alt_send_esc = true;

        #[cfg(target_os = "macos")]
        let alt_send_esc = self.state.option_as_alt;

        if alt_send_esc && self.messenger.get_modifiers().alt() && utf8_len == 1 {
            bytes.insert(0, b'\x1b');
        }

        self.messenger.send_bytes(bytes);
    }

    pub fn get_mode(&self) -> Mode {
        let terminal = self.terminal.lock();
        terminal.mode()
    }

    #[inline]
    pub fn input_keycode(
        &mut self,
        virtual_keycode: Option<winit::event::VirtualKeyCode>,
        scancode: u32,
    ) {
        if self.ime.preedit().is_some() {
            return;
        }

        let mode = BindingMode::new(&self.get_mode());
        let mods = self.messenger.get_modifiers();
        let mut ignore_chars = None;

        for i in 0..self.bindings.len() {
            let binding = &self.bindings[i];

            let key = match (binding.trigger, virtual_keycode) {
                (Key::Scancode(_), _) => Key::Scancode(scancode),
                (_, Some(key)) => Key::Keycode(key),
                _ => continue,
            };

            if binding.is_triggered_by(mode.clone(), mods, &key) {
                *ignore_chars.get_or_insert(true) &= binding.action != Act::ReceiveChar;

                match &binding.action {
                    Act::Esc(s) => {
                        self.messenger.send_bytes(
                            s.replace("\r\n", "\r").replace('\n', "\r").into_bytes(),
                        );
                    }
                    Act::Paste => {
                        let content = self.clipboard.get(ClipboardType::Clipboard);
                        self.paste(&content, true);
                        // self.messenger.send_bytes(content.as_bytes().to_vec());
                    }
                    Act::ReceiveChar | Act::None => (),
                    _ => (),
                }
            }
        }

        self.ignore_chars = ignore_chars.unwrap_or(false);
    }

    #[inline]
    pub fn paste(&mut self, text: &str, bracketed: bool) {
        if bracketed && self.get_mode().contains(Mode::BRACKETED_PASTE) {
            self.messenger.send_bytes(b"\x1b[200~"[..].to_vec());

            // Write filtered escape sequences.
            //
            // We remove `\x1b` to ensure it's impossible for the pasted text to write the bracketed
            // paste end escape `\x1b[201~` and `\x03` since some shells incorrectly terminate
            // bracketed paste on its receival.
            let filtered = text.replace(['\x1b', '\x03'], "");
            self.messenger.send_bytes(filtered.into_bytes());

            self.messenger.send_bytes(b"\x1b[201~"[..].to_vec());
        } else {
            self.messenger
                .send_bytes(text.replace("\r\n", "\r").replace('\n', "\r").into_bytes());
        }
    }

    #[inline]
    pub fn skeleton(&mut self, color: colors::ColorWGPU) {
        self.sugarloaf.init(color, self.layout.styles.term);
    }

    #[inline]
    pub fn render(&mut self) {
        let mut terminal = self.terminal.lock();
        let visible_rows = terminal.visible_rows();
        let cursor_position = terminal.cursor();
        drop(terminal);

        self.state.update(
            visible_rows,
            cursor_position,
            &mut self.sugarloaf,
            self.layout.styles.term,
            // self.ime
        );

        self.sugarloaf.render();
    }

    #[inline]
    pub fn scroll(&mut self, _new_scroll_x_px: f64, new_scroll_y_px: f64) {
        // let width = self.layout.width as f64;
        // let height = self.layout.height as f64;

        // if self
        //     .ctx
        //     .terminal()
        //     .mode()
        //     .contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL)
        //     && !self.ctx.modifiers().shift()
        // {
        // // let multiplier = f64::from(self.ctx.config().terminal_config.scrolling.multiplier);

        // // self.layout.mouse_mut().accumulated_scroll.x += new_scroll_x_px;//* multiplier;
        // // self.layout.mouse_mut().accumulated_scroll.y += new_scroll_y_px;// * multiplier;

        // // // The chars here are the same as for the respective arrow keys.
        // let line_cmd = if new_scroll_y_px > 0. { b'A' } else { b'B' };
        // let column_cmd = if new_scroll_x_px > 0. { b'D' } else { b'C' };

        // // let lines = (self.layout.cursor.accumulated_scroll.y / self.layout.font_size as f64).abs() as usize;
        // let lines = 1;
        // let columns = (self.layout.cursor.accumulated_scroll.x / width).abs() as usize;

        // let mut content = Vec::with_capacity(3 * (lines + columns));

        // for _ in 0..lines {
        //     content.push(0x1b);
        //     content.push(b'O');
        //     content.push(line_cmd);
        // }

        // for _ in 0..columns {
        //     content.push(0x1b);
        //     content.push(b'O');
        //     content.push(column_cmd);
        // }

        // println!("{:?} {:?} {:?} {:?}", content, lines, columns, self.layout.cursor);
        // if content.len() > 0 {
        //     self.messenger.write_to_pty(content);
        // }
        // }

        self.layout.mouse_mut().accumulated_scroll.y +=
            new_scroll_y_px * self.layout.mouse.multiplier;
        let lines = (self.layout.mouse.accumulated_scroll.y
            / self.layout.font_size as f64) as i32;

        if lines != 0 {
            let mut terminal = self.terminal.lock();
            terminal.scroll_display(Scroll::Delta(lines));
            drop(terminal);
        }
    }

    pub fn layout(&mut self) -> &mut Layout {
        &mut self.layout
    }

    #[inline]
    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) -> &mut Self {
        self.sugarloaf.resize(new_size.width, new_size.height);
        self.layout
            .set_size(new_size.width, new_size.height)
            .update();
        let (c, l) = self.layout.compute();

        let mut terminal = self.terminal.lock();
        terminal.resize::<Layout>(self.layout.columns, self.layout.rows);
        drop(terminal);

        let _ = self.messenger.send_resize(
            new_size.width as u16,
            new_size.height as u16,
            c as u16,
            l as u16,
        );
        self
    }

    pub fn set_scale(
        &mut self,
        new_scale: f32,
        new_size: winit::dpi::PhysicalSize<u32>,
    ) -> &mut Self {
        self.sugarloaf
            .resize(new_size.width, new_size.height)
            .rescale(new_scale);

        self.layout
            .set_scale(new_scale)
            .set_size(new_size.width, new_size.height)
            .update();
        self
    }
}
