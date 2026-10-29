//! mIRC formatting codes (bold, colors...) found in message text.
//!
//! Reference: <https://modern.ircdocs.horse/formatting>

const BOLD: char = '\x02';
const COLOR: char = '\x03';
const HEX_COLOR: char = '\x04';
const RESET: char = '\x0f';
const MONOSPACE: char = '\x11';
const REVERSE: char = '\x16';
const ITALIC: char = '\x1d';
const STRIKETHROUGH: char = '\x1e';
const UNDERLINE: char = '\x1f';

/// Text or background color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    /// Index in the mIRC palette (0–98).
    Palette(u8),
    Rgb(u8, u8, u8),
}

impl Color {
    /// RGB value of the color, if the palette index is defined.
    pub fn rgb(self) -> Option<(u8, u8, u8)> {
        match self {
            Self::Rgb(r, g, b) => Some((r, g, b)),
            Self::Palette(index) => PALETTE.get(usize::from(index)).map(|&hex| {
                let [_, r, g, b] = hex.to_be_bytes();
                (r, g, b)
            }),
        }
    }
}

/// Style applied to a span of text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub monospace: bool,
    pub reverse: bool,
    pub foreground: Option<Color>,
    pub background: Option<Color>,
}

/// Uniform span of text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span<'a> {
    pub text: &'a str,
    pub style: Style,
}

/// Splits a text into styled spans, removing the control codes.
pub fn parse(input: &str) -> Vec<Span<'_>> {
    let mut spans = Vec::new();
    let mut style = Style::default();
    let mut start = 0;
    let mut chars = input.char_indices().peekable();

    while let Some((index, c)) = chars.next() {
        let toggle: Option<fn(&mut Style)> = match c {
            BOLD => Some(|s| s.bold = !s.bold),
            ITALIC => Some(|s| s.italic = !s.italic),
            UNDERLINE => Some(|s| s.underline = !s.underline),
            STRIKETHROUGH => Some(|s| s.strikethrough = !s.strikethrough),
            MONOSPACE => Some(|s| s.monospace = !s.monospace),
            REVERSE => Some(|s| s.reverse = !s.reverse),
            RESET => Some(|s| *s = Style::default()),
            COLOR | HEX_COLOR => None,
            _ => continue,
        };

        if index > start {
            spans.push(Span {
                text: &input[start..index],
                style,
            });
        }

        if let Some(toggle) = toggle {
            toggle(&mut style);
            start = index + c.len_utf8();
            continue;
        }

        let rest = &input[index + 1..];
        let (colors, consumed) = if c == COLOR {
            parse_palette_colors(rest)
        } else {
            parse_hex_colors(rest)
        };
        if let Some((foreground, background)) = colors {
            style.foreground = foreground;
            if let Some(background) = background {
                style.background = background;
            }
        } else {
            // A color code without digits resets both colors.
            style.foreground = None;
            style.background = None;
        }
        start = index + 1 + consumed;
        while chars.peek().is_some_and(|&(i, _)| i < start) {
            chars.next();
        }
    }

    if start < input.len() {
        spans.push(Span {
            text: &input[start..],
            style,
        });
    }
    spans
}

/// Text without any formatting code.
pub fn strip(input: &str) -> String {
    let text: String = parse(input).into_iter().map(|span| span.text).collect();
    sanitize(&text).into_owned()
}

/// Removes invisible characters that are dangerous on display:
/// - bidirectional controls (U+202A–U+202E, U+2066–U+2069), which can
///   reverse text and disguise a link
///   (`https://evil.example/\u{202E}moc.elgoog`);
/// - the remaining C0 control characters (except tab), otherwise shown as
///   boxes.
///
/// mIRC formatting codes must be interpreted first.
pub fn sanitize(input: &str) -> std::borrow::Cow<'_, str> {
    let unsafe_char = |c: char| {
        matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}') || (c.is_control() && c != '\t')
    };
    if input.contains(unsafe_char) {
        input.chars().filter(|&c| !unsafe_char(c)).collect::<String>().into()
    } else {
        input.into()
    }
}

/// `(foreground, background)`; a `None` foreground means a reset.
/// The background is `None` if absent, `Some(None)` if it is “default” (99).
type ColorPair = Option<(Option<Color>, Option<Option<Color>>)>;

fn parse_palette_colors(rest: &str) -> (ColorPair, usize) {
    let Some((foreground, mut consumed)) = parse_digits(rest) else {
        return (None, 0);
    };
    let mut background = None;
    if let Some(after_comma) = rest[consumed..].strip_prefix(',')
        && let Some((bg, len)) = parse_digits(after_comma)
    {
        background = Some(palette_color(bg));
        consumed += 1 + len;
    }
    (Some((palette_color(foreground), background)), consumed)
}

fn parse_hex_colors(rest: &str) -> (ColorPair, usize) {
    let Some(foreground) = parse_hex(rest) else {
        return (None, 0);
    };
    let mut consumed = 6;
    let mut background = None;
    if let Some(after_comma) = rest[consumed..].strip_prefix(',')
        && let Some(bg) = parse_hex(after_comma)
    {
        background = Some(Some(bg));
        consumed += 7;
    }
    (Some((Some(foreground), background)), consumed)
}

fn parse_digits(s: &str) -> Option<(u8, usize)> {
    let len = s.bytes().take(2).take_while(u8::is_ascii_digit).count();
    (len > 0).then(|| (s[..len].parse().expect("chiffres ASCII"), len))
}

