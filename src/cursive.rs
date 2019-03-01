use std::any::Any;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use crossbeam_channel::{self, Receiver, Sender};

use crate::backend;
use crate::direction;
use crate::event::{Callback, Event, EventResult};
use crate::printer::Printer;
use crate::theme;
use crate::vec::Vec2;
use crate::view::{self, Finder, IntoBoxedView, Position, View};
use crate::views::{self, LayerPosition};

static DEBUG_VIEW_ID: &'static str = "_cursive_debug_view";

/// Central part of the cursive library.
///
/// It initializes ncurses on creation and cleans up on drop.
/// To use it, you should populate it with views, layouts and callbacks,
/// then start the event loop with run().
///
/// It uses a list of screen, with one screen active at a time.
pub struct Cursive {
    theme: theme::Theme,
    screens: Vec<views::StackView>,
    global_callbacks: HashMap<Event, Vec<Callback>>,
    menubar: views::Menubar,

    // Last layer sizes of the stack view.
    // If it changed, clear the screen.
    last_sizes: Vec<Vec2>,

    autorefresh: bool,

    active_screen: ScreenId,

    running: bool,

    backend: Box<dyn backend::Backend>,

    cb_source: Receiver<Box<dyn CbFunc>>,
    cb_sink: Sender<Box<dyn CbFunc>>,
}

/// Identifies a screen in the cursive root.
pub type ScreenId = usize;

/// Convenient alias to the result of `Cursive::cb_sink`.
pub type CbSink = Sender<Box<dyn CbFunc>>;

/// Asynchronous callback function trait.
///
/// Every `FnOnce(&mut Cursive) -> () + Send` automatically
/// implements this.
///
/// This is a workaround only because `Box<FnOnce()>` is not
/// working and `FnBox` is unstable.
pub trait CbFunc: Send {
    /// Calls the function.
    fn call_box(self: Box<Self>, _: &mut Cursive);
}

impl<F: FnOnce(&mut Cursive) -> () + Send> CbFunc for F {
    fn call_box(self: Box<Self>, siv: &mut Cursive) {
        (*self)(siv)
    }
}

#[cfg(feature = "termion-backend")]
impl Default for Cursive {
    fn default() -> Self {
        Self::termion()
    }
}

#[cfg(all(not(feature = "termion-backend"), feature = "pancurses-backend"))]
impl Default for Cursive {
    fn default() -> Self {
        Self::pancurses()
    }
}

#[cfg(all(
    not(feature = "termion-backend"),
    not(feature = "pancurses-backend"),
    feature = "blt-backend"
))]
impl Default for Cursive {
    fn default() -> Self {
        Self::blt()
    }
}

#[cfg(all(
    not(feature = "termion-backend"),
    not(feature = "pancurses-backend"),
    not(feature = "blt-backend"),
    feature = "ncurses-backend"
))]
impl Default for Cursive {
    fn default() -> Self {
        Self::ncurses()
    }
}

impl Cursive {
    /// Creates a new Cursive root, and initialize the back-end.
    ///
    /// * If you just want a cursive instance, use `Cursive::default()`.
    /// * If you want a specific backend, then:
    ///   * `Cursive::ncurses()` if the `ncurses-backend` feature is enabled (it is by default).
    ///   * `Cursive::pancurses()` if the `pancurses-backend` feature is enabled.
    ///   * `Cursive::termion()` if the `termion-backend` feature is enabled.
    ///   * `Cursive::blt()` if the `blt-backend` feature is enabled.
    ///   * `Cursive::dummy()` for a dummy backend, mostly useful for tests.
    /// * If you want to use a third-party backend, then `Cursive::new` is indeed the way to go:
    ///   * `Cursive::new(bring::your::own::Backend::new)`
    ///
    /// Examples:
    ///
    /// ```rust,no_run
    /// # use cursive::{Cursive, backend};
    /// let siv = Cursive::new(backend::dummy::Backend::init); // equivalent to Cursive::dummy()
    /// ```
    pub fn new<F>(backend_init: F) -> Self
    where
        F: FnOnce() -> Box<dyn backend::Backend>,
    {
        let theme = theme::load_default();

        let (cb_sink, cb_source) = crossbeam_channel::unbounded();

        let backend = backend_init();

        Cursive {
            autorefresh: false,
            theme,
            screens: vec![views::StackView::new()],
            last_sizes: Vec::new(),
            global_callbacks: HashMap::new(),
            menubar: views::Menubar::new(),
            active_screen: 0,
            running: true,
            cb_source,
            cb_sink,
            backend,
        }
    }

