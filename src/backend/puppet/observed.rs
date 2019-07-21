//! Structs representing output of puppet backend
use enumset::EnumSet;
use std::ops::Index;
use std::ops::IndexMut;
use std::rc::Rc;
use std::string::ToString;
use crate::theme::ColorPair;
use crate::theme::Effect;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
use crate::Vec2;

/// Style of observed cell
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ObservedStyle {
    /// Colors: front and back
    pub colors: ColorPair,
    /// Effects enabled on observed cell
    pub effects: EnumSet<Effect>,
}

/// Contents of observed cell
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum GraphemePart {
    /// Represents begin of wide character
    Begin(String),
    /// Represents a cell that is filled with continuation of some character that begun in cell with lower x-index.
    Continuation,
}

impl GraphemePart {
    /// Returns true iff GraphemePart is Continuation
    pub fn is_continuation(&self) -> bool {
        match self {
            &GraphemePart::Continuation => true,
            _ => false,
        }
    }

    /// Returns Some(String) if GraphemePart is Begin(String), else None.
    pub fn as_option(&self) -> Option<&String> {
        match self {
            &GraphemePart::Begin(ref string) => Some(string),
            &GraphemePart::Continuation => None,
        }
    }

    /// Returns String if GraphemePart is Begin(String), panics otherwise.
    pub fn unwrap(&self) -> String {
        match self {
            &GraphemePart::Begin(ref s) => s.clone(),
            _ => panic!("unwrapping GraphemePart::Continuation"),
        }
    }
}

/// Represents a single cell of terminal.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ObservedCell {
    /// Absolute position
    pub pos: Vec2,
    /// Style
    pub style: Rc<ObservedStyle>,
    /// Part of grapheme - either it's beginning or continuation when character is multi-cell long.
    pub letter: GraphemePart,
}

impl ObservedCell {
    /// Constructor
    pub fn new(
        pos: Vec2,
        style: Rc<ObservedStyle>,
        letter: Option<String>,
    ) -> Self {
        let letter: GraphemePart = match letter {
            Some(s) => GraphemePart::Begin(s),
            None => GraphemePart::Continuation,
        };

        ObservedCell { pos, style, letter }
    }
}

/// Puppet backend output
///
/// Represents single frame.
#[derive(Debug, Clone)]
pub struct ObservedScreen {
    /// Size
    size: Vec2,
    /// Contents. Each cell can be set or empty.
    contents: Vec<Option<ObservedCell>>,
}

impl ObservedScreen {
    /// Creates empty ObservedScreen
    pub fn new(size: Vec2) -> Self {
        let contents: Vec<Option<ObservedCell>> = vec![None; size.x * size.y];

        ObservedScreen { size, contents }
    }

    fn flatten_index(&self, index: &Vec2) -> usize {
        assert!(index.x < self.size.x);
        assert!(index.y < self.size.y);

        index.y * self.size.x + index.x
    }

    fn unflatten_index(&self, index: usize) -> Vec2 {
        assert!(index < self.contents.len());

        Vec2::new(index / self.size.x, index % self.size.x)
    }

    /// Sets all cells to empty cells with given style
    pub fn clear(&mut self, style: &Rc<ObservedStyle>) {
        for idx in 0..self.contents.len() {
            self.contents[idx] = Some(ObservedCell::new(
                self.unflatten_index(idx),
                style.clone(),
                None,
            ))
        }
    }

    /// Size
    pub fn size(&self) -> Vec2 {
        self.size
    }

    /// Returns a rectangular subset of observed screen.
    pub fn piece(&self, min: Vec2, max: Vec2) -> ObservedPiece {
        ObservedPiece::new(self, min, max)
    }


