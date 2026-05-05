mod root_view;

use anyhow::{anyhow, Result};
use rust_embed::RustEmbed;
use std::borrow::Cow;
use warpui::{platform, AddWindowOptions, AssetProvider};

#[derive(Clone, Copy, RustEmbed)]
#[folder = "assets"]
pub struct Assets;

pub static ASSETS: Assets = Assets;

impl AssetProvider for Assets {
    fn get(&self, path: &str) -> Result<Cow<'_, [u8]>> {
        <Assets as RustEmbed>::get(path)
            .map(|f| f.data)
            .ok_or_else(|| anyhow!("no asset exists at path {}", path))
    }
}

fn main() -> Result<()> {
    println!("PureWarp - Starting GPU-accelerated terminal emulator...");

    let app_builder =
        platform::AppBuilder::new(platform::AppCallbacks::default(), Box::new(ASSETS), None);

    let _ = app_builder.run(move |ctx| {
        ctx.add_window(AddWindowOptions::default(), |_cx| root_view::RootView {});
    });

    Ok(())
}
