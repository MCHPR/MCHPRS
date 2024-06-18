use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;

static URL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new("([a-zA-Z0-9§\\-:/]+\\.[a-zA-Z/0-9§\\-:_#]+(\\.[a-zA-Z/0-9.§\\-:#\\?\\+=_]+)?)")
        .unwrap()
});

fn is_valid_hex(ch: char) -> bool {
    ch.is_numeric() || ('a'..='f').contains(&ch) || ('A'..='F').contains(&ch)
}

#[derive(Serialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ColorCode {
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

#[derive(Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum TextColor {
    Hex(String),
    ColorCode(ColorCode),
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
enum ClickEventType {
    OpenUrl,
    // RunCommand,
    // SuggestCommand,
}

#[derive(Serialize, Debug, Clone)]
pub struct ClickEvent {
    action: ClickEventType,
    value: String,
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

    pub fn strikethrough(mut self, val: bool) -> Self {
        self.component.strikethrough = val;
        self
    }

    pub fn finish(self) -> TextComponent {
        self.component
    }
}

#[derive(Serialize, Default, Debug, Clone)]
pub struct TextComponent {
    pub text: String,
    #[serde(skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub underlined: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub strikethrough: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub obfuscated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<TextColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "clickEvent")]
    pub click_event: Option<ClickEvent>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<TextComponent>,
}

impl TextComponent {
    pub fn from_legacy_text(message: &str) -> Vec<TextComponent> {
        let mut components = Vec::new();

        let mut cur_component: TextComponent = Default::default();

        let mut chars = message.chars();
        'main_loop: while let Some(c) = chars.next() {
            if c == '&' {
                if let Some(code) = chars.next() {
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
                                cur_component.color = Some(TextColor::ColorCode(color));
                            }
                        }
                        continue;
                    }
                    cur_component.text.push(c);
                    cur_component.text.push(code);
                    continue;
                }
            }
            if c == '#' {
                let mut hex = String::from(c);
                for _ in 0..6 {
                    if let Some(c) = chars.next() {
                        hex.push(c);
                        if !is_valid_hex(c) {
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

        // This code is stinky
        // Find urls and add click action
        let mut new_componenets = Vec::with_capacity(components.len());
        for component in components {
            let mut last = 0;
            let text = &component.text;

            for match_ in URL_REGEX.find_iter(text) {
                let index = match_.start();
                let matched = match_.as_str();
                if last != index {
                    let mut new = component.clone();
                    new.text = String::from(&text[last..index]);
                    new_componenets.push(new);
                }
                let mut new = component.clone();
                new.text = matched.to_string();
                new.click_event = Some(ClickEvent {
                    action: ClickEventType::OpenUrl,
                    value: matched.to_string(),
                });
                new_componenets.push(new);
                last = index + matched.len();
            }
            if last < text.len() {
                let mut new = component.clone();
                new.text = String::from(&text[last..]);
                new_componenets.push(new);
            }
        }

        new_componenets
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
            && self.color.is_none()
            && self.click_event.is_none()
    }
}

impl<S> From<S> for TextComponent
where
    S: Into<String>,
{
    fn from(value: S) -> Self {
        let mut tc: TextComponent = Default::default();
        tc.text = value.into();
        tc
    }
}
