//! Keymaps

use anyhow::{anyhow, Result};
use pest::Parser;
use pest_derive::Parser;
use termwiz::input::{KeyCode, Modifiers};

use crate::bindings::{Binding, BindingConfig};

#[derive(Parser)]
#[grammar = "keymaps/keymap.pest"]
struct KeymapFileParser;

// File data to generate a keymap
pub(crate) struct KeymapFile(Vec<((Modifiers, KeyCode), BindingConfig)>);

impl KeymapFile {
    fn parse_keycode(ident: &str) -> Option<KeyCode> {
        use KeyCode::*;
        match ident {
            "Space" => Some(Char(' ')),
            "Cancel" => Some(Cancel),
            "Backspace" => Some(Backspace),
            "Tab" => Some(Tab),
            "Clear" => Some(Clear),
            "Enter" => Some(Enter),
            "Shift" => Some(Shift),
            "Escape" => Some(Escape),
            "Menu" => Some(Menu),
            "LeftMenu" => Some(LeftMenu),
            "RightMenu" => Some(RightMenu),
            "Pause" => Some(Pause),
            "CapsLock" => Some(CapsLock),
            "PageUp" => Some(PageUp),
            "PageDown" => Some(PageDown),
            "End" => Some(End),
            "Home" => Some(Home),
            "LeftArrow" => Some(LeftArrow),
            "RightArrow" => Some(RightArrow),
            "UpArrow" => Some(UpArrow),
            "DownArrow" => Some(DownArrow),
            "Left" => Some(LeftArrow),
            "Right" => Some(RightArrow),
            "Up" => Some(UpArrow),
            "Down" => Some(DownArrow),
            "Select" => Some(Select),
            "Print" => Some(Print),
            "Execute" => Some(Execute),
            "PrintScreen" => Some(PrintScreen),
            "Insert" => Some(Insert),
            "Delete" => Some(Delete),
            "Help" => Some(Help),
            "Applications" => Some(Applications),
            "Sleep" => Some(Sleep),
            "Numpad0" => Some(Numpad0),
            "Numpad1" => Some(Numpad1),
            "Numpad2" => Some(Numpad2),
            "Numpad3" => Some(Numpad3),
            "Numpad4" => Some(Numpad4),
            "Numpad5" => Some(Numpad5),
            "Numpad6" => Some(Numpad6),
            "Numpad7" => Some(Numpad7),
            "Numpad8" => Some(Numpad8),
            "Numpad9" => Some(Numpad9),
            "Multiply" => Some(Multiply),
            "Add" => Some(Add),
            "Separator" => Some(Separator),
            "Subtract" => Some(Subtract),
            "Decimal" => Some(Decimal),
            "Divide" => Some(Divide),
            "NumLock" => Some(NumLock),
            "ScrollLock" => Some(ScrollLock),
            "BrowserBack" => Some(BrowserBack),
            "BrowserForward" => Some(BrowserForward),
            "BrowserRefresh" => Some(BrowserRefresh),
            "BrowserStop" => Some(BrowserStop),
            "BrowserSearch" => Some(BrowserSearch),
            "BrowserFavorites" => Some(BrowserFavorites),
            "BrowserHome" => Some(BrowserHome),
            "VolumeMute" => Some(VolumeMute),
            "VolumeDown" => Some(VolumeDown),
            "VolumeUp" => Some(VolumeUp),
            "MediaNextTrack" => Some(MediaNextTrack),
            "MediaPrevTrack" => Some(MediaPrevTrack),
            "MediaStop" => Some(MediaStop),
            "MediaPlayPause" => Some(MediaPlayPause),
            "ApplicationLeftArrow" => Some(ApplicationLeftArrow),
            "ApplicationRightArrow" => Some(ApplicationRightArrow),
            "ApplicationUpArrow" => Some(ApplicationUpArrow),
            "ApplicationDownArrow" => Some(ApplicationDownArrow),
            other => {
                if other.starts_with('F') && other.chars().skip(1).all(char::is_numeric) {
                    let n = other[1..].parse::<u8>().ok()?;
                    Some(Function(n))
                } else {
                    None
                }
            }
        }
    }

    fn parse_key(pair: pest::iterators::Pair<Rule>) -> Result<((Modifiers, KeyCode), bool)> {
        let key = pair.into_inner().next().expect("key should contain item");
        let mut modifiers = Modifiers::NONE;
        let visible = match key.as_rule() {
            Rule::visible_key => true,
            Rule::invisible_key => false,
            other => panic!("unexpected rule inside key: {:?}", other),
        };
        for item in key.into_inner() {
            match item.as_rule() {
                Rule::modifier => match item.as_str() {
                    "SHIFT" => modifiers |= Modifiers::SHIFT,
                    "CTRL" => modifiers |= Modifiers::CTRL,
                    "ALT" => modifiers |= Modifiers::ALT,
                    "SUPER" => modifiers |= Modifiers::SUPER,
                    unknown => return Err(anyhow!("Unknown modifier: {}", unknown)),
                },
                Rule::keycode => {
                    let value = item
                        .into_inner()
                        .next()
                        .expect("keycode should contain value");
                    let keycode = match value.as_rule() {
                        Rule::char => KeyCode::Char(
                            value
                                .as_str()
                                .chars()
                                .next()
                                .expect("keycode should contain a character"),
                        ),
                        Rule::ident => Self::parse_keycode(value.as_str())
                            .ok_or_else(|| anyhow!("Unrecognised key: {}", value.as_str()))?,
                        other => panic!("Unexpected rule inside keycode: {:?}", other),
                    };
                    return Ok(((modifiers, keycode), visible));
                }
                other => panic!("Unexpected rule inside key: {:?}", other),
            }
        }
        Err(anyhow!("Key definition missing"))
    }

    fn parse_binding(pair: pest::iterators::Pair<Rule>) -> Result<Binding> {
        let mut ident = None;
        let mut params = Vec::new();
        let span = pair.as_str();
        for item in pair.into_inner() {
            match item.as_rule() {
                Rule::ident => ident = Some(String::from(item.as_str())),
                Rule::binding_param => params.push(String::from(item.as_str())),
                other => panic!("Unexpected rule inside binding: {:?}", other),
            }
        }
        let ident = ident.ok_or_else(|| anyhow!("invalid binding: {}", span))?;
        Ok(Binding::parse(ident, params)?)
    }

    pub(crate) fn parse(data: &str) -> Result<KeymapFile> {
        let mut keymap = Vec::new();
        let mut parsed = KeymapFileParser::parse(Rule::file, data)?;
        if let Some(file) = parsed.next() {
            for item in file.into_inner() {
                let mut keys = Vec::new();
                if item.as_rule() == Rule::item {
                    for part in item.into_inner() {
                        match part.as_rule() {
                            Rule::key => {
                                keys.push(Self::parse_key(part)?);
                            }
                            Rule::binding => {
                                let binding = Self::parse_binding(part)?;
                                for (key, visible) in keys.into_iter() {
                                    let binding = binding.clone();
                                    let binding_config = BindingConfig { binding, visible };
                                    keymap.push((key, binding_config));
                                }
                                keys = Vec::new();
                            }
                            other => panic!("Unexpected rule inside item: {:?}", other),
                        }
                    }
                }
            }
        }
        Ok(KeymapFile(keymap))
    }

    pub(crate) fn iter(&self) -> impl IntoIterator<Item = &((Modifiers, KeyCode), BindingConfig)> {
        self.0.iter()
    }
}
