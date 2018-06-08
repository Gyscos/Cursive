use super::Color;
use enum_map::EnumMap;
use toml;
use std::collections::HashMap;
use std::ops::{Index, IndexMut};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteValue {
    Leaf(Color),
    Node(HashMap<String, PaletteValue>),
}

/// Color configuration for the application.
///
/// Assign each color role an actual color.
///
/// It implements `Index` and `IndexMut` to access and modify this mapping:
///
/// # Example
///
/// ```rust
/// # use cursive::theme;
/// use cursive::theme::PaletteColor::*;
/// use cursive::theme::Color::*;
/// use cursive::theme::BaseColor::*;
///
/// let mut palette = theme::default_palette();
///
/// assert_eq!(palette[Background], Dark(Blue));
/// palette[Shadow] = Light(Red);
/// ```
// pub type Palette = EnumMap<PaletteColor, Color>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    default : EnumMap<PaletteColor, Color>,
    custom : Option<PaletteValue>
}

impl Index<PaletteColor> for Palette {
    type Output = Color;

    fn index(&self, palette_color : PaletteColor) -> &Color {
        &self.default[palette_color]
    }
}

impl IndexMut<PaletteColor> for Palette {
    fn index_mut(&mut self, palette_color : PaletteColor) -> &mut Color {
        &mut self.default[palette_color]
    }
}

impl Index<(&'static str, PaletteColor)> for Palette {
    type Output = Color;

    fn index(&self, (path, palette_color) : (&'static str, PaletteColor)) -> &Color {
        if self.custom.is_none() {
            &self.default[palette_color]
        } else {

            let mut current_node : Option<&PaletteValue> = self.custom.as_ref();
            for node_name in path.split('/') {
                match current_node {
                    Some(PaletteValue::Node(hm)) => current_node = hm.get(node_name),
                    // this default cover both None, and Some(PaletteValue::Leaf(_)) in case where
                    // a internal node is expected.
                    _ => return &self.default[palette_color]
                }
            }

            match current_node {
                Some(PaletteValue::Leaf(color)) => color,
                // this default cover both None, and Some(PaletteValue::Leaf(_)) in case where
                // a internal node is expected.
                _ => &self.default[palette_color]
            }
        }
    }
}

/// Returns the default palette for a cursive application.
///
/// * `Background` => `Dark(Blue)`
/// * `Shadow` => `Dark(Black)`
/// * `View` => `Dark(White)`
/// * `Primary` => `Dark(Black)`
/// * `Secondary` => `Dark(Blue)`
/// * `Tertiary` => `Dark(White)`
/// * `TitlePrimary` => `Dark(Red)`
/// * `TitleSecondary` => `Dark(Yellow)`
/// * `Highlight` => `Dark(Red)`
/// * `HighlightInactive` => `Dark(Blue)`
pub fn default_palette() -> Palette {
    use self::PaletteColor::*;
    use theme::BaseColor::*;
    use theme::Color::*;

    Palette {
        default : enum_map!{
            Background => Dark(Blue),
            Shadow => Dark(Black),
            View => Dark(White),
            Primary => Dark(Black),
            Secondary => Dark(Blue),
            Tertiary => Dark(White),
            TitlePrimary => Dark(Red),
            TitleSecondary => Dark(Yellow),
            Highlight => Dark(Red),
            HighlightInactive => Dark(Blue),
        },
        custom : None
    }
}

/// Fills `palette` with the colors from the given `table`.
pub(crate) fn load_table(palette: &mut Palette, table: &toml::value::Table) {
    // TODO: use serde for that?
    // Problem: toml-rs doesn't do well with Enums...
    load_color(
        &mut palette[PaletteColor::Background],
        table.get("background"),
    );
    load_color(&mut palette[PaletteColor::Shadow], table.get("shadow"));
    load_color(&mut palette[PaletteColor::View], table.get("view"));
    load_color(&mut palette[PaletteColor::Primary], table.get("primary"));
    load_color(
        &mut palette[PaletteColor::Secondary],
        table.get("secondary"),
    );
    load_color(&mut palette[PaletteColor::Tertiary], table.get("tertiary"));
    load_color(
        &mut palette[PaletteColor::TitlePrimary],
        table.get("title_primary"),
    );
    load_color(
        &mut palette[PaletteColor::TitleSecondary],
        table.get("title_secondary"),
    );
    load_color(
        &mut palette[PaletteColor::Highlight],
        table.get("highlight"),
    );
    load_color(
        &mut palette[PaletteColor::HighlightInactive],
        table.get("highlight_inactive"),
    );
}

/// Color entry in a palette.
///
/// Each `ColorRole` is used for a specific role in a default application.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumMap)]
pub enum PaletteColor {
    /// Color used for the application background.
    Background,
    /// Color used for View shadows.
    Shadow,
    /// Color used for View backgrounds.
    View,
    /// Primary color used for the text.
    Primary,
    /// Secondary color used for the text.
    Secondary,
    /// Tertiary color used for the text.
    Tertiary,
    /// Primary color used for title text.
    TitlePrimary,
    /// Secondary color used for title text.
    TitleSecondary,
    /// Color used for highlighting text.
    Highlight,
    /// Color used for highlighting inactive text.
    HighlightInactive,
}

impl PaletteColor {
    /// Given a palette, resolve `self` to a concrete color.
    pub fn resolve(self, palette: &Palette) -> Color {
        palette[self]
    }
}

/// Parses `value` and fills `target` if it's a valid color.
fn load_color(target: &mut Color, value: Option<&toml::Value>) -> bool {
    if let Some(value) = value {
        match *value {
            toml::Value::String(ref value) => {
                if let Some(color) = Color::parse(value) {
                    *target = color;
                    true
                } else {
                    false
                }
            }
            toml::Value::Array(ref array) => {
                array.iter().any(|item| load_color(target, Some(item)))
            }
            _ => false,
        }
    } else {
        false
    }
}
