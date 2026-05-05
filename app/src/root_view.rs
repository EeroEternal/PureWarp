use pathfinder_color::ColorU;
use warpui::{elements::Rect, AppContext, Element, Entity, TypedActionView, View};

pub struct RootView {}

impl Entity for RootView {
    type Event = ();
}

impl View for RootView {
    fn ui_name() -> &'static str {
        "PureWarpRootView"
    }

    fn render(&self, _: &AppContext) -> Box<dyn Element> {
        Rect::new().with_background_color(ColorU::black()).finish()
    }
}

impl TypedActionView for RootView {
    type Action = ();
}