    /// Creates a new Cursive root using a ncurses backend.
    #[cfg(feature = "ncurses-backend")]
    pub fn ncurses() -> Self {
        Self::new(backend::curses::n::Backend::init)
    }

    /// Creates a new Cursive root using a pancurses backend.
    #[cfg(feature = "pancurses-backend")]
    pub fn pancurses() -> Self {
        Self::new(backend::curses::pan::Backend::init)
    }

    /// Creates a new Cursive root using a termion backend.
    #[cfg(feature = "termion-backend")]
    pub fn termion() -> Self {
        Self::new(backend::termion::Backend::init)
    }

    /// Creates a new Cursive root using a bear-lib-terminal backend.
    #[cfg(feature = "blt-backend")]
    pub fn blt() -> Self {
        Self::new(backend::blt::Backend::init)
    }

    /// Creates a new Cursive root using a dummy backend.
    ///
    /// Nothing will be output. This is mostly here for tests.
    pub fn dummy() -> Self {
        Self::new(backend::dummy::Backend::init)
    }

    /// Show the debug console.
    ///
    /// Currently, this will show logs if [`::logger::init()`] was called.
    pub fn show_debug_console(&mut self) {
        self.add_layer(
            views::Dialog::around(views::ScrollView::new(views::IdView::new(
                DEBUG_VIEW_ID,
                views::DebugView::new(),
            )))
            .title("Debug console"),
        );
    }

    /// Show the debug console, or hide it if it's already visible.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use cursive::Cursive;
    /// # let mut siv = Cursive::dummy();
    /// siv.add_global_callback('~', Cursive::toggle_debug_console);
    /// ```
    pub fn toggle_debug_console(&mut self) {
        if let Some(pos) = self.screen_mut().find_layer_from_id(DEBUG_VIEW_ID)
        {
            self.screen_mut().remove_layer(pos);
        } else {
            self.show_debug_console();
        }
    }

    /// Returns a sink for asynchronous callbacks.
    ///
    /// Returns the sender part of a channel, that allows to send
    /// callbacks to `self` from other threads.
    ///
    /// Callbacks will be executed in the order
    /// of arrival on the next event cycle.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate cursive;
    /// # use cursive::*;
    /// # fn main() {
    /// let mut siv = Cursive::dummy();
    ///
    /// // quit() will be called during the next event cycle
    /// siv.cb_sink().send(Box::new(|s: &mut Cursive| s.quit())).unwrap();
    /// # }
    /// ```
    pub fn cb_sink(&self) -> &CbSink {
        &self.cb_sink
    }

    /// Selects the menubar.
    pub fn select_menubar(&mut self) {
        self.menubar.take_focus(direction::Direction::none());
    }

    /// Sets the menubar autohide feature.
    ///
    /// * When enabled (default), the menu is only visible when selected.
    /// * When disabled, the menu is always visible and reserves the top row.
    pub fn set_autohide_menu(&mut self, autohide: bool) {
        self.menubar.autohide = autohide;
    }

