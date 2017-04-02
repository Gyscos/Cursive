

use {Cursive, Printer, With};
use direction::Direction;
use event::{Callback, Event, EventResult, Key};
use std::cell::RefCell;

use std::rc::Rc;
use theme::{ColorStyle, Effect};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
use utils::simple_suffix;
use vec::Vec2;
use view::View;


/// Input box where the user can enter and edit text.
///
/// # Examples
///
/// From the [edit example].
///
/// [edit example]: https://github.com/gyscos/Cursive/blob/master/examples/edit.rs
///
/// ```no_run
/// # extern crate cursive;
/// # use cursive::Cursive;
/// # use cursive::traits::*;
/// # use cursive::views::{Dialog, EditView, TextView};
/// # fn main() {
/// let mut siv = Cursive::new();
///
/// // Create a dialog with an edit text and a button.
/// // The user can either hit the <Ok> button,
/// // or press Enter on the edit text.
/// siv.add_layer(Dialog::new()
///     .title("Enter your name")
///     .padding((1, 1, 1, 0))
///     .content(EditView::new()
///         .on_submit(show_popup)
///         .with_id("name")
///         .fixed_width(20))
///     .button("Ok", |s| {
///         let name = s.call_on_id("name", |view: &mut EditView| view.get_content())
///             .unwrap();
///         show_popup(s, &name);
///     }));
///
/// fn show_popup(s: &mut Cursive, name: &str) {
///     if name.is_empty() {
///         s.add_layer(Dialog::info("Please enter a name!"));
///     } else {
///         let content = format!("Hello {}!", name);
///         s.pop_layer();
///         s.add_layer(Dialog::around(TextView::new(content))
///             .button("Quit", |s| s.quit()));
///     }
/// }
///
/// # }
/// ```
pub struct EditView {
    /// Current content.
    content: Rc<String>,
    /// Cursor position in the content, in bytes.
    cursor: usize,

    /// Number of bytes to skip at the beginning of the content.
    ///
    /// (When the content is too long for the display, we hide part of it)
    offset: usize,
    /// Last display length, to know the possible offset range
    last_length: usize,

    /// Callback when the content is modified.
    ///
    /// Will be called with the current content and the cursor position.
    on_edit: Option<Rc<Fn(&mut Cursive, &str, usize)>>,

    /// Callback when <Enter> is pressed.
    on_submit: Option<Rc<Fn(&mut Cursive, &str)>>,

    /// When `true`, only print `*` instead of the true content.
    secret: bool,

    /// Character to fill empty space
    filler: char,

    enabled: bool,
}

new_default!(EditView);

impl EditView {
    /// Creates a new, empty edit view.
    pub fn new() -> Self {
        EditView {
            content: Rc::new(String::new()),
            cursor: 0,
            offset: 0,
            last_length: 0, // scrollable: false,
            on_edit: None,
            on_submit: None,
            secret: false,
            filler: '_',
            enabled: true,
        }
    }

    /// If `secret` is `true`, the content won't be displayed in clear.
    ///
    /// Only `*` will be shown.
    pub fn set_secret(&mut self, secret: bool) {
        self.secret = secret;
    }

    /// Hides the content of the view.
    ///
    /// Only `*` will be shown.
    pub fn secret(self) -> Self {
        self.with(|s| s.set_secret(true))
    }

    /// Sets the character to fill in blank space
    ///
    /// Defaults to '_'
    pub fn set_filler(&mut self, filler: char) {
        self.filler = filler;
    }

    /// Disables this view.
    ///
    /// A disabled view cannot be selected.
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Disables this view.
    ///
    /// Chainable variant.
    pub fn disabled(self) -> Self {
        self.with(Self::disable)
    }

    /// Re-enables this view.
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Sets a mutable callback to be called whenever the content is modified.
    ///
    /// `callback` will be called with the view
    /// content and the current cursor position.
    ///
    /// *Warning*: this callback cannot be called recursively. If you somehow
    /// trigger this callback again in the given closure, it will be ignored.
    ///
    /// If you don't need a mutable closure but want the possibility of
    /// recursive calls, see [`set_on_edit`](#method.set_on_edit).
    pub fn set_on_edit_mut<F>(&mut self, callback: F)
        where F: FnMut(&mut Cursive, &str, usize) + 'static
    {
        let callback = RefCell::new(callback);
        // Here's the weird trick: if we're already borrowed,
        // just ignored the callback.
        self.set_on_edit(move |s, text, cursor| {
            if let Ok(mut f) = callback.try_borrow_mut() {
                // Beeeaaah that's ugly.
                // Why do we need to manually dereference here?
                (&mut *f)(s, text, cursor);
            }
        });
    }

    /// Sets a callback to be called whenever the content is modified.
    ///
    /// `callback` will be called with the view
    /// content and the current cursor position.
    ///
    /// This callback can safely trigger itself recursively if needed
    /// (for instance if you call `on_event` on this view from the callback).
    ///
    /// If you need a mutable closure and don't care about the recursive
    /// aspect, see [`set_on_edit_mut`](#method.set_on_edit_mut).
    pub fn set_on_edit<F>(&mut self, callback: F)
        where F: Fn(&mut Cursive, &str, usize) + 'static
    {
        self.on_edit = Some(Rc::new(callback));
    }

