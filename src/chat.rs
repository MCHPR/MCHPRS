use regex::Regex;
use serde::ser::Serializer;
use serde::Serialize;

lazy_static! {
    static ref URL_REGEX: Regex =
        Regex::new("^(?:(https?)://)?([-\\w_\\.]{2,}\\.[a-z]{2,4})(/\\S*)?$").unwrap();
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ChatColor {
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

impl ChatColor {
    fn from_color_code(code: char) -> Option<ChatColor> {
        match code {
            '0' => Some(ChatColor::Black),
            '1' => Some(ChatColor::DarkBlue),
            '2' => Some(ChatColor::DarkGreen),
            '3' => Some(ChatColor::DarkAqua),
            '4' => Some(ChatColor::DarkRed),
            '5' => Some(ChatColor::DarkPurple),
            '6' => Some(ChatColor::Gold),
            '7' => Some(ChatColor::Gray),
            '8' => Some(ChatColor::DarkGray),
            '9' => Some(ChatColor::Blue),
            'a' => Some(ChatColor::Green),
            'b' => Some(ChatColor::Aqua),
            'c' => Some(ChatColor::Red),
            'd' => Some(ChatColor::LightPurple),
            'e' => Some(ChatColor::Yellow),
            'f' => Some(ChatColor::White),
            'k' => Some(ChatColor::Obfuscated),
            'l' => Some(ChatColor::Bold),
            'm' => Some(ChatColor::Strikethrough),
            'n' => Some(ChatColor::Underline),
            'o' => Some(ChatColor::Italic),
            'r' => Some(ChatColor::Reset),
            _ => None,
        }
    }
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
enum ClickEventType {
    OpenUrl,
    RunCommand,
    SuggestCommand,
}

#[derive(Serialize, Debug, Clone)]
pub struct ClickEvent {
    action: ClickEventType,
    value: String,
}

/// This is only used for ChatComponent serialize
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(field: &bool) -> bool {
    !*field
}

fn ser_bool_str<S: Serializer>(val: &bool, s: S) -> Result<S::Ok, S::Error> {
    if *val {
        s.serialize_str("true")
    } else {
        s.serialize_str("false")
    }
}

pub struct ChatComponentBuilder {
    component: ChatComponent,
}

impl ChatComponentBuilder {
    pub fn new(text: String) -> Self {
        let component = ChatComponent {
            text,
            ..Default::default()
        };
        Self { component }
    }

    pub fn color(mut self, color: ChatColor) -> Self {
        self.component.color = Some(color);
        self
    }

    pub fn strikethrough(mut self, val: bool) -> Self {
        self.component.strikethrough = val;
        self
    }

    pub fn finish(self) -> ChatComponent {
        self.component
    }
}

#[derive(Serialize, Default, Debug, Clone)]
pub struct ChatComponent {
    pub text: String,
    #[serde(skip_serializing_if = "is_false", serialize_with = "ser_bool_str")]
    pub bold: bool,
    #[serde(skip_serializing_if = "is_false", serialize_with = "ser_bool_str")]
    pub italic: bool,
    #[serde(skip_serializing_if = "is_false", serialize_with = "ser_bool_str")]
    pub underlined: bool,
    #[serde(skip_serializing_if = "is_false", serialize_with = "ser_bool_str")]
    pub strikethrough: bool,
    #[serde(skip_serializing_if = "is_false", serialize_with = "ser_bool_str")]
    pub obfuscated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ChatColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub click_event: Option<ClickEvent>,
}

impl ChatComponent {
    pub fn from_legacy_text(message: String) -> Vec<ChatComponent> {
        let mut components = Vec::new();

        let mut next_component: ChatComponent = Default::default();
        let mut s = String::new();

        let mut chars = message.chars();
        while let Some(c) = chars.next() {
            if c == '&' {
                if let Some(code) = chars.next() {
                    if let Some(color) = ChatColor::from_color_code(code) {
                        if !s.is_empty() {
                            let formatting = next_component.clone();
                            next_component.text = s;
                            s = String::new();
                            components.push(next_component);
                            next_component = formatting;
                        }
                        match color {
                            ChatColor::Bold => next_component.bold = true,
                            ChatColor::Italic => next_component.italic = true,
                            ChatColor::Underline => next_component.underlined = true,
                            ChatColor::Strikethrough => next_component.strikethrough = true,
                            ChatColor::Obfuscated => next_component.obfuscated = true,
                            _ => {
                                next_component.text = s;
                                s = String::new();
                                components.push(next_component);
                                next_component = Default::default();
                                next_component.color = Some(color);
                            }
                        }
                        continue;
                    }
                    s.push(c);
                    s.push(code);
                    continue;
                }
            }
            s.push(c);
        }
        next_component.text = s;
        components.push(next_component);
        components
    }

    fn encode_json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}
