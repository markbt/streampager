//! Keymaps

use anyhow::{anyhow, Context, Result};
use pest::Parser;
use pest_derive::Parser;
use termwiz::input::{KeyCode, Modifiers};

use crate::bindings::{Binding, Keybind, Keymap};

// Static data to generate a keymap.
type KeymapData = &'static [((Modifiers, KeyCode), Keybind)];

// Keymap macro implementation.
//
// Token-tree muncher: { rest } ( visible ) ( modifiers ) ( keys ) [ data ]
//
// Consumes definition from 'rest'.  Modifiers are accumulated in 'modifiers'.  Key definitions are
// accumulated in 'keys'.  Bindings are accumulated in 'data'.
macro_rules! keymap_impl {
    // Base case: generate keymap data.
    ( {} ( $visible:literal ) () () $data:tt ) => {
        pub(crate) static KEYMAP: $crate::keymaps::KeymapData = &$data;
    };

    // , (consume comma between keys)
    (
        { , $( $rest:tt )* }
        ( $visible:literal )
        ( )
        ( $( $keys:tt )* )
        [ $( $data:tt )* ]
    ) => {
        keymap_impl! {
            { $( $rest )* }
            ( $visible )
            ( )
            ( $( $keys )* )
            [ $( $data )* ]
        }
    };

    // => Binding (termination)
    (
        {
            => $binding:ident $( ( $( $bind_params:tt )* ) )? ;
            $( $rest:tt )*
        }
        ( $visible:literal )
        ( )
        ( )
        [ $( $data:tt )* ]
    ) => {
        keymap_impl! {
            { $( $rest )* }
            ( $visible )
            ( )
            ( )
            [ $( $data )* ]
        }
    };

    // => Binding (assign key)
    (
        {
            => $binding:ident $( ( $( $bind_params:tt )* ) )? ;
            $( $rest:tt )*
        }
        ( $visible:literal )
        ( )
        ( $key:tt $key_visible:literal $( $keys:tt )* )
        [ $( $data:tt )* ]
    ) => {
        keymap_impl! {
            {
                => $binding $( ( $( $bind_params )* ) )? ;
                $( $rest )*
            }
            ( $visible )
            ( )
            ( $( $keys )* )
            [
                $( $data )*
                (
                    $key,
                    $crate::bindings::Keybind {
                        binding: $crate::bindings::Binding::$binding $( ( $( $bind_params )* ) )?,
                        visible: $key_visible,
                    },
                ),
            ]
        }
    };

    // CTRL
    (
        { CTRL $( $rest:tt )* }
        ( $visible:literal )
        ( $( $modifier:ident )* )
        ( $( $keys:tt )* )
        [ $( $data:tt )* ]
    ) => {
        keymap_impl! {
            { $( $rest )* }
            ( $visible )
            ( $( $modifier )* CTRL )
            ( $( $keys )* )
            [ $( $data )* ]
        }
    };

    // SHIFT
    (
        { SHIFT $( $rest:tt )* }
        ( $visible:literal )
        ( $( $modifier:ident )* )
        ( $( $keys:tt )* )
        [ $( $data:tt )* ]
    ) => {
        keymap_impl! {
            { $( $rest )* }
            ( $visible )
            ( $( $modifier )* SHIFT )
            ( $( $keys )* )
            [ $( $data )* ]
        }
    };

    // ALT
    (
        { ALT $( $rest:tt )* }
        ( $visible:literal )
        ( $( $modifier:ident )* )
        ( $( $keys:tt )* )
        [ $( $data:tt )* ]
    ) => {
        keymap_impl! {
            { $( $rest )* }
            ( $visible )
            ( $( $modifier )* ALT )
            ( $( $keys )* )
            [ $( $data )* ]
        }
    };

    // SUPER
    (
        { SUPER $( $rest:tt )* }
        ( $visible:literal )
        ( $( $modifier:ident )* )
        ( $( $keys:tt )* )
        [ $( $data:tt )* ]
    ) => {
        keymap_impl! {
            { $( $rest )* }
            ( $visible )
            ( $( $modifier )* SUPER )
            ( $( $keys )* )
            [ $( $data )* ]
        }
    };

    // Character key (e.g. 'c')
    (
        { $key:literal $( $rest:tt )* }
        ( $visible:literal )
        ( $( $modifier:ident )* )
        ( $( $keys:tt )* )
        [ $( $data:tt )* ]
    ) => {
        keymap_impl! {
            { $( $rest )* }
            ( true )
            ( )
            (
                $( $keys )*
                (
                    termwiz::input::Modifiers::from_bits_truncate(
                        $( termwiz::input::Modifiers::$modifier.bits() | )*
                        termwiz::input::Modifiers::NONE.bits()
                    ),
                    termwiz::input::KeyCode::Char($key),
                )
                $visible
            )
            [ $( $data )* ]
        }
    };

    // KeyCode(params)
    (
        { $key:ident $( ( $( $key_params:tt )* ) )? $( $rest:tt )* }
        ( $visible:literal )
        ( $( $modifier:ident )* )
        ( $( $keys:tt )* )
        [ $( $data:tt )* ]
    ) => {
        keymap_impl! {
            { $( $rest )* }
            ( true )
            ( )
            (
                $( $keys )*
                (
                    termwiz::input::Modifiers::from_bits_truncate(
                        $( termwiz::input::Modifiers::$modifier.bits() | )*
                        termwiz::input::Modifiers::NONE.bits()
                    ),
                    termwiz::input::KeyCode::$key $( ( $( $key_params )* ) )?,
                )
                $visible
            )
            [ $( $data )* ]
        }
    };

    // ( hidden binding )
    (
        { ( $( $bind:tt )* ) $( $rest:tt )* }
        ( $visible:literal )
        ( $( $modifier:ident )* )
        ( $( $keys:tt )* )
        [ $( $data:tt )* ]
    ) => {
        keymap_impl! {
            { $( $bind )* $( $rest )* }
            ( false )
            ( $( $modifier )* )
            ( $( $keys )* )
            [ $( $data )* ]
        }
    };
}

