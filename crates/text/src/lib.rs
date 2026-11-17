use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum ColorCode {
    #[default]
    Black,
    DarkBlue,
    DarkGreen,
    DarkAqua,
    DarkRed,
    DarkPurple,
    Gold,
    Gray,
    DarkGray,
    Blue,
    Green,
    Aqua,
    Red,
    LightPurple,
    Yellow,
    White,
    Obfuscated,
    Bold,
    Strikethrough,
    Underline,
    Italic,
    Reset,
}

impl ColorCode {
    fn parse(code: char) -> Option<ColorCode> {
        Some(match code {
            '0' => ColorCode::Black,
            '1' => ColorCode::DarkBlue,
            '2' => ColorCode::DarkGreen,
            '3' => ColorCode::DarkAqua,
            '4' => ColorCode::DarkRed,
            '5' => ColorCode::DarkPurple,
            '6' => ColorCode::Gold,
            '7' => ColorCode::Gray,
            '8' => ColorCode::DarkGray,
            '9' => ColorCode::Blue,
            'a' => ColorCode::Green,
            'b' => ColorCode::Aqua,
            'c' => ColorCode::Red,
            'd' => ColorCode::LightPurple,
            'e' => ColorCode::Yellow,
            'f' => ColorCode::White,
            'k' => ColorCode::Obfuscated,
            'l' => ColorCode::Bold,
            'm' => ColorCode::Strikethrough,
            'n' => ColorCode::Underline,
            'o' => ColorCode::Italic,
            'r' => ColorCode::Reset,
            _ => return None,
        })
    }

    fn is_formatting(self) -> bool {
        use ColorCode::*;
        matches!(
            self,
            Obfuscated | Bold | Strikethrough | Underline | Italic | Reset
        )
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum TextColor {
    Hex(String),
    ColorCode(ColorCode),
}

impl Into<TextColor> for ColorCode {
    fn into(self) -> TextColor {
        TextColor::ColorCode(self)
    }
}

/// This is only used for `TextComponent` serialize
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(field: &bool) -> bool {
    !*field
}

pub struct TextComponentBuilder {
    component: TextComponent,
}

impl TextComponentBuilder {
    pub fn new(text: String) -> Self {
        let component = TextComponent {
            text,
            ..Default::default()
        };
        Self { component }
    }

    pub fn color(mut self, color: TextColor) -> Self {
        self.component.color = Some(color);
        self
    }

    pub fn color_code(mut self, color: ColorCode) -> Self {
        self.component.color = Some(TextColor::ColorCode(color));
        self
    }

    pub fn strikethrough(mut self) -> Self {
        self.component.strikethrough = true;
        self
    }

    pub fn bold(mut self) -> Self {
        self.component.bold = true;
        self
    }

    pub fn finish(self) -> TextComponent {
        self.component
    }
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct TextComponent {
    pub text: String,
    #[serde(skip_serializing_if = "is_false")]
    #[serde(default)]
    pub bold: bool,
    #[serde(skip_serializing_if = "is_false")]
    #[serde(default)]
    pub italic: bool,
    #[serde(skip_serializing_if = "is_false")]
    #[serde(default)]
    pub underlined: bool,
    #[serde(skip_serializing_if = "is_false")]
    #[serde(default)]
    pub strikethrough: bool,
    #[serde(skip_serializing_if = "is_false")]
    #[serde(default)]
    pub obfuscated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<TextColor>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub extra: Vec<TextComponent>,
}

impl TextComponent {
    pub fn from_legacy_text(message: &str) -> Vec<TextComponent> {
        let mut components = Vec::new();

        let mut cur_component: TextComponent = Default::default();

        let mut chars = message.chars();
        'main_loop: while let Some(c) = chars.next() {
            if c == '&'
                && let Some(code) = chars.next()
            {
                if let Some(color) = ColorCode::parse(code) {
                    let make_new = !cur_component.text.is_empty();
                    if color.is_formatting() && make_new {
                        components.push(cur_component.clone());
                        cur_component.text.clear();
                    }
                    match color {
                        ColorCode::Bold => cur_component.bold = true,
                        ColorCode::Italic => cur_component.italic = true,
                        ColorCode::Underline => cur_component.underlined = true,
                        ColorCode::Strikethrough => cur_component.strikethrough = true,
                        ColorCode::Obfuscated => cur_component.obfuscated = true,
                        _ => {
                            components.push(cur_component);
                            cur_component = Default::default();
                            if !matches!(color, ColorCode::Reset) {
                                cur_component.color = Some(TextColor::ColorCode(color));
                            }
                        }
                    }
                    continue;
                }
                cur_component.text.push(c);
                cur_component.text.push(code);
                continue;
            }
            if c == '#' {
                let mut hex = String::from(c);
                for _ in 0..6 {
                    if let Some(c) = chars.next() {
                        hex.push(c);
                        if !c.is_ascii_hexdigit() {
                            cur_component.text += &hex;
                            continue 'main_loop;
                        }
                    } else {
                        cur_component.text += &hex;
                        continue 'main_loop;
                    }
                }
                components.push(cur_component);
                cur_component = Default::default();
                cur_component.color = Some(TextColor::Hex(hex));
                continue;
            }
            cur_component.text.push(c);
        }
        components.push(cur_component);
        components.retain(|component| !component.text.is_empty());
        components
    }

    pub fn encode_json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }

    pub fn is_text_only(&self) -> bool {
        !self.bold
            && !self.italic
            && !self.underlined
            && !self.strikethrough
            && !self.obfuscated
            && self.extra.is_empty()
            && self.color.is_none()
    }
}

impl<S> From<S> for TextComponent
where
    S: Into<String>,
{
    fn from(value: S) -> Self {
        TextComponent {
            text: value.into(),
            ..Default::default()
        }
    }
}
