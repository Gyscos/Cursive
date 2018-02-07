extern crate cursive;

use cursive::Cursive;
use cursive::view::Position;
use cursive::views::LayerPosition;
use cursive::views::TextView;

/// Moves top layer by the specified amount
fn move_top(c: &mut Cursive, x_in: isize, y_in: isize) {
    // Step 1. Get the current position of the layer.
    let s = c.screen_mut();
    let l = LayerPosition::FromFront(0);

    let (x, y) = s.offset().pair();

    // Step 2. add the specifed amount
    // (unsigned math in Rust is a mess.)
    let x = if x_in < 0 {
        x - (-x_in) as usize
    } else {
        x + x_in as usize
    };
    let y = if y_in < 0 {
        y - (-y_in) as usize
    } else {
        y + y_in as usize
    };

    // convert the new x and y into a position
    let p = Position::absolute((x, y));

    // Step 3. Apply the new position
    s.reposition_layer(l, p);
}

fn main() {
    let mut siv = Cursive::new();
    siv.set_fps(60);

    // We can quit by pressing `q`
    siv.add_global_callback('q', Cursive::quit);
    // Next Gen FPS Controls.
    siv.add_global_callback('w', |s| move_top(s, 0, -1));
    siv.add_global_callback('a', |s| move_top(s, -1, 0));
    siv.add_global_callback('s', |s| move_top(s, 0, 1));
    siv.add_global_callback('d', |s| move_top(s, 1, 0));

    // Add window to fly around.
    siv.add_layer(TextView::new(
        "Press w,a,s,d to move the window.\n\
         Press q to quit the application.",
    ));

    // Run the event loop
    siv.run();
}