fn parse_hex(s: &str) -> Option<Color> {
    let hex = s.get(..6)?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    let [_, r, g, b] = value.to_be_bytes();
    Some(Color::Rgb(r, g, b))
}

fn palette_color(index: u8) -> Option<Color> {
    (index != 99).then_some(Color::Palette(index))
}

/// Extended mIRC palette (indices 0–98).
#[allow(clippy::unreadable_literal)]
const PALETTE: [u32; 99] = [
    0xffffff, 0x000000, 0x00007f, 0x009300, 0xff0000, 0x7f0000, 0x9c009c, 0xfc7f00,
    0xffff00, 0x00fc00, 0x009393, 0x00ffff, 0x0000fc, 0xff00ff, 0x7f7f7f, 0xd2d2d2,
    0x470000, 0x472100, 0x474700, 0x324700, 0x004700, 0x00472c, 0x004747, 0x002747,
    0x000047, 0x2e0047, 0x470047, 0x47002a, 0x740000, 0x743a00, 0x747400, 0x517400,
    0x007400, 0x007449, 0x007474, 0x004074, 0x000074, 0x4b0074, 0x740074, 0x740045,
    0xb50000, 0xb56300, 0xb5b500, 0x7db500, 0x00b500, 0x00b571, 0x00b5b5, 0x0063b5,
    0x0000b5, 0x7500b5, 0xb500b5, 0xb5006b, 0xff0000, 0xff8c00, 0xffff00, 0xb2ff00,
    0x00ff00, 0x00ffa0, 0x00ffff, 0x008cff, 0x0000ff, 0xa500ff, 0xff00ff, 0xff0098,
    0xff5959, 0xffb459, 0xffff71, 0xcfff60, 0x6fff6f, 0x65ffc9, 0x6dffff, 0x59b4ff,
    0x5959ff, 0xc459ff, 0xff66ff, 0xff59bc, 0xff9c9c, 0xffd39c, 0xffff9c, 0xe2ff9c,
    0x9cff9c, 0x9cffdb, 0x9cffff, 0x9cd3ff, 0x9c9cff, 0xdc9cff, 0xff9cff, 0xff94d3,
    0x000000, 0x131313, 0x282828, 0x363636, 0x4d4d4d, 0x656565, 0x818181, 0x9f9f9f,
    0xbcbcbc, 0xe2e2e2, 0xffffff,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_bidi_and_control_characters() {
        assert_eq!(sanitize("https://evil.example/\u{202E}moc.elgoog"), "https://evil.example/moc.elgoog");
        assert_eq!(sanitize("a\u{2067}b\u{7}c\td"), "abc\td");
        assert!(matches!(sanitize("clean é"), std::borrow::Cow::Borrowed(_)));
        assert_eq!(strip("\x02gras\x02\u{202E}"), "gras");
    }

    fn texts<'a>(spans: &[Span<'a>]) -> Vec<&'a str> {
        spans.iter().map(|s| s.text).collect()
    }

    #[test]
    fn plain_text_is_single_span() {
        let spans = parse("hello");
        assert_eq!(spans, [Span { text: "hello", style: Style::default() }]);
    }

    #[test]
    fn toggles_styles() {
        let spans = parse("a\x02b\x1dc\x02d\x0fe");
        assert_eq!(texts(&spans), ["a", "b", "c", "d", "e"]);
        assert!(spans[1].style.bold && !spans[1].style.italic);
        assert!(spans[2].style.bold && spans[2].style.italic);
        assert!(!spans[3].style.bold && spans[3].style.italic);
        assert_eq!(spans[4].style, Style::default());
    }

    #[test]
    fn parses_palette_colors() {
        let spans = parse("\x034red\x0304,12on blue\x03plain");
        assert_eq!(texts(&spans), ["red", "on blue", "plain"]);
        assert_eq!(spans[0].style.foreground, Some(Color::Palette(4)));
        assert_eq!(spans[1].style.background, Some(Color::Palette(12)));
        assert_eq!(spans[2].style.foreground, None);
        assert_eq!(spans[2].style.background, None);
    }

    #[test]
    fn comma_without_digit_is_text() {
        let spans = parse("\x035,text");
        assert_eq!(texts(&spans), [",text"]);
        assert_eq!(spans[0].style.foreground, Some(Color::Palette(5)));
    }

    #[test]
    fn three_digits_keep_the_third_as_text() {
        let spans = parse("\x031234");
        assert_eq!(texts(&spans), ["34"]);
        assert_eq!(spans[0].style.foreground, Some(Color::Palette(12)));
    }

    #[test]
    fn color_99_means_default() {
        let spans = parse("\x0399,99x");
        assert_eq!(spans[0].style.foreground, None);
        assert_eq!(spans[0].style.background, None);
    }

    #[test]
    fn parses_hex_colors() {
        let spans = parse("\x04FF8800,000000x");
        assert_eq!(spans[0].style.foreground, Some(Color::Rgb(0xff, 0x88, 0)));
        assert_eq!(spans[0].style.background, Some(Color::Rgb(0, 0, 0)));
    }

    #[test]
    fn handles_multibyte_text() {
        assert_eq!(strip("\x02été\x02 ☕"), "été ☕");
    }

    #[test]
    fn palette_lookup() {
        assert_eq!(Color::Palette(4).rgb(), Some((0xff, 0, 0)));
        assert_eq!(Color::Palette(98).rgb(), Some((0xff, 0xff, 0xff)));
        assert_eq!(Color::Palette(120).rgb(), None);
    }
}