    /// Access the menu tree used by the menubar.
    ///
    /// This allows to add menu items to the menubar.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate cursive;
    /// #
    /// # use cursive::{Cursive, event};
    /// # use cursive::views::{Dialog};
    /// # use cursive::traits::*;
    /// # use cursive::menu::*;
    /// #
    /// # fn main() {
    /// let mut siv = Cursive::dummy();
    ///
    /// siv.menubar()
    ///    .add_subtree("File",
    ///         MenuTree::new()
    ///             .leaf("New", |s| s.add_layer(Dialog::info("New file!")))
    ///             .subtree("Recent", MenuTree::new().with(|tree| {
    ///                 for i in 1..100 {
    ///                     tree.add_leaf(format!("Item {}", i), |_| ())
    ///                 }
    ///             }))
    ///             .delimiter()
    ///             .with(|tree| {
    ///                 for i in 1..10 {
    ///                     tree.add_leaf(format!("Option {}", i), |_| ());
    ///                 }
    ///             })
    ///             .delimiter()
    ///             .leaf("Quit", |s| s.quit()))
    ///    .add_subtree("Help",
    ///         MenuTree::new()
    ///             .subtree("Help",
    ///                      MenuTree::new()
    ///                          .leaf("General", |s| {
    ///                              s.add_layer(Dialog::info("Help message!"))
    ///                          })
    ///                          .leaf("Online", |s| {
    ///                              s.add_layer(Dialog::info("Online help?"))
    ///                          }))
    ///             .leaf("About",
    ///                   |s| s.add_layer(Dialog::info("Cursive v0.0.0"))));
    ///
    /// siv.add_global_callback(event::Key::Esc, |s| s.select_menubar());
    /// # }
    /// ```
    pub fn menubar(&mut self) -> &mut views::Menubar {
        &mut self.menubar
    }

    /// Returns the currently used theme.
    pub fn current_theme(&self) -> &theme::Theme {
        &self.theme
    }

    /// Sets the current theme.
    pub fn set_theme(&mut self, theme: theme::Theme) {
        self.theme = theme;
        self.clear();
    }

    /// Clears the screen.
    ///
    /// Users rarely have to call this directly.
    pub fn clear(&self) {
        self.backend
            .clear(self.theme.palette[theme::PaletteColor::Background]);
    }

    /// Loads a theme from the given file.
    ///
    /// `filename` must point to a valid toml file.
    pub fn load_theme_file<P: AsRef<Path>>(
        &mut self, filename: P,
    ) -> Result<(), theme::Error> {
        theme::load_theme_file(filename).map(|theme| self.set_theme(theme))
    }

    /// Loads a theme from the given string content.
    ///
    /// Content must be valid toml.
    pub fn load_toml(&mut self, content: &str) -> Result<(), theme::Error> {
        theme::load_toml(content).map(|theme| self.set_theme(theme))
    }

    /// Enables or disables automatic refresh of the screen.
    ///
    /// When on, regularly redraws everything, even when no input is given.
    pub fn set_autorefresh(&mut self, autorefresh: bool) {
        self.autorefresh = autorefresh;
    }

    /// Returns a reference to the currently active screen.
    pub fn screen(&self) -> &views::StackView {
        let id = self.active_screen;
        &self.screens[id]
    }

    /// Returns a mutable reference to the currently active screen.
    pub fn screen_mut(&mut self) -> &mut views::StackView {
        let id = self.active_screen;
        &mut self.screens[id]
    }

    /// Returns the id of the currently active screen.
    pub fn active_screen(&self) -> ScreenId {
        self.active_screen
    }

    /// Adds a new screen, and returns its ID.
    pub fn add_screen(&mut self) -> ScreenId {
        let res = self.screens.len();
        self.screens.push(views::StackView::new());
        res
    }

    /// Convenient method to create a new screen, and set it as active.
    pub fn add_active_screen(&mut self) -> ScreenId {
        let res = self.add_screen();
        self.set_screen(res);
        res
    }

    /// Sets the active screen. Panics if no such screen exist.
    pub fn set_screen(&mut self, screen_id: ScreenId) {
        if screen_id >= self.screens.len() {
            panic!(
                "Tried to set an invalid screen ID: {}, but only {} \
                 screens present.",
                screen_id,
                self.screens.len()
            );
        }
        self.active_screen = screen_id;
    }

