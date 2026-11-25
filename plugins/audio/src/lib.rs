#[cfg(feature = "gpui")]
mod view;
#[cfg(feature = "gpui")]
pub use view::{AudioNode, audio_content, audio_controls};

#[cfg(feature = "gpui")]
use gpdoc::PluginViewExt;
use gpdoc_plugin::atom::{atom, insert_atom};
use gpdoc_plugin::prelude::{
    AttrSpec, AttrSpecs, AttrType, Attrs, Command, Node, Placement, Plugin, Value,
};
use regex::Regex;

#[derive(Clone, Debug)]
pub struct AudioOptions {
    pub placement: Placement,
    pub allow_base64: bool,
    pub controls: bool,
    pub autoplay: bool,
    pub repeat: bool,
    pub muted: bool,
    pub preload: String,
    pub disable_remote_playback: bool,
}

impl Default for AudioOptions {
    fn default() -> Self {
        Self {
            placement: Placement::Block,
            allow_base64: false,
            controls: true,
            autoplay: false,
            repeat: false,
            muted: false,
            preload: "metadata".into(),
            disable_remote_playback: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Set {
    pub src: String,
    pub preload: Option<String>,
    pub controls_list: Option<String>,
    pub cross_origin: Option<String>,
    pub controls: Option<bool>,
    pub autoplay: Option<bool>,
    pub repeat: Option<bool>,
    pub muted: Option<bool>,
    pub disable_remote_playback: Option<bool>,
}

impl Command for Set {
    const NAME: &'static str = "setAudio";
}

impl Set {
    fn into_attrs(self) -> Attrs {
        let mut attrs = Attrs::from([("src".into(), self.src.into())]);
        if let Some(value) = self.preload {
            attrs.insert("preload".into(), value.into());
        }
        if let Some(value) = self.controls_list {
            attrs.insert("controlslist".into(), value.into());
        }
        if let Some(value) = self.cross_origin {
            attrs.insert("crossorigin".into(), value.into());
        }
        if let Some(value) = self.controls {
            attrs.insert("controls".into(), value.into());
        }
        if let Some(value) = self.autoplay {
            attrs.insert("autoplay".into(), value.into());
        }
        if let Some(value) = self.repeat {
            attrs.insert("loop".into(), value.into());
        }
        if let Some(value) = self.muted {
            attrs.insert("muted".into(), value.into());
        }
        if let Some(value) = self.disable_remote_playback {
            attrs.insert("disableremoteplayback".into(), value.into());
        }
        attrs
    }
}

fn audio_label(attrs: &Attrs) -> String {
    match attrs.get("src").and_then(Value::as_str) {
        Some(src) if !src.is_empty() && !src.starts_with("data:") => format!("▶ {src}"),
        _ => "▶ Audio".into(),
    }
}

gpdoc_plugin::plugin_config! {
    pub Config {
        paste_rules PasteRules { audio_url: paste }
    }
}

pub fn audio(options: AudioOptions) -> Plugin<Config> {
    let mut attrs = AttrSpecs::from([
        (
            "src".into(),
            AttrSpec::with_default(AttrType::NullableString, Value::Null),
        ),
        (
            "preload".into(),
            AttrSpec::with_default(AttrType::String, Value::String(options.preload.clone())),
        ),
    ]);
    for key in ["controlslist", "crossorigin"] {
        attrs.insert(key.into(), AttrSpec::optional(AttrType::NullableString));
    }
    for (key, flag) in [
        ("controls", options.controls),
        ("autoplay", options.autoplay),
        ("loop", options.repeat),
        ("muted", options.muted),
        ("disableremoteplayback", options.disable_remote_playback),
    ] {
        attrs.insert(
            key.into(),
            AttrSpec::with_default(AttrType::Bool, Value::Bool(flag)),
        );
    }
    let allow_base64 = options.allow_base64;
    let data_url = Regex::new(r"(?i)^data:audio/[a-z0-9.+-]+;base64,").unwrap();
    let paste = Regex::new(
        r"(?i)https?://[^\s]+?\.(?:mp3|wav|ogg|oga|flac|m4a|aac|opus|weba|webm)(?:\?[^\s]*)?",
    )
    .unwrap();
    let plugin = Plugin::<Config>::new("audio", |p| {
        p.node(atom("audio", options.placement, attrs));
        p.atom_label("audio", audio_label);
        p.paste_rules()
            .audio_url()
            .set(gpdoc_plugin::atom::atom_paste_rule("audio", paste, |m| {
                Some((
                    0,
                    [("src".into(), Value::String(m.get(0)?.as_str().into()))].into(),
                ))
            }));

        p.command::<Set>(move |ctx, args| {
            let src = &args.src;
            let valid = src.starts_with('/')
                || src.starts_with("./")
                || src.starts_with("../")
                || (allow_base64 && data_url.is_match(src))
                || url::Url::parse(src).is_ok_and(|url| matches!(url.scheme(), "http" | "https"));
            if !valid {
                return Ok(None);
            }
            insert_atom(
                ctx,
                Node::leaf("audio", args.into_attrs()),
                ctx.selection().bounds(ctx.document()),
            )
        });
    });
    #[cfg(feature = "gpui")]
    let plugin = plugin.gpui_default_view(audio_content);
    plugin
}
