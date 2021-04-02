use regex::Regex;
use serde::Serialize;

lazy_static! {
    static ref URL_REGEX: Regex =
        Regex::new("^(?:(https?)://)?([-\\w_\\.]{2,}\\.[a-z]{2,4})(/\\S*)?$").unwrap();
}

#[derive(Serialize, Debug, Clone)]
enum ChatColor {
    #[serde(rename = "black")]
    Black,
    #[serde(rename = "dark_blue")]
    DarkBlue,
    #[serde(rename = "dark_green")]
    DarkGreen,
    #[serde(rename = "dark_aqua")]
    DarkAqua,
    #[serde(rename = "dark_red")]
    DarkRed,
    #[serde(rename = "dark_purple")]
    DarkPurple,
    #[serde(rename = "gold")]
    Gold,
    #[serde(rename = "gray")]
    Gray,
    #[serde(rename = "dark_gray")]
    DarkGray,
    #[serde(rename = "blue")]
    Blue,
    #[serde(rename = "green")]
    Green,
    #[serde(rename = "aqua")]
    Aqua,
    #[serde(rename = "red")]
    Red,
    #[serde(rename = "light_purple")]
    LightPurple,
    #[serde(rename = "yellow")]
    Yellow,
    #[serde(rename = "white")]
    White,
    #[serde(rename = "obfuscated")]
    Obfuscated,
    #[serde(rename = "bold")]
    Bold,
    #[serde(rename = "strikethrough")]
    Strikethrough,
    #[serde(rename = "underline")]
    Underline,
    #[serde(rename = "italic")]
    Italic,
    #[serde(rename = "reset")]
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
enum ClickEventType {
    #[serde(rename = "open_url")]
    OpenUrl,
    #[serde(rename = "run_command")]
    RunCommand,
    #[serde(rename = "suggest_command")]
    SuggestCommand,
}

#[derive(Serialize, Debug, Clone)]
struct ClickEvent {
    action: ClickEventType,
    value: String,
}

/// This is only used for ChatComponent serialize
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(field: &bool) -> bool {
    !*field
}

#[derive(Serialize, Default, Debug, Clone)]
pub struct ChatComponent {
    text: String,
    #[serde(skip_serializing_if = "is_false")]
    bold: bool,
    #[serde(skip_serializing_if = "is_false")]
    italic: bool,
    #[serde(skip_serializing_if = "is_false")]
    underlined: bool,
    #[serde(skip_serializing_if = "is_false")]
    strikethrough: bool,
    #[serde(skip_serializing_if = "is_false")]
    obfuscated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<ChatColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    click_event: Option<ClickEvent>,
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
