//! Resolve image sources using the same configured asset and HTTP clients as GPUI's img element.
use super::*;
use gpdoc_core::{AtomImage, AtomImageSource};
use gpui::{ImgResourceLoader, RenderImage, Resource};
use std::{collections::HashMap, sync::Arc};

pub(super) type Images = HashMap<AtomImageSource, Arc<RenderImage>>;

pub(super) fn load(view: &Entity<EditorView>, window: &mut Window, cx: &mut App) -> Images {
    let sources: std::collections::BTreeSet<_> = view
        .read(cx)
        .buffer
        .formats()
        .iter()
        .filter_map(|s| s.format.atom_image.as_ref().map(|i| i.source.clone()))
        .collect();
    sources
        .into_iter()
        .filter_map(|source| {
            let data = match &source {
                AtomImageSource::Resource(uri) => {
                    let resource = if uri.starts_with("https://") || uri.starts_with("http://") {
                        Resource::Uri(uri.to_string().into())
                    } else if !uri.contains(':') && !uri.starts_with("//") {
                        Resource::Embedded(uri.trim_start_matches('/').to_owned().into())
                    } else {
                        return None;
                    };
                    window.use_asset::<ImgResourceLoader>(&resource, cx)?.ok()?
                }
                AtomImageSource::Formula { latex, display } => {
                    window.use_asset::<FormulaAsset>(&(latex.clone(), *display), cx)??
                }
            };
            Some((source, data))
        })
        .collect()
}

/// GPUI caches the resulting image by formula and display style, including failed parses.
struct FormulaAsset;
impl gpui::Asset for FormulaAsset {
    type Source = (Arc<str>, bool);
    type Output = Option<Arc<RenderImage>>;

    fn load(
        (latex, display): Self::Source,
        cx: &mut App,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        let renderer = cx.svg_renderer();
        async move {
            let svg = formula_svg(&latex, display)?;
            gpui::Image::from_bytes(gpui::ImageFormat::Svg, svg.into_bytes())
                .to_image_data(renderer)
                .ok()
        }
    }
}

fn formula_svg(latex: &str, display: bool) -> Option<String> {
    static FONT: std::sync::OnceLock<latex_rust::MathFont> = std::sync::OnceLock::new();
    let font = FONT
        .get_or_init(|| latex_rust::MathFont::stix_two_math().expect("bundled STIX Two Math font"));
    latex_rust::latex_to_svg(
        latex,
        font,
        &latex_rust::SvgOptions {
            display,
            ..Default::default()
        },
    )
    .ok()
}

pub(super) fn dimensions(
    spec: &AtomImage,
    data: &RenderImage,
    available: Option<Pixels>,
) -> Size<Pixels> {
    let intrinsic = data.size(0);
    let ratio = intrinsic.width.0 as f32 / intrinsic.height.0.max(1) as f32;
    let w = spec
        .width
        .map(|n| n as f32)
        .or_else(|| spec.height.map(|h| h as f32 * ratio))
        .unwrap_or(intrinsic.width.0 as f32);
    let h = spec.height.map_or(w / ratio, |n| n as f32);
    let scale = available.map_or(1., |limit| (limit / px(w)).clamp(0., 1.));
    size(px(w * scale), px(h * scale))
}

#[cfg(all(test, feature = "native-tests"))]
mod tests {
    use super::*;

    #[gpui::test]
    async fn typesets_a_display_fraction_and_falls_back_for_unknown_latex(
        cx: &mut gpui::TestAppContext,
    ) {
        let load = |cx: &gpui::TestAppContext, latex: &str, display| {
            cx.update(|app| app.fetch_asset::<FormulaAsset>(&(latex.into(), display)).0)
        };
        let inline = load(cx, r"\frac{1}{2}", false).await.unwrap();
        let display = load(cx, r"\frac{1}{2}", true).await.unwrap();
        assert!(display.size(0).height > inline.size(0).height);
        assert!(
            display
                .as_bytes(0)
                .unwrap()
                .chunks_exact(4)
                .any(|pixel| pixel[3] != 0)
        );
        assert!(load(cx, r"\unknowngpdoccommand", false).await.is_none());
    }
}