    /// Returns occurences of given string pattern
    pub fn find_occurences<'a>(
        &'a self,
        pattern: &str,
    ) -> Vec<ObservedLine<'a>> {
        // TODO(njskalski): test for two-cell letters.
        // TODO(njskalski): fails with whitespaces like "\t".

        let mut hits: Vec<ObservedLine> = vec![];
        for y in self.min().y..self.max().y {
            'x: for x in self.min().x..self.max().x {
                // check candidate.

                if pattern.len() > self.size.x - x {
                    continue;
                }

                let mut pattern_cursor: usize = 0;
                let mut pos_cursor: usize = 0;

                loop {
                    let pattern_symbol = pattern
                        .graphemes(true)
                        .skip(pattern_cursor)
                        .next()
                        .unwrap_or_else(|| {
                            panic!(
                                "Found no char at cursor {} in {}",
                                pattern_cursor, &pattern
                            )
                        });

                    let pos_it = Vec2::new(x + pos_cursor, y);

                    let found_symbol: Option<&String> =
                        if let Some(ref cell) = self[&pos_it] {
                            cell.letter.as_option()
                        } else {
                            None
                        };

                    match found_symbol {
                        Some(screen_symbol) => {
                            if pattern_symbol == screen_symbol {
                                pattern_cursor += 1;
                                pos_cursor += screen_symbol.width();
                            } else {
                                continue 'x;
                            }
                        }
                        None => {
                            if pattern_symbol == " " {
                                pattern_cursor += 1;
                                pos_cursor += 1;
                            } else {
                                continue 'x;
                            }
                        }
                    };

                    if pattern_cursor == pattern.graphemes(true).count() {
                        break;
                    };
                }

                if pattern_cursor == pattern.graphemes(true).count() {
                    hits.push(ObservedLine::new(
                        self,
                        Vec2::new(x, y),
                        pos_cursor,
                    ));
                }
            }
        }
        hits
    }
}

/// Represents rectangular piece of observed screen (Puppet backend output)
pub trait ObservedPieceInterface {

    /// Minimums of coordinates
    fn min(&self) -> Vec2;
    /// Maximums of coordinates
    fn max(&self) -> Vec2;

    /// Reference of ObservablePiece this one is a subsection of or Self
    fn parent(&self) -> &ObservedScreen;

    /// Size of piece
    fn size(&self) -> Vec2 {
        self.max() - self.min()
    }

    /// Returns a string representation of consecutive lines of this piece.
    fn as_strings(&self) -> Vec<String> {
        let mut v: Vec<String> = vec![];
        for y in self.min().y..self.max().y {
            let mut s = String::new();
            for x in self.min().x..self.max().x {
                match &self.parent()[&Vec2::new(x, y)] {
                    None => s.push(' '),
                    Some(cell) => match &cell.letter {
                        GraphemePart::Begin(lex) => s.push_str(&lex),
                        _ => {}
                    },
                }
            }
            v.push(s);
        }
        v
    }

    /// Returns expanded sibling of this piece
    ///
    /// Asserts if request can be satisfied.
    fn expanded(&self, up_left: Vec2, down_right: Vec2) -> ObservedPiece {
        assert!(self.min().x >= up_left.x);
        assert!(self.min().y >= up_left.y);
        assert!(self.max().x + down_right.x <= self.parent().size.x);
        assert!(self.max().y + down_right.y <= self.parent().size.y);

        ObservedPiece::new(
            self.parent(),
            self.min() - up_left,
            self.max() + down_right,
        )
    }
}

/// Represents a piece or whole of observed screen.
pub struct ObservedPiece<'a> {
    min: Vec2,
    max: Vec2,
    parent: &'a ObservedScreen,
}

impl<'a> ObservedPiece<'a> {
    fn new(parent: &'a ObservedScreen, min: Vec2, max: Vec2) -> Self {
        ObservedPiece { min, max, parent }
    }
}

impl ObservedPieceInterface for ObservedScreen {
    fn min(&self) -> Vec2 {
        Vec2::new(0, 0)
    }

    fn max(&self) -> Vec2 {
        self.size
    }

    fn parent(&self) -> &ObservedScreen {
        self
    }
}

impl<'a> ObservedPieceInterface for ObservedPiece<'a> {
    fn min(&self) -> Vec2 {
        self.min
    }

    fn max(&self) -> Vec2 {
        self.max
    }

    fn parent(&self) -> &ObservedScreen {
        self.parent
    }
}

/// Represents a single line of observed screen.
pub struct ObservedLine<'a> {
    line_start: Vec2,
    line_len: usize,
    parent: &'a ObservedScreen,
}

impl<'a> ObservedLine<'a> {
    fn new(
        parent: &'a ObservedScreen,
        line_start: Vec2,
        line_len: usize,
    ) -> Self {
        ObservedLine {
            parent,
            line_start,
            line_len,
        }
    }

    /// Returns the same line, but expanded.
    ///
    /// Asserts whether request can be satisfied
    pub fn expanded_line(&self, left: usize, right: usize) -> Self {
        assert!(left <= self.line_start.x);
        assert!(
            self.line_start.x + self.line_len + right <= self.parent.size.x
        );

        ObservedLine {
            parent: self.parent,
            line_start: Vec2::new(self.line_start.x - left, self.line_start.y),
            line_len: self.line_len + left + right,
        }
    }
}