    /// Sets a mutable callback to be called whenever the content is modified.
    ///
    /// Chainable variant. See [`set_on_edit_mut`](#method.set_on_edit_mut).
    pub fn on_edit_mut<F>(self, callback: F) -> Self
        where F: FnMut(&mut Cursive, &str, usize) + 'static
    {
        self.with(|v| v.set_on_edit_mut(callback))
    }

    /// Sets a callback to be called whenever the content is modified.
    ///
    /// Chainable variant. See [`set_on_edit`](#method.set_on_edit).
    pub fn on_edit<F>(self, callback: F) -> Self
        where F: Fn(&mut Cursive, &str, usize) + 'static
    {
        self.with(|v| v.set_on_edit(callback))
    }

    /// Sets a mutable callback to be called when `<Enter>` is pressed.
    ///
    /// `callback` will be given the content of the view.
    ///
    /// *Warning*: this callback cannot be called recursively. If you somehow
    /// trigger this callback again in the given closure, it will be ignored.
    ///
    /// If you don't need a mutable closure but want the possibility of
    /// recursive calls, see [`set_on_submit`](#method.set_on_submit).
    pub fn set_on_submit_mut<F>(&mut self, callback: F)
        where F: FnMut(&mut Cursive, &str) + 'static
    {
        // TODO: don't duplicate all those methods.
        // Instead, have some generic function immutify()
        // or something that wraps a FnMut closure.
        let callback = RefCell::new(callback);
        self.set_on_submit(move |s, text| if let Ok(mut f) =
            callback.try_borrow_mut() {
            (&mut *f)(s, text);
        });
    }

    /// Sets a callback to be called when `<Enter>` is pressed.
    ///
    /// `callback` will be given the content of the view.
    ///
    /// This callback can safely trigger itself recursively if needed
    /// (for instance if you call `on_event` on this view from the callback).
    ///
    /// If you need a mutable closure and don't care about the recursive
    /// aspect, see [`set_on_submit_mut`](#method.set_on_submit_mut).
    pub fn set_on_submit<F>(&mut self, callback: F)
        where F: Fn(&mut Cursive, &str) + 'static
    {
        self.on_submit = Some(Rc::new(callback));
    }

    /// Sets a mutable callback to be called when `<Enter>` is pressed.
    ///
    /// Chainable variant.
    pub fn on_submit_mut<F>(self, callback: F) -> Self
        where F: FnMut(&mut Cursive, &str) + 'static
    {
        self.with(|v| v.set_on_submit_mut(callback))
    }

    /// Sets a callback to be called when `<Enter>` is pressed.
    ///
    /// Chainable variant.
    pub fn on_submit<F>(self, callback: F) -> Self
        where F: Fn(&mut Cursive, &str) + 'static
    {
        self.with(|v| v.set_on_submit(callback))
    }

    /// Enable or disable this view.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Returns `true` if this view is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Replace the entire content of the view with the given one.
    pub fn set_content<S: Into<String>>(&mut self, content: S) {
        let content = content.into();
        let len = content.len();

        self.content = Rc::new(content);
        self.offset = 0;
        self.set_cursor(len);
    }

    /// Get the current text.
    pub fn get_content(&self) -> Rc<String> {
        self.content.clone()
    }

    /// Sets the current content to the given value.
    ///
    /// Convenient chainable method.
    pub fn content<S: Into<String>>(mut self, content: S) -> Self {
        self.set_content(content);
        self
    }

    /// Sets the cursor position.
    pub fn set_cursor(&mut self, cursor: usize) {
        self.cursor = cursor;

        self.keep_cursor_in_view();
    }

    /// Insert `ch` at the current cursor position.
    pub fn insert(&mut self, ch: char) {
        // `make_mut` applies copy-on-write
        // It means it'll just return a ref if no one else has a ref,
        // and it will clone it into `self.content` otherwise.
        Rc::make_mut(&mut self.content).insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    /// Remove the character at the current cursor position.
    pub fn remove(&mut self, len: usize) {
        let start = self.cursor;
        let end = self.cursor + len;
        for _ in Rc::make_mut(&mut self.content).drain(start..end) {}
    }

    fn keep_cursor_in_view(&mut self) {
        // keep cursor in [offset, offset+last_length] by changing offset
        // so keep offset in [last_length-cursor,cursor]
        // Also call this on resize,
        // but right now it is an event like any other
        if self.cursor < self.offset {
            self.offset = self.cursor;
        } else {
            // So we're against the right wall.
            // Let's find how much space will be taken by the selection
            // (either a char, or _)
            let c_len = self.content[self.cursor..]
                .graphemes(true)
                .map(|g| g.width())
                .next()
                .unwrap_or(1);
            if c_len > self.last_length {
                // Weird - no available space?
                return;
            }
            // Now, we have to fit self.content[..self.cursor]
            // into self.last_length - c_len.
            let available = self.last_length - c_len;
            // Look at the content before the cursor (we will print its tail).
            // From the end, count the length until we reach `available`.
            // Then sum the byte lengths.
            let suffix_length = simple_suffix(&self.content[self.offset..
                                               self.cursor],
                                              available)
                .length;
            self.offset = self.cursor - suffix_length;
            // Make sure the cursor is in view
            assert!(self.cursor >= self.offset);

        }

        // If we have too much space
        if self.content[self.offset..].width() < self.last_length {
            let suffix_length =
                simple_suffix(&self.content, self.last_length - 1).length;
            self.offset = self.content.len() - suffix_length;
        }
    }
}

/// Returns a `&str` with `length` characters `*`.
///
/// Only works for small `length` (1 or 2).
/// Best used for single character replacement.
fn make_small_stars(length: usize) -> &'static str {
    &"****"[..length]
}

