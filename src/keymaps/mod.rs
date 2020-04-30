//! Keymaps

use anyhow::{anyhow, Result};

use crate::bindings::{Binding, Keymap};

// Static data to generate a keymap.
type KeymapData = &'static [(
    (termwiz::input::Modifiers, termwiz::input::KeyCode), Binding,
)];

// Keymap macro implementation.
//
// Token-tree muncher: { rest } ( modifiers ) ( keys ) [ data ]
//
// Consumes definition from 'rest'.  Modifiers are accumulated in 'modifiers'.  Key definitions are
// accumulated in 'keys'.  Bindings are accumulated in 'data'.
macro_rules! keymap_impl {
    // Base case: generate keymap data.
    ( {} () () $data:tt ) => {
        pub(crate) static KEYMAP: $crate::keymaps::KeymapData = &$data;
    };

    // , (consume comma between keys)
    (
        { , $( $rest:tt )* }
        ( )
        ( $( $keys:tt )* )
        [ $( $data:tt )* ]
    ) => {
        keymap_impl! {
            { $( $rest )* }
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
        ( )
        ( )
        [ $( $data:tt )* ]
    ) => {
        keymap_impl! {
            { $( $rest )* }
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
        ( )
        ( $key:tt $( $keys:tt )* )
        [ $( $data:tt )* ]
    ) => {
        keymap_impl! {
            {
                => $binding $( ( $( $bind_params )* ) )? ;
                $( $rest )*
            }
            ( )
            ( $( $keys )* )
            [
                $( $data )*
                (
                    $key,
                    $crate::bindings::Binding::$binding $( ( $( $bind_params )* ) )?,
                ),
            ]
        }
    };

    // CTRL
    (
        { CTRL $( $rest:tt )* }
        ( $( $modifier:ident )* )
        ( $( $keys:tt )* )
        [ $( $data:tt )* ]
    ) => {
        keymap_impl! {
            { $( $rest )* }
            ( $( $modifier )* CTRL )
            ( $( $keys )* )
            [ $( $data )* ]
        }
    };

    // SHIFT
    (
        { SHIFT $( $rest:tt )* }
        ( $( $modifier:ident )* )
        ( $( $keys:tt )* )
        [ $( $data:tt )* ]
    ) => {
        keymap_impl! {
            { $( $rest )* }
            ( $( $modifier )* SHIFT )
            ( $( $keys )* )
            [ $( $data )* ]
        }
    };

    // ALT
    (
        { ALT $( $rest:tt )* }
        ( $( $modifier:ident )* )
        ( $( $keys:tt )* )
        [ $( $data:tt )* ]
    ) => {
        keymap_impl! {
            { $( $rest )* }
            ( $( $modifier )* ALT )
            ( $( $keys )* )
            [ $( $data )* ]
        }
    };

    // SUPER
    (
        { SUPER $( $rest:tt )* }
        ( $( $modifier:ident )* )
        ( $( $keys:tt )* )
        [ $( $data:tt )* ]
    ) => {
        keymap_impl! {
            { $( $rest )* }
            ( $( $modifier )* SUPER )
            ( $( $keys )* )
            [ $( $data )* ]
        }
    };

    // Character key (e.g. 'c')
    (
        { $key:literal $( $rest:tt )* }
        ( $( $modifier:ident )* )
        ( $( $keys:tt )* )
        [ $( $data:tt )* ]
    ) => {
        keymap_impl! {
            { $( $rest )* }
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
            )
            [ $( $data )* ]
        }
    };

    // KeyCode(params)
    (
        { $key:ident $( ( $( $key_params:tt )* ) )? $( $rest:tt )* }
        ( $( $modifier:ident )* )
        ( $( $keys:tt )* )
        [ $( $data:tt )* ]
    ) => {
        keymap_impl! {
            { $( $rest )* }
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
            )
            [ $( $data )* ]
        }
    };
}

macro_rules! keymap {
    ( $( $all:tt )* ) => {
        keymap_impl! { { $( $all )* } () () [] }
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

pub(crate) fn load(name: &str) -> Result<Keymap> {
    for (keymap_name, keymap_data) in KEYMAPS {
        if &name == keymap_name {
            return Ok(keymap_data.iter().cloned().collect());
        }
    }

    Err(anyhow!("Keymap not found: {}", name))
}