    /// Tries to find the view pointed to by the given selector.
    ///
    /// Runs a closure on the view once it's found, and return the
    /// result.
    ///
    /// If the view is not found, or if it is not of the asked type,
    /// returns None.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate cursive;
    /// # use cursive::{Cursive, views, view};
    /// # use cursive::traits::*;
    /// # fn main() {
    /// fn main() {
    ///     let mut siv = Cursive::dummy();
    ///
    ///     siv.add_layer(views::TextView::new("Text #1").with_id("text"));
    ///
    ///     siv.add_global_callback('p', |s| {
    ///         s.call_on(
    ///             &view::Selector::Id("text"),
    ///             |view: &mut views::TextView| {
    ///                 view.set_content("Text #2");
    ///             },
    ///         );
    ///     });
    ///
    /// }
    /// # }
    /// ```
    pub fn call_on<V, F, R>(
        &mut self, sel: &view::Selector<'_>, callback: F,
    ) -> Option<R>
    where
        V: View + Any,
        F: FnOnce(&mut V) -> R,
    {
        self.screen_mut().call_on(sel, callback)
    }

    /// Tries to find the view identified by the given id.
    ///
    /// Convenient method to use `call_on` with a `view::Selector::Id`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate cursive;
    /// # use cursive::{Cursive, views};
    /// # use cursive::traits::*;
    /// # fn main() {
    /// let mut siv = Cursive::dummy();
    ///
    /// siv.add_layer(views::TextView::new("Text #1")
    ///                               .with_id("text"));
    ///
    /// siv.add_global_callback('p', |s| {
    ///     s.call_on_id("text", |view: &mut views::TextView| {
    ///         view.set_content("Text #2");
    ///     });
    /// });
    /// # }
    /// ```
    pub fn call_on_id<V, F, R>(&mut self, id: &str, callback: F) -> Option<R>
    where
        V: View + Any,
        F: FnOnce(&mut V) -> R,
    {
        self.call_on(&view::Selector::Id(id), callback)
    }

    /// Convenient method to find a view wrapped in [`IdView`].
    ///
    /// This looks for a `IdView<V>` with the given ID, and return
    /// a [`ViewRef`] to the wrapped view. The `ViewRef` implements
    /// `DerefMut<Target=T>`, so you can treat it just like a `&mut T`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use cursive::Cursive;
    /// # use cursive::views::{TextView, ViewRef};
    /// # let mut siv = Cursive::dummy();
    /// use cursive::traits::Identifiable;
    ///
    /// siv.add_layer(TextView::new("foo").with_id("id"));
    ///
    /// // Could be called in a callback
    /// let mut view: ViewRef<TextView> = siv.find_id("id").unwrap();
    /// view.set_content("bar");
    /// ```
    ///
    /// Note that you must specify the exact type for the view you're after; for example, using the
    /// wrong item type in a `SelectView` will not find anything:
    ///
    /// ```rust
    /// # use cursive::Cursive;
    /// # use cursive::views::{SelectView};
    /// # let mut siv = Cursive::dummy();
    /// use cursive::traits::Identifiable;
    ///
    /// let select = SelectView::new().item("zero", 0u32).item("one", 1u32);
    /// siv.add_layer(select.with_id("select"));
    ///
    /// // Specifying a wrong type will not return anything.
    /// assert!(siv.find_id::<SelectView<String>>("select").is_none());
    ///
    /// // Omitting the type will use the default type, in this case `String`.
    /// assert!(siv.find_id::<SelectView>("select").is_none());
    ///
    /// // But with the correct type, it works fine.
    /// assert!(siv.find_id::<SelectView<u32>>("select").is_some());
    /// ```
    ///
    /// [`IdView`]: views/struct.IdView.html
    /// [`ViewRef`]: views/type.ViewRef.html
    pub fn find_id<V>(&mut self, id: &str) -> Option<views::ViewRef<V>>
    where
        V: View + Any,
    {
        self.call_on_id(id, views::IdView::<V>::get_mut)
    }