impl View for EditView {
    fn draw(&self, printer: &Printer) {
        assert_eq!(printer.size.x, self.last_length,
                "Was promised {}, received {}",
                self.last_length,
                printer.size.x);

        let width = self.content.width();
        printer.with_color(ColorStyle::Secondary, |printer| {
            let effect = if self.enabled {
                Effect::Reverse
            } else {
                Effect::Simple
            };
            printer.with_effect(effect, |printer| {
                if width < self.last_length {
                    // No problem, everything fits.
                    if self.secret {
                        printer.print_hline((0, 0), width, "*");
                    } else {
                        printer.print((0, 0), &self.content);
                    }
                    printer.print_hline((width, 0),
                                        printer.size.x - width,
                                        self.filler.to_string().as_str());
                } else {
                    let content = &self.content[self.offset..];
                    let display_bytes = content.graphemes(true)
                        .scan(0, |w, g| {
                            *w += g.width();
                            if *w > self.last_length { None } else { Some(g) }
                        })
                        .map(|g| g.len())
                        .fold(0, |a, b| a + b);

                    let content = &content[..display_bytes];
                    let width = content.width();

                    if self.secret {
                        printer.print_hline((0, 0), width, "*");
                    } else {
                        printer.print((0, 0), content);
                    }

                    if width < self.last_length {
                        printer.print_hline((width, 0),
                                            self.last_length - width,
                                            self.filler.to_string().as_str());
                    }
                }
            });

            // Now print cursor
            if printer.focused {
                let c: &str = if self.cursor == self.content.len() {
                    "_"
                } else {
                    // Get the char from the string... Is it so hard?
                    let selected = self.content[self.cursor..]
                        .graphemes(true)
                        .next()
                        .expect(&format!("Found no char at cursor {} in {}",
                                         self.cursor,
                                         &self.content));
                    if self.secret {
                        make_small_stars(selected.width())
                    } else {
                        selected
                    }
                };
                let offset = self.content[self.offset..self.cursor].width();
                printer.print((offset, 0), c);
            }
        });
    }

    fn layout(&mut self, size: Vec2) {
        self.last_length = size.x;
        // println_stderr!("Promised: {}", size.x);
    }

    fn take_focus(&mut self, _: Direction) -> bool {
        self.enabled
    }

    fn on_event(&mut self, event: Event) -> EventResult {

        match event {
            Event::Char(ch) => self.insert(ch),
            // TODO: handle ctrl-key?
            Event::Key(Key::Home) => self.cursor = 0,
            Event::Key(Key::End) => self.cursor = self.content.len(),
            Event::Key(Key::Left) if self.cursor > 0 => {
                let len = self.content[..self.cursor]
                    .graphemes(true)
                    .last()
                    .unwrap()
                    .len();
                self.cursor -= len;
            }
            Event::Key(Key::Right) if self.cursor < self.content.len() => {
                let len = self.content[self.cursor..]
                    .graphemes(true)
                    .next()
                    .unwrap()
                    .len();
                self.cursor += len;
            }
            Event::Key(Key::Backspace) if self.cursor > 0 => {
                let len = self.content[..self.cursor]
                    .graphemes(true)
                    .last()
                    .unwrap()
                    .len();
                self.cursor -= len;
                self.remove(len);
            }
            Event::Key(Key::Del) if self.cursor < self.content.len() => {
                let len = self.content[self.cursor..]
                    .graphemes(true)
                    .next()
                    .unwrap()
                    .len();
                self.remove(len);
            }
            Event::Key(Key::Enter) if self.on_submit.is_some() => {
                let cb = self.on_submit.clone().unwrap();
                let content = self.content.clone();
                return EventResult::with_cb(move |s| { cb(s, &content); });
            }
            _ => return EventResult::Ignored,
        }

        self.keep_cursor_in_view();

        let cb = self.on_edit.clone().map(|cb| {

            // Get a new Rc on the content
            let content = self.content.clone();
            let cursor = self.cursor;

            Callback::from_fn(move |s| { cb(s, &content, cursor); })
        });
        EventResult::Consumed(cb)
    }
}
