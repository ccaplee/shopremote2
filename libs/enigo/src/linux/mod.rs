/// nix-based Linux 구현
mod nix_impl;
/// xdotool 기반 X11 구현
mod xdo;

pub use self::nix_impl::Enigo;