    /// Moves the focus to the view identified by `id`.
    ///
    /// Convenient method to call `focus` with a `view::Selector::Id`.
    pub fn focus_id(&mut self, id: &str) -> Result<(), ()> {
        self.focus(&view::Selector::Id(id))
    }

    /// Moves the focus to the view identified by `sel`.
    pub fn focus(&mut self, sel: &view::Selector<'_>) -> Result<(), ()> {
        self.screen_mut().focus_view(sel)
    }

    /// Adds a global callback.
    ///
    /// Will be triggered on the given key press when no view catches it.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate cursive;
    /// # use cursive::*;
    /// # fn main() {
    /// let mut siv = Cursive::dummy();
    ///
    /// siv.add_global_callback('q', |s| s.quit());
    /// # }
    /// ```
    pub fn add_global_callback<F, E: Into<Event>>(&mut self, event: E, cb: F)
    where
        F: FnMut(&mut Cursive) + 'static,
    {
        self.global_callbacks
            .entry(event.into())
            .or_insert_with(Vec::new)
            .push(Callback::from_fn_mut(cb));
    }

    /// Removes any callback tied to the given event.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate cursive;
    /// # use cursive::*;
    /// # fn main() {
    /// let mut siv = Cursive::dummy();
    ///
    /// siv.add_global_callback('q', |s| s.quit());
    /// siv.clear_global_callbacks('q');
    /// # }
    /// ```
    pub fn clear_global_callbacks<E>(&mut self, event: E)
    where
        E: Into<Event>,
    {
        let event = event.into();
        self.global_callbacks.remove(&event);
    }

    /// Add a layer to the current screen.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate cursive;
    /// # use cursive::*;
    /// # fn main() {
    /// let mut siv = Cursive::dummy();
    ///
    /// siv.add_layer(views::TextView::new("Hello world!"));
    /// # }
    /// ```
    pub fn add_layer<T>(&mut self, view: T)
    where
        T: IntoBoxedView,
    {
        self.screen_mut().add_layer(view);
    }

    /// Adds a new full-screen layer to the current screen.
    ///
    /// Fullscreen layers have no shadow.
    pub fn add_fullscreen_layer<T>(&mut self, view: T)
    where
        T: IntoBoxedView,
    {
        self.screen_mut().add_fullscreen_layer(view);
    }

    /// Convenient method to remove a layer from the current screen.
    pub fn pop_layer(&mut self) -> Option<Box<dyn View>> {
        self.screen_mut().pop_layer()
    }

    /// Convenient stub forwarding layer repositioning.
    pub fn reposition_layer(
        &mut self, layer: LayerPosition, position: Position,
    ) {
        self.screen_mut().reposition_layer(layer, position);
    }

    // Handles a key event when it was ignored by the current view
    fn on_ignored_event(&mut self, event: Event) {
        let cb_list = match self.global_callbacks.get(&event) {
            None => return,
            Some(cb_list) => cb_list.clone(),
        };
        // Not from a view, so no viewpath here
        for cb in cb_list {
            cb(self);
        }
    }

    /// Processes an event.
    ///
    /// * If the menubar is active, it will be handled the event.
    /// * The view tree will be handled the event.
    /// * If ignored, global_callbacks will be checked for this event.
    pub fn on_event(&mut self, event: Event) {
        if event == Event::Exit {
            self.quit();
        }

        if event == Event::WindowResize {
            self.clear();
        }

        if let Event::Mouse {
            event, position, ..
        } = event
        {
            if event.grabs_focus()
                && !self.menubar.autohide
                && !self.menubar.has_submenu()
                && position.y == 0
            {
                self.select_menubar();
            }
        }

        // Event dispatch order:
        // * Focused element:
        //     * Menubar (if active)
        //     * Current screen (top layer)
        // * Global callbacks
        if self.menubar.receive_events() {
            self.menubar.on_event(event).process(self);
        } else {
            let offset = if self.menubar.autohide { 0 } else { 1 };
            match self.screen_mut().on_event(event.relativized((0, offset))) {
                // If the event was ignored,
                // it is our turn to play with it.
                EventResult::Ignored => self.on_ignored_event(event),
                EventResult::Consumed(None) => (),
                EventResult::Consumed(Some(cb)) => cb(self),
            }
        }
    }