impl<'a> ObservedPieceInterface for ObservedLine<'a> {
    fn min(&self) -> Vec2 {
        self.line_start
    }

    fn max(&self) -> Vec2 {
        self.line_start + Vec2::new(self.line_len, 1)
    }

    fn parent(&self) -> &ObservedScreen {
        self.parent
    }
}

impl<'a> ToString for ObservedLine<'a> {
    fn to_string(&self) -> String {
        self.as_strings().remove(0)
    }
}

impl Index<&Vec2> for ObservedPieceInterface {
    type Output = Option<ObservedCell>;

    fn index(&self, index: &Vec2) -> &Self::Output {
        assert!(self.max().x - self.min().x > index.x);
        assert!(self.max().y - self.min().y > index.y);

        let parent_index = self.min() + index;

        &self.parent()[&parent_index]
    }
}

impl Index<&Vec2> for ObservedScreen {
    type Output = Option<ObservedCell>;

    fn index(&self, index: &Vec2) -> &Self::Output {
        let idx = self.flatten_index(&index);
        &self.contents[idx]
    }
}

impl IndexMut<&Vec2> for ObservedScreen {
    fn index_mut(&mut self, index: &Vec2) -> &mut Option<ObservedCell> {
        let idx = self.flatten_index(&index);
        &mut self.contents[idx]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::puppet::Backend;
    use crate::backend::puppet::DEFAULT_OBSERVED_STYLE;
    use core::borrow::BorrowMut;
    use crate::event::Event;
    use std::time::Duration;
    use crate::view::*;
    use crate::views::TextArea;
    use crate::views::TextView;
    use crate::views::*;
    use crate::Cursive;

    /// Expecting fake_screen to be square, # will be replaced with blank.
    fn get_observed_screen(fake_screen: &Vec<&str>) -> ObservedScreen {
        let observed_style: Rc<ObservedStyle> =
            Rc::new(DEFAULT_OBSERVED_STYLE.clone());

        let height = fake_screen.len();
        let width = fake_screen[0].width();
        let size = Vec2::new(width, height);

        let mut os = ObservedScreen::new(size);

        for y in 0..fake_screen.len() {
            let mut x: usize = 0;
            for letter in fake_screen[y].graphemes(true) {
                let idx = os.flatten_index(&Vec2::new(x, y));
                os.contents[idx] = if letter == "#" {
                    None
                } else {
                    Some(ObservedCell::new(
                        Vec2::new(x, y),
                        observed_style.clone(),
                        Some(letter.to_owned()),
                    ))
                };

                x += letter.width();
            }
        }

        os
    }

    //    /// Expecting fake_screen to be square via Cursive draw
    //    fn get_observed_screen_via_draw(fake_screen : &Vec<&str>) -> ObservedScreen {
    //        let height = fake_screen.len();
    //        let width = fake_screen[0].graphemes(true).count();
    //        let size = Vec2::new(width, height);
    //
    //        let backend = Backend::init(Some(size));
    //        let sink = backend.stream();
    //        let input = backend.input();
    //
    //        let mut siv = Cursive::new(move || backend);
    //
    //        let mut ta = TextArea::new().content(fake_screen.join("")).fixed_size(size);
    //
    //        siv.add_fullscreen_layer(ta);
    //        let observed_style : Rc<ObservedStyle> = Rc::new(DEFAULT_OBSERVED_STYLE.clone());
    //        siv.quit();
    //        input.send(Some(Event::Refresh));
    //        siv.step();
    //
    //        let mut last_screen : Option<ObservedScreen> = None;
    //
    //        while let Ok(screen) = sink.recv_timeout(Duration::new(0,0)) {
    //            last_screen = Some(screen);
    //        }
    //
    //        last_screen.unwrap()
    //    }

    #[test]
    fn test_test() {
        let fake_screen: Vec<&'static str> =
            vec!["..hello***", "!!##$$$$$*", ".hello^^^^"];

        let os = get_observed_screen(&fake_screen);

        assert_eq!(
            os[&Vec2::new(0, 0)].as_ref().unwrap().letter.as_option(),
            Some(&".".to_owned())
        );
        assert_eq!(os[&Vec2::new(2, 1)], None);
    }

    #[test]
    fn find_occurrences_no_blanks() {
        let fake_screen: Vec<&'static str> =
            vec!["..hello***", "!!##$$$$$*", ".hello^^^^"];

        let os = get_observed_screen(&fake_screen);

        let hits = os.find_occurences("hello");

        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].size(), Vec2::new(5, 1));
        assert_eq!(hits[1].size(), Vec2::new(5, 1));