macro_rules! keymap {
    ( $( $all:tt )* ) => {
        keymap_impl! { { $( $all )* } (true) () () [] }
    };
}

macro_rules! keymaps {
    ( $( $visibility:vis mod $name:ident ; )* ) => {
        $( $visibility mod $name ; )*

        pub(crate) static KEYMAPS: &'static [(&'static str, $crate::keymaps::KeymapData)] = &[
            $( ( stringify!( $name ), $crate::keymaps::$name::KEYMAP ), )*
        ];
    }
}

keymaps! {
    pub(crate) mod default;
}

#[derive(Parser)]
#[grammar = "keymaps/keymap.pest"]
struct KeymapFileParser;

// File data to generate a keymap
struct KeymapFile(Vec<((Modifiers, KeyCode), Keybind)>);

impl KeymapFile {
    fn parse_keycode(ident: &str) -> Option<KeyCode> {
        use KeyCode::*;
        match ident {
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
                if other.starts_with("F") && other.chars().skip(1).all(char::is_numeric) {
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
        return Err(anyhow!("Key definition missing"));
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
                                    let keybind = Keybind { binding, visible };
                                    keymap.push((key, keybind));
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
}

pub(crate) fn load(name: &str) -> Result<Keymap> {
    for (keymap_name, keymap_data) in KEYMAPS {
        if &name == keymap_name {
            return Ok(Keymap::from(keymap_data.iter()));
        }
    }

    if let Some(mut path) = dirs::config_dir() {
        path.push("streampager");
        path.push("keymaps");
        path.push(name);
        if let Ok(keymap_data) = std::fs::read_to_string(&path) {
            let keymap_file = KeymapFile::parse(&keymap_data)
                .with_context(|| format!("failed to parse keymap from {:?}", path))?;
            return Ok(Keymap::from(keymap_file.0.iter()));
        }
    }

    Err(anyhow!("Keymap not found: {}", name))
}