    /// Returns the size of the screen, in characters.
    pub fn screen_size(&self) -> Vec2 {
        self.backend.screen_size()
    }

    fn layout(&mut self) {
        let size = self.screen_size();
        let offset = if self.menubar.autohide { 0 } else { 1 };
        let size = size.saturating_sub((0, offset));
        self.screen_mut().layout(size);
    }

    fn draw(&mut self) {
        let sizes = self.screen().layer_sizes();
        if self.last_sizes != sizes {
            self.clear();
            self.last_sizes = sizes;
        }

        let printer =
            Printer::new(self.screen_size(), &self.theme, &*self.backend);

        let selected = self.menubar.receive_events();

        // Print the stackview background before the menubar
        let offset = if self.menubar.autohide { 0 } else { 1 };
        let id = self.active_screen;
        let sv_printer = printer.offset((0, offset)).focused(!selected);

        self.screens[id].draw_bg(&sv_printer);

        // Draw the currently active screen
        // If the menubar is active, nothing else can be.
        // Draw the menubar?
        if self.menubar.visible() {
            let printer = printer.focused(self.menubar.receive_events());
            self.menubar.draw(&printer);
        }

        // finally draw stackview layers
        // using variables from above
        self.screens[id].draw_fg(&sv_printer);
    }

    /// Returns `true` until [`quit(&mut self)`] is called.
    ///
    /// [`quit(&mut self)`]: #method.quit
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Runs the event loop.
    ///
    /// It will wait for user input (key presses)
    /// and trigger callbacks accordingly.
    ///
    /// Calls [`step(&mut self)`] until [`quit(&mut self)`] is called.
    ///
    /// After this function returns, you can call
    /// it again and it will start a new loop.
    ///
    /// [`step(&mut self)`]: #method.step
    /// [`quit(&mut self)`]: #method.quit
    pub fn run(&mut self) {
        self.running = true;

        self.refresh();

        // And the big event loop begins!
        while self.running {
            self.step();
        }
    }

    /// Performs a single step from the event loop.
    ///
    /// Useful if you need tighter control on the event loop.
    /// Otherwise, [`run(&mut self)`] might be more convenient.
    ///
    /// [`run(&mut self)`]: #method.run
    pub fn step(&mut self) {
        let mut boring = true;

        // First, handle all available input
        while let Some(event) = self.backend.poll_event() {
            boring = false;
            self.on_event(event);

            if !self.running {
                return;
            }
        }

        // Then, handle any available callback
        while let Ok(cb) = self.cb_source.try_recv() {
            boring = false;
            cb.call_box(self);

            if !self.running {
                return;
            }
        }

        if self.autorefresh || !boring {
            // Only re-draw if nothing happened.
            self.refresh();
        }

        if boring {
            // Otherwise, sleep some more
            std::thread::sleep(Duration::from_millis(30));
        }
    }

    /// Refresh the screen with the current view tree state.
    fn refresh(&mut self) {
        // Do we need to redraw everytime?
        // Probably, actually.
        // TODO: Do we need to re-layout everytime?
        self.layout();

        // TODO: Do we need to redraw every view every time?
        // (Is this getting repetitive? :p)
        self.draw();
        self.backend.refresh();
    }

    /// Stops the event loop.
    pub fn quit(&mut self) {
        self.running = false;
    }

    /// Does not do anything.
    pub fn noop(&mut self) {
        // foo
    }
}

impl Drop for Cursive {
    fn drop(&mut self) {
        self.backend.finish();
    }
}