        assert_eq!(hits[0].to_string(), "hello");
        assert_eq!(hits[1].to_string(), "hello");

        assert_eq!(hits[0].min(), Vec2::new(2, 0));
        assert_eq!(hits[0].max(), Vec2::new(7, 1));

        assert_eq!(hits[1].min(), Vec2::new(1, 2));
        assert_eq!(hits[1].max(), Vec2::new(6, 3));
    }

    #[test]
    fn find_occurrences_some_blanks() {
        let fake_screen: Vec<&'static str> =
            vec!["__hello world_", "hello!world___", "___hello#world"];

        let os = get_observed_screen(&fake_screen);

        let hits = os.find_occurences("hello world");

        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].size(), Vec2::new(11, 1));
        assert_eq!(hits[1].size(), Vec2::new(11, 1));

        assert_eq!(hits[0].to_string(), "hello world");
        assert_eq!(hits[1].to_string(), "hello world");

        assert_eq!(hits[0].min(), Vec2::new(2, 0));
        assert_eq!(hits[0].max(), Vec2::new(13, 1));

        assert_eq!(hits[1].min(), Vec2::new(3, 2));
        assert_eq!(hits[1].max(), Vec2::new(14, 3));
    }

    #[test]
    fn test_expand_lines() {
        let fake_screen: Vec<&'static str> = vec!["abc hello#efg"];

        let os = get_observed_screen(&fake_screen);

        let hits = os.find_occurences("hello");

        assert_eq!(hits.len(), 1);
        let hit = hits.first().unwrap();
        assert_eq!(hit.size(), Vec2::new(5, 1));
        let expanded_left = hit.expanded_line(3, 0);
        assert_eq!(expanded_left.size(), Vec2::new(8, 1));
        assert_eq!(expanded_left.to_string(), "bc hello");

        let expanded_left = hit.expanded_line(4, 0);
        assert_eq!(expanded_left.size(), Vec2::new(9, 1));
        assert_eq!(expanded_left.to_string(), "abc hello");

        let expanded_right = hit.expanded_line(0, 2);
        assert_eq!(expanded_right.size(), Vec2::new(7, 1));
        assert_eq!(expanded_right.to_string(), "hello e");

        let expanded_right = hit.expanded_line(0, 4);
        assert_eq!(expanded_right.size(), Vec2::new(9, 1));
        assert_eq!(expanded_right.to_string(), "hello efg");
    }

    #[test]
    fn test_expand_lines_weird_symbol_1() {
        let fake_screen: Vec<&'static str> = vec!["abc ▸ <root>#efg"];

        let os = get_observed_screen(&fake_screen);

        let hits = os.find_occurences("root");

        assert_eq!(hits.len(), 1);
        let hit = hits.first().unwrap();
        assert_eq!(hit.size(), Vec2::new(4, 1));
        let expanded_left = hit.expanded_line(3, 0);
        assert_eq!(expanded_left.size(), Vec2::new(7, 1));
        assert_eq!(expanded_left.to_string(), "▸ <root");

        let expanded_left = hit.expanded_line(7, 0);
        assert_eq!(expanded_left.size(), Vec2::new(11, 1));
        assert_eq!(expanded_left.to_string(), "abc ▸ <root");

        let expanded_right = hit.expanded_line(0, 5);
        assert_eq!(expanded_right.size(), Vec2::new(9, 1));
        assert_eq!(expanded_right.to_string(), "root> efg");
    }

    #[test]
    fn test_expand_lines_weird_symbol_2() {
        let fake_screen: Vec<&'static str> = vec!["abc ▸ <root>#efg"];

        let os = get_observed_screen(&fake_screen);

        let hits = os.find_occurences("▸");

        assert_eq!(hits.len(), 1);
        let hit = hits.first().unwrap();
        assert_eq!(hit.size(), Vec2::new(1, 1));
        let expanded_left = hit.expanded_line(3, 0);
        assert_eq!(expanded_left.size(), Vec2::new(4, 1));
        assert_eq!(expanded_left.to_string(), "bc ▸");

        let expanded_right = hit.expanded_line(0, 9);
        assert_eq!(expanded_right.size(), Vec2::new(10, 1));
        assert_eq!(expanded_right.to_string(), "▸ <root> e");
    }
}
